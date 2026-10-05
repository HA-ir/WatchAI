use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};
use watchai_core::session::SessionLifecycleEvent;

/// Maximum line length permitted over the test socket (64 KiB) to prevent unbounded memory allocation.
pub const MAX_TEST_SOCKET_LINE_BYTES: usize = 64 * 1024;

/// Reads a bounded line terminating with `\n`.
/// If the line exceeds `max_bytes` before encountering `\n`, excess bytes are discarded
/// until `\n` is reached, and an empty string is returned without unbounded allocation.
pub async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max_bytes: usize,
) -> std::io::Result<Option<String>> {
    buf.clear();
    let mut total_read = 0;
    let mut exceeded = false;

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if buf.is_empty() {
                return Ok(None); // Clean EOF
            }
            break;
        }

        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let take_len = pos + 1;
            if !exceeded && total_read + pos <= max_bytes {
                buf.extend_from_slice(&available[..pos]);
            } else {
                exceeded = true;
            }
            reader.consume(take_len);
            break;
        } else {
            let len = available.len();
            if !exceeded && total_read + len <= max_bytes {
                buf.extend_from_slice(available);
                total_read += len;
            } else {
                exceeded = true;
            }
            reader.consume(len);
        }
    }

    if exceeded {
        warn!("Discarded test socket input exceeding 64 KiB limit");
        return Ok(Some(String::new()));
    }

    let s = String::from_utf8_lossy(buf).trim().to_string();
    Ok(Some(s))
}

