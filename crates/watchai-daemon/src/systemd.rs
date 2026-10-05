use std::env;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use tracing::debug;

/// Systemd notify manager for user services.
/// Interacts with systemd via the `NOTIFY_SOCKET` Unix datagram mechanism.
#[derive(Debug, Clone)]
pub struct SystemdNotifier {
    socket_path: Option<String>,
}

impl Default for SystemdNotifier {
    fn default() -> Self {
        Self::from_env()
    }
}

impl SystemdNotifier {
    /// Detect systemd notification socket from `NOTIFY_SOCKET` environment variable.
    pub fn from_env() -> Self {
        Self {
            socket_path: env::var("NOTIFY_SOCKET").ok(),
        }
    }

    /// Construct an unconfigured/disabled notifier (for testing non-systemd fallback).
    pub fn disabled() -> Self {
        Self { socket_path: None }
    }

    /// Construct notifier with an explicit socket path (ideal for testing).
    pub fn with_socket(path: impl Into<String>) -> Self {
        Self {
            socket_path: Some(path.into()),
        }
    }

    /// Returns true if running under a systemd notification environment.
    pub fn is_available(&self) -> bool {
        self.socket_path.is_some()
    }

    /// Retrieve the configured notification socket path.
    pub fn socket_path(&self) -> Option<&str> {
        self.socket_path.as_deref()
    }

    /// Send a raw state string to the systemd notification socket.
    /// Returns:
    /// - `Ok(true)` if notification was successfully sent.
    /// - `Ok(false)` if `NOTIFY_SOCKET` is unset (safe fallback outside systemd).
    /// - `Err(e)` on socket I/O failure.
    pub fn notify(&self, state: &str) -> io::Result<bool> {
        let path_str = match &self.socket_path {
            Some(p) => p,
            None => return Ok(false),
        };

        let sock = UnixDatagram::unbound()?;

        #[cfg(target_os = "linux")]
        if let Some(abstract_name) = path_str.strip_prefix('@') {
            use std::os::linux::net::SocketAddrExt;
            let addr = std::os::unix::net::SocketAddr::from_abstract_name(abstract_name)?;
            sock.connect_addr(&addr)?;
            sock.send(state.as_bytes())?;
            return Ok(true);
        }

        sock.send_to(state.as_bytes(), Path::new(path_str))?;
        Ok(true)
    }

    /// Notify systemd that service initialization is complete and D-Bus name is acquired.
    pub fn notify_ready(&self) -> io::Result<bool> {
        debug!("Sending systemd notification: READY=1");
        self.notify("READY=1")
    }

    /// Notify systemd of an updated status message.
    pub fn notify_status(&self, status: &str) -> io::Result<bool> {
        self.notify(&format!("STATUS={}", status))
    }

    /// Notify systemd that service is beginning graceful termination.
    pub fn notify_stopping(&self) -> io::Result<bool> {
        debug!("Sending systemd notification: STOPPING=1");
        self.notify("STOPPING=1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_notifier_returns_false_when_unset() {
        let notifier = SystemdNotifier::disabled();
        assert!(!notifier.is_available());
        assert!(!notifier.notify_ready().unwrap());
        assert!(!notifier.notify_stopping().unwrap());
        assert!(!notifier.notify_status("Test").unwrap());
    }

    #[test]
    fn test_notifier_sends_ready_and_stopping_to_unix_datagram() {
        let temp_dir = std::env::temp_dir().join(format!("watchai-sd-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let sock_path = temp_dir.join("notify.sock");

        // Bind receiving socket
        let receiver = UnixDatagram::bind(&sock_path).unwrap();

        let notifier = SystemdNotifier::with_socket(sock_path.to_str().unwrap());
        assert!(notifier.is_available());

        // 1. Send READY=1
        let sent_ready = notifier.notify_ready().unwrap();
        assert!(sent_ready);

        let mut buf = [0u8; 64];
        let (len, _) = receiver.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"READY=1");

        // 2. Send STATUS=Running
        let sent_status = notifier.notify_status("Running").unwrap();
        assert!(sent_status);

        let (len, _) = receiver.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"STATUS=Running");

        // 3. Send STOPPING=1
        let sent_stopping = notifier.notify_stopping().unwrap();
        assert!(sent_stopping);

        let (len, _) = receiver.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"STOPPING=1");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
