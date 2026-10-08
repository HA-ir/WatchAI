#![allow(dead_code)]

use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use watchai_core::session::SessionLifecycleEvent;
use watchai_ipc::protocol::SessionDto;

/// RAII process guard that guarantees termination and reaping of child process groups
/// upon Drop (preventing leaked zombie processes in CI and local test runs).
pub struct ChildGuard {
    child: Option<Child>,
    pgid: Pid,
}

impl ChildGuard {
    pub fn new(child: Child) -> Self {
        let pid = child.id() as i32;
        Self {
            child: Some(child),
            pgid: Pid::from_raw(pid),
        }
    }

    pub fn id(&self) -> u32 {
        self.child.as_ref().map(|c| c.id()).unwrap_or(0)
    }

    pub fn send_signal(&self, sig: Signal) -> std::io::Result<()> {
        killpg(self.pgid, sig).map_err(|e| std::io::Error::other(e.to_string()))
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        if let Some(child) = self.child.as_mut() {
            child.try_wait()
        } else {
            Ok(None)
        }
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        if let Some(mut child) = self.child.take() {
            child.wait()
        } else {
            Err(std::io::Error::other("Child already waited"))
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // 1. Send SIGTERM to entire process group
            let _ = killpg(self.pgid, Signal::SIGTERM);

            // 2. Poll for termination up to 500ms
            let start = Instant::now();
            let mut exited = false;
            while start.elapsed() < Duration::from_millis(500) {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        exited = true;
                        break;
                    }
                    _ => std::thread::sleep(Duration::from_millis(20)),
                }
            }

            // 3. Fall back to SIGKILL if still active
            if !exited {
                let _ = killpg(self.pgid, Signal::SIGKILL);
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// Lightweight RAII directory guard that removes itself upon Drop.
pub struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("watchai-{}-{}", prefix, nanos));
        fs::create_dir_all(&path).expect("Failed to create temporary directory");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Resolves the absolute path to `watchai-daemon` executable.
pub fn resolve_daemon_binary() -> PathBuf {
    if let Ok(p) = std::env::var("WATCHAI_DAEMON_BIN") {
        return PathBuf::from(p);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let debug = root.join("target/debug/watchai-daemon");
    if debug.exists() {
        return debug;
    }
    let release = root.join("target/release/watchai-daemon");
    if release.exists() {
        return release;
    }
    debug
}

/// Resolves the absolute path to `watchai-mock` executable.
pub fn resolve_mock_binary() -> PathBuf {
    if let Ok(p) = std::env::var("WATCHAI_MOCK_BIN") {
        return PathBuf::from(p);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let debug = root.join("target/debug/watchai-mock");
    if debug.exists() {
        return debug;
    }
    let release = root.join("target/release/watchai-mock");
    if release.exists() {
        return release;
    }
    debug
}

/// Typed D-Bus client proxy interface for WatchAI daemon.
#[zbus::proxy(
    interface = "org.freedesktop.WatchAI",
    default_service = "org.freedesktop.WatchAI",
    default_path = "/org/freedesktop/WatchAI"
)]
pub trait WatchAiService {
    fn get_aggregate_state(&self) -> zbus::Result<(String, u32, u32, u32, String)>;
    fn get_sessions(&self) -> zbus::Result<Vec<SessionDto>>;
    fn get_session(&self, session_id: String) -> zbus::Result<SessionDto>;

    #[zbus(signal)]
    fn session_added(&self, session: SessionDto) -> zbus::Result<()>;

    #[zbus(signal)]
    fn session_updated(&self, session: SessionDto) -> zbus::Result<()>;

