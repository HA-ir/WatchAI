use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::{debug, info, warn};
use watchai_adapters::claude_code::ProviderTelemetryPayload;
use watchai_adapters::registry::AdapterRegistry;
use watchai_core::session::SessionRegistry;

/// Maximum line length permitted over the production telemetry socket (64 KiB)
/// to prevent unbounded memory allocation from adversarial or corrupted inputs.
pub const MAX_TELEMETRY_LINE_BYTES: usize = 64 * 1024;

/// Resolves the effective Unix domain socket path for production telemetry events.
///
/// Priority:
/// 1. `WATCHAI_TELEMETRY_SOCKET` environment variable (for testing and isolated harnesses).
/// 2. `$XDG_RUNTIME_DIR/watchai/events.sock` (standard user-private runtime directory).
/// 3. `/tmp/watchai-<UID>/events.sock` (secure UID-isolated fallback).
pub fn resolve_telemetry_socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("WATCHAI_TELEMETRY_SOCKET") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let trimmed = runtime_dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("watchai").join("events.sock");
        }
    }

    let uid = nix::unistd::getuid().as_raw();
    PathBuf::from(format!("/tmp/watchai-{}/events.sock", uid))
}

/// Prepares the parent directory for the socket file with strict 0700 permissions.
pub fn prepare_socket_dir(socket_path: &Path) -> std::io::Result<()> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(parent)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(parent, perms)?;
        }
    }
    Ok(())
}

/// Removes stale socket file if present on daemon startup.
pub fn remove_stale_socket(socket_path: &Path) -> std::io::Result<()> {
    if socket_path.exists() {
        debug!("Removing stale telemetry socket file at {:?}", socket_path);
        fs::remove_file(socket_path)?;
    }
    Ok(())
}

/// Binds the Unix listener and applies strict 0600 file permissions.
pub fn bind_telemetry_listener(socket_path: &Path) -> std::io::Result<UnixListener> {
    prepare_socket_dir(socket_path)?;
    remove_stale_socket(socket_path)?;

    let listener = UnixListener::bind(socket_path)?;
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(socket_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(socket_path, perms)?;
    }

    Ok(listener)
}

/// Runs the asynchronous telemetry socket listener loop until shutdown is signaled.
pub async fn run_telemetry_listener(
    socket_path: PathBuf,
    adapters: Arc<AdapterRegistry>,
    registry: SessionRegistry,
    mut shutdown_rx: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let listener = bind_telemetry_listener(&socket_path)?;
    info!("Production telemetry socket bound at {:?}", socket_path);

    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((stream, _)) => {
                        let adapters = adapters.clone();
                        let registry = registry.clone();
                        tokio::spawn(async move {
                            handle_telemetry_client(stream, adapters, registry).await;
                        });
                    }
                    Err(e) => {
                        warn!("Telemetry listener accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                info!("Telemetry listener received shutdown signal. Terminating loop cleanly.");
                break;
            }
        }
    }

    // Clean up socket file on clean shutdown
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    Ok(())
}