/// Handles an individual mock client connection over the Unix stream socket.
async fn handle_test_client(stream: UnixStream, event_tx: mpsc::Sender<SessionLifecycleEvent>) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::with_capacity(4096);

    loop {
        match read_bounded_line(&mut reader, &mut buf, MAX_TEST_SOCKET_LINE_BYTES).await {
            Ok(Some(line)) => {
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<SessionLifecycleEvent>(&line) {
                    Ok(event) => {
                        debug!("Ingesting event from test socket: {:?}", event.session_id());
                        if let Err(e) = event_tx.send(event).await {
                            warn!(
                                "Failed to forward test socket event to ingestion channel: {}",
                                e
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Malformed JSON rejected on test socket: {}", e);
                    }
                }
            }
            Ok(None) => {
                // Client closed connection cleanly
                break;
            }
            Err(e) => {
                debug!("Test socket client read error: {}", e);
                break;
            }
        }
    }
}

/// Binds a Unix domain socket listener at `socket_path` and forwards deserialized
/// `SessionLifecycleEvent` instances to `event_tx`.
///
/// Unlinks any stale socket file before binding, and removes the socket file upon clean shutdown.
pub async fn run_test_socket_listener(
    socket_path: PathBuf,
    event_tx: mpsc::Sender<SessionLifecycleEvent>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> std::io::Result<()> {
    // 1. Remove stale socket file if present before binding
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    // 2. Ensure parent directory exists with restricted 0700 permissions
    if let Some(parent) = socket_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }
    }

    // 3. Bind Unix listener
    info!("Binding test event socket at: {:?}", socket_path);
    let listener = UnixListener::bind(&socket_path)?;

    // Restrict test socket file to 0600 (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600));
    }

    // 4. Main accept loop with graceful shutdown monitoring
    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((stream, _)) => {
                        let tx = event_tx.clone();
                        tokio::spawn(async move {
                            handle_test_client(stream, tx).await;
                        });
                    }
                    Err(e) => {
                        warn!("Error accepting test socket connection: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                info!("Shutting down test event socket listener.");
                break;
            }
        }
    }

    // 5. Clean up socket file on shutdown
    if socket_path.exists() {
        let _ = fs::remove_file(&socket_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
    use watchai_core::state::LifecycleState;

    fn unique_socket_path(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("watchai-{}-{}.sock", label, nanos))
    }

    #[tokio::test]
    async fn test_socket_lifecycle_ingestion() {
        let socket_path = unique_socket_path("lifecycle");
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let listener_path = socket_path.clone();
        let listener_handle = tokio::spawn(async move {
            run_test_socket_listener(listener_path, event_tx, shutdown_rx).await
        });

        // Give listener a moment to bind
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(&socket_path)
            .await
            .expect("Failed to connect to test socket");

        let event = SessionLifecycleEvent::SessionRegistered {
            session_id: "test-s1".to_string(),
            provider_id: "claude-code".to_string(),
            provider_display_name: "Claude Code".to_string(),
            project_path: "/tmp/proj".to_string(),
            process_id: None,
            initial_state: LifecycleState::Starting,
            timestamp: Utc::now(),
        };

        let mut json = serde_json::to_string(&event).unwrap();
        json.push('\n');
        stream.write_all(json.as_bytes()).await.unwrap();

        let received = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("Timeout waiting for event")
            .expect("Event channel closed");

        assert_eq!(received.session_id(), "test-s1");

        // Shutdown listener
        let _ = shutdown_tx.send(true);
        let _ = listener_handle.await;

        // Verify socket file was unlinked
        assert!(
            !socket_path.exists(),
            "Socket file must be unlinked on shutdown"
        );
    }

    #[tokio::test]
    async fn test_socket_64k_line_limit_enforcement() {
        let socket_path = unique_socket_path("64k-limit");
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let listener_path = socket_path.clone();
        let listener_handle = tokio::spawn(async move {
            run_test_socket_listener(listener_path, event_tx, shutdown_rx).await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(&socket_path).await.unwrap();

        // 1. Send > 64 KiB line without newline
        let oversized = vec![b'a'; 70 * 1024];
        stream.write_all(&oversized).await.unwrap();
        stream.write_all(b"\n").await.unwrap();

        // 2. Send valid event afterwards
        let valid_event = SessionLifecycleEvent::Heartbeat {
            session_id: "valid-s1".to_string(),
            timestamp: Utc::now(),
        };
        let mut json = serde_json::to_string(&valid_event).unwrap();
        json.push('\n');
        stream.write_all(json.as_bytes()).await.unwrap();

        let received = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("Timeout waiting for valid event")
            .expect("Channel closed");

        assert_eq!(received.session_id(), "valid-s1");

        let _ = shutdown_tx.send(true);
        let _ = listener_handle.await;
    }

    #[tokio::test]
    async fn test_socket_malformed_json_recovery() {
        let socket_path = unique_socket_path("malformed");
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let listener_path = socket_path.clone();
        let listener_handle = tokio::spawn(async move {
            run_test_socket_listener(listener_path, event_tx, shutdown_rx).await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = UnixStream::connect(&socket_path).await.unwrap();

        // 1. Send invalid JSON
        stream.write_all(b"{ not valid json }\n").await.unwrap();

        // 2. Send valid event
        let valid_event = SessionLifecycleEvent::Heartbeat {
            session_id: "valid-s2".to_string(),
            timestamp: Utc::now(),
        };
        let mut json = serde_json::to_string(&valid_event).unwrap();
        json.push('\n');
        stream.write_all(json.as_bytes()).await.unwrap();

        let received = tokio::time::timeout(Duration::from_secs(1), event_rx.recv())
            .await
            .expect("Timeout waiting for event")
            .expect("Channel closed");

        assert_eq!(received.session_id(), "valid-s2");

        let _ = shutdown_tx.send(true);
        let _ = listener_handle.await;
    }

    #[tokio::test]
    async fn test_socket_permissions_restricted_to_owner() {
        let parent_dir = std::env::temp_dir().join(format!(
            "watchai-perm-dir-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let socket_path = parent_dir.join("test.sock");
        let (event_tx, _event_rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let listener_path = socket_path.clone();
        let listener_handle = tokio::spawn(async move {
            run_test_socket_listener(listener_path, event_tx, shutdown_rx).await
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let parent_meta = fs::metadata(&parent_dir).expect("Parent dir must exist");
            let parent_mode = parent_meta.permissions().mode() & 0o777;
            assert_eq!(
                parent_mode, 0o700,
                "Parent directory mode must be restricted to 0700"
            );

            let socket_meta = fs::metadata(&socket_path).expect("Socket file must exist");
            let socket_mode = socket_meta.permissions().mode() & 0o777;
            assert_eq!(
                socket_mode, 0o600,
                "Socket file mode must be restricted to 0600"
            );
        }

        let _ = shutdown_tx.send(true);
        let _ = listener_handle.await;
        let _ = fs::remove_dir_all(&parent_dir);
    }
}