    #[zbus(signal)]
    fn session_removed(&self, session_id: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn aggregate_state_changed(
        &self,
        state: String,
        active_session_count: u32,
        waiting_session_count: u32,
        error_session_count: u32,
        updated_at: String,
    ) -> zbus::Result<()>;
}

/// Comprehensive E2E Test Fixture isolating D-Bus, filesystem, and daemon child processes.
pub struct E2eTestFixture {
    pub temp_dir: TempDirGuard,
    pub proc_dir: PathBuf,
    pub dbus_address: String,
    pub dbus_daemon: ChildGuard,
    pub test_socket_path: PathBuf,
    pub daemon_child: Option<ChildGuard>,
    pub connection: zbus::Connection,
    pub stdout_capture: Arc<Mutex<Vec<u8>>>,
    pub stderr_capture: Arc<Mutex<Vec<u8>>>,
}

impl E2eTestFixture {
    /// Bootstraps an isolated test fixture with a private `dbus-daemon` and `watchai-daemon`.
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDirGuard::new("e2e-fixture");
        let proc_dir = temp_dir.path().join("proc");
        fs::create_dir_all(&proc_dir).expect("Failed to create mock proc directory");
        let test_socket_path = temp_dir.path().join("watchai-test.sock");
        let test_telemetry_socket_path = temp_dir.path().join("watchai-telemetry.sock");

        // 1. Spawn isolated dbus-daemon session bus
        let mut dbus_cmd = Command::new("dbus-daemon");
        dbus_cmd
            .arg("--session")
            .arg("--print-address")
            .arg("--nofork")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        unsafe {
            dbus_cmd.pre_exec(|| {
                nix::unistd::setpgid(Pid::from_raw(0), Pid::from_raw(0))
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(())
            });
        }

        let mut dbus_proc = dbus_cmd.spawn().expect("Failed to spawn dbus-daemon");
        let dbus_stdout = dbus_proc
            .stdout
            .take()
            .expect("Failed to capture dbus-daemon stdout");
        let mut reader = BufReader::new(dbus_stdout);
        let mut dbus_address = String::new();
        reader
            .read_line(&mut dbus_address)
            .expect("Failed to read dbus address");
        let dbus_address = dbus_address.trim().to_string();

        let dbus_daemon = ChildGuard::new(dbus_proc);

        // 2. Launch real production watchai-daemon under isolated environment
        let daemon_bin = resolve_daemon_binary();
        assert!(
            daemon_bin.exists(),
            "watchai-daemon binary must exist at {:?}",
            daemon_bin
        );

        let mut daemon_cmd = Command::new(&daemon_bin);
        daemon_cmd
            .env("DBUS_SESSION_BUS_ADDRESS", &dbus_address)
            .env("WATCHAI_TEST_SOCKET", &test_socket_path)
            .env("WATCHAI_TELEMETRY_SOCKET", &test_telemetry_socket_path)
            .env("WATCHAI_PROC_ROOT", &proc_dir)
            .env("RUST_LOG", "debug")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        unsafe {
            daemon_cmd.pre_exec(|| {
                nix::unistd::setpgid(Pid::from_raw(0), Pid::from_raw(0))
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(())
            });
        }

        let mut daemon_proc = daemon_cmd.spawn().expect("Failed to spawn watchai-daemon");

        let stdout_capture = Arc::new(Mutex::new(Vec::new()));
        let stderr_capture = Arc::new(Mutex::new(Vec::new()));

        let d_stdout = daemon_proc
            .stdout
            .take()
            .expect("Failed to capture daemon stdout");
        let d_stderr = daemon_proc
            .stderr
            .take()
            .expect("Failed to capture daemon stderr");