/// Handles an incoming telemetry client connection over the Unix stream socket.
pub async fn handle_telemetry_client(
    stream: UnixStream,
    adapters: Arc<AdapterRegistry>,
    registry: SessionRegistry,
) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // Clean EOF
            Ok(bytes_read) => {
                if bytes_read > MAX_TELEMETRY_LINE_BYTES {
                    warn!(
                        "Discarded oversized telemetry line ({} bytes > {} max)",
                        bytes_read, MAX_TELEMETRY_LINE_BYTES
                    );
                    break;
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<ProviderTelemetryPayload>(trimmed) {
                    Ok(payload) => {
                        let provider_id = payload.effective_provider_id();
                        let res = match provider_id {
                            "opencode" => {
                                if let Some(adapter) = adapters.opencode_adapter() {
                                    adapter.handle_telemetry(&registry, payload).await
                                } else {
                                    warn!("No OpenCode adapter registered for telemetry");
                                    Ok(None)
                                }
                            }
                            "codex-cli" => {
                                if let Some(adapter) = adapters.codex_adapter() {
                                    adapter.handle_telemetry(&registry, payload).await
                                } else {
                                    warn!("No Codex CLI adapter registered for telemetry");
                                    Ok(None)
                                }
                            }
                            _ => {
                                if let Some(adapter) = adapters.claude_adapter() {
                                    adapter.handle_telemetry(&registry, payload).await
                                } else {
                                    warn!("No Claude Code adapter registered for telemetry");
                                    Ok(None)
                                }
                            }
                        };
                        if let Err(e) = res {
                            warn!("Failed to ingest telemetry payload: {:?}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Malformed JSON rejected on telemetry socket: {}", e);
                    }
                }
            }
            Err(e) => {
                debug!("Telemetry client read error: {}", e);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use watchai_adapters::traits::EventSink;
    use watchai_core::session::AgentSession;
    use watchai_core::state::LifecycleState;

    #[test]
    fn test_resolve_telemetry_socket_path() {
        std::env::set_var("WATCHAI_TELEMETRY_SOCKET", "/tmp/custom-watchai-test.sock");
        let path = resolve_telemetry_socket_path();
        assert_eq!(path, PathBuf::from("/tmp/custom-watchai-test.sock"));
        std::env::remove_var("WATCHAI_TELEMETRY_SOCKET");

        let path_default = resolve_telemetry_socket_path();
        assert!(
            path_default.to_string_lossy().contains("watchai")
                && path_default.to_string_lossy().ends_with("events.sock")
        );
    }

    #[tokio::test]
    async fn test_telemetry_socket_bind_and_permissions() {
        let temp_dir =
            std::env::temp_dir().join(format!("watchai-sock-test-{}", std::process::id()));
        let sock_path = temp_dir.join("test-events.sock");

        let listener = bind_telemetry_listener(&sock_path).expect("Failed to bind socket");

        // Verify directory permissions 0700
        let dir_meta = fs::metadata(&temp_dir).unwrap();
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);

        // Verify socket file permissions 0600
        let sock_meta = fs::metadata(&sock_path).unwrap();
        assert_eq!(sock_meta.permissions().mode() & 0o777, 0o600);

        drop(listener);
        let _ = fs::remove_file(&sock_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_telemetry_socket_end_to_end_transmission() {
        let temp_dir =
            std::env::temp_dir().join(format!("watchai-e2e-sock-{}", std::process::id()));
        let sock_path = temp_dir.join("events.sock");

        let mut adapters = AdapterRegistry::default_registry();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        adapters.attach_event_sink(EventSink::new(tx, None));
        let adapters_arc = Arc::new(adapters);

        let registry = SessionRegistry::new();
        let session = AgentSession::new(
            "proc-e2e-session".to_string(),
            "claude-code",
            "Claude Code",
            "/home/user/project",
            Some(9999),
            LifecycleState::Idle,
            watchai_core::session::AdapterStatus::DiscoveryRequired,
        );
        registry.upsert(session).await;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server_path = sock_path.clone();
        let srv_adapters = adapters_arc.clone();
        let srv_registry = registry.clone();

        let srv_handle = tokio::spawn(async move {
            run_telemetry_listener(server_path, srv_adapters, srv_registry, shutdown_rx).await
        });

        // Yield to allow server to bind
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Send telemetry payload from mock client
        let mut client = std::os::unix::net::UnixStream::connect(&sock_path).unwrap();
        let payload = ProviderTelemetryPayload {
            pid: 9999,
            provider_id: Some("claude-code".to_string()),
            claude_session_id: Some("uuid-123".to_string()),
            cwd: Some("/home/user/project".to_string()),
            hook_event: "UserPromptSubmit".to_string(),
            tool_name: None,
            exit_reason: None,
        };
        let line = serde_json::to_string(&payload).unwrap() + "\n";
        client.write_all(line.as_bytes()).unwrap();
        drop(client);

        // Await event in channel receiver
        let event = rx.recv().await.unwrap();
        assert_eq!(event.session_id(), "proc-e2e-session");

        // Stop server
        shutdown_tx.send(true).unwrap();
        let _ = srv_handle.await;

        // Verify socket file was cleaned up on shutdown
        assert!(!sock_path.exists());
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