        let out_buf = stdout_capture.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(d_stdout);
            let mut chunk = [0u8; 1024];
            while let Ok(n) = r.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                out_buf.lock().unwrap().extend_from_slice(&chunk[..n]);
            }
        });

        let err_buf = stderr_capture.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(d_stderr);
            let mut chunk = [0u8; 1024];
            while let Ok(n) = r.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                err_buf.lock().unwrap().extend_from_slice(&chunk[..n]);
            }
        });

        let daemon_child = ChildGuard::new(daemon_proc);

        // 3. Connect zbus client connection to the ephemeral bus
        let connection = zbus::connection::Builder::address(dbus_address.as_str())?
            .build()
            .await?;

        let fixture = Self {
            temp_dir,
            proc_dir,
            dbus_address,
            dbus_daemon,
            test_socket_path,
            daemon_child: Some(daemon_child),
            connection,
            stdout_capture,
            stderr_capture,
        };

        // 4. Bounded wait for daemon D-Bus name acquisition (2.0s ceiling)
        fixture.wait_for_dbus_ready(Duration::from_secs(2)).await?;

        Ok(fixture)
    }

    /// Obtains a typed proxy for WatchAI D-Bus methods and signals.
    pub async fn proxy(&self) -> Result<WatchAiServiceProxy<'_>, zbus::Error> {
        WatchAiServiceProxy::new(&self.connection).await
    }

    /// Waits up to `timeout` for daemon to claim D-Bus name and respond to method calls.
    pub async fn wait_for_dbus_ready(&self, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        let proxy = WatchAiServiceProxy::new(&self.connection)
            .await
            .map_err(|e| format!("Proxy creation error: {e}"))?;

        while start.elapsed() < timeout {
            if proxy.get_aggregate_state().await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        self.dump_diagnostics();
        Err(format!(
            "Timed out after {:?} waiting for daemon D-Bus service readiness",
            timeout
        ))
    }

    /// Direct event transmission over the test socket.
    pub async fn send_mock_event(
        &self,
        event: &SessionLifecycleEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = UnixStream::connect(&self.test_socket_path).await?;
        let mut json = serde_json::to_string(event)?;
        json.push('\n');
        stream.write_all(json.as_bytes()).await?;
        stream.flush().await?;
        Ok(())
    }

    /// Executes `watchai-mock` CLI binary targeting this fixture's test socket.
    pub fn run_mock_cli(&self, args: &[&str]) -> Result<String, String> {
        let mock_bin = resolve_mock_binary();
        assert!(
            mock_bin.exists(),
            "watchai-mock binary must exist at {:?}",
            mock_bin
        );

        let mut cmd = Command::new(&mock_bin);
        cmd.arg("--socket").arg(&self.test_socket_path);
        for a in args {
            cmd.arg(a);
        }
        cmd.env("WATCHAI_TEST_SOCKET", &self.test_socket_path);
        cmd.env("WATCHAI_PROC_ROOT", &self.proc_dir);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute watchai-mock: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(format!(
                "watchai-mock failed (code {:?}): stdout: {}, stderr: {}",
                output.status.code(),
                stdout,
                stderr
            ));
        }

        Ok(stdout)
    }

    /// Sends `SIGTERM` to `watchai-daemon` and waits for bounded graceful shutdown.
    pub async fn stop_daemon(&mut self, timeout: Duration) -> Result<ExitStatus, String> {
        if let Some(mut child) = self.daemon_child.take() {
            child
                .send_signal(Signal::SIGTERM)
                .map_err(|e| format!("Failed to send SIGTERM: {e}"))?;

            let start = Instant::now();
            while start.elapsed() < timeout {
                if let Ok(Some(status)) = child.try_wait() {
                    return Ok(status);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            self.dump_diagnostics();
            Err(format!(
                "Daemon failed to shut down within {:?} after SIGTERM",
                timeout
            ))
        } else {
            Err("Daemon child already terminated".to_string())
        }
    }

    /// Dumps captured stdout and stderr diagnostics for post-mortem analysis.
    pub fn dump_diagnostics(&self) {
        let out = self.stdout_capture.lock().unwrap();
        let err = self.stderr_capture.lock().unwrap();
        eprintln!("\n=== WATCHAI-DAEMON STDOUT DIAGNOSTICS ===");
        eprintln!("{}", String::from_utf8_lossy(&out));
        eprintln!("\n=== WATCHAI-DAEMON STDERR DIAGNOSTICS ===");
        eprintln!("{}", String::from_utf8_lossy(&err));
        eprintln!("=========================================\n");
    }
}
