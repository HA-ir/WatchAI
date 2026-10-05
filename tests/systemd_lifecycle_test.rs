use std::fs;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use watchai_core::session::SessionRegistry;
use watchai_daemon::logging::{is_sensitive, redact_sensitive_text, RedactingWriter};
use watchai_daemon::systemd::SystemdNotifier;
use watchai_ipc::dbus_service::{WatchAiDbusService, BUS_NAME, OBJECT_PATH};

#[test]
fn test_systemd_user_service_unit_file_contract() {
    // T058: Contract verification for systemd/watchai.service
    let root_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Must have workspace root parent directory");
    let service_path = root_dir.join("systemd/watchai.service");
    assert!(
        service_path.exists(),
        "systemd/watchai.service unit file must exist at {:?}",
        service_path
    );

    let content =
        fs::read_to_string(&service_path).expect("Failed to read systemd/watchai.service");

    // 1. Mandatory sections
    assert!(
        content.contains("[Unit]"),
        "Unit file must have [Unit] section"
    );
    assert!(
        content.contains("[Service]"),
        "Unit file must have [Service] section"
    );
    assert!(
        content.contains("[Install]"),
        "Unit file must have [Install] section"
    );

    // 2. D-Bus activation and naming
    assert!(
        content.contains("Type=dbus"),
        "Service must declare Type=dbus for D-Bus synchronization"
    );
    assert!(
        content.contains(&format!("BusName={}", BUS_NAME)),
        "Service must declare BusName={}",
        BUS_NAME
    );

    // 3. Restart and slice policy
    assert!(
        content.contains("Restart=on-failure"),
        "Service must declare Restart=on-failure"
    );
    assert!(
        content.contains("RestartSec=2s"),
        "Service must declare RestartSec=2s"
    );
    assert!(
        content.contains("Slice=session.slice"),
        "Service must declare Slice=session.slice"
    );

    // 4. Graphical user session integration
    assert!(
        content.contains("PartOf=graphical-session.target"),
        "Service must be PartOf=graphical-session.target"
    );
    assert!(
        content.contains("WantedBy=graphical-session.target"),
        "Service must declare WantedBy=graphical-session.target"
    );

    // 5. Unprivileged execution invariants (Zero root / zero sudo requirement)
    assert!(
        !content.contains("User=root"),
        "Service must never run as root"
    );
    assert!(
        !content.contains("sudo"),
        "Service must never require or invoke sudo"
    );
    assert!(
        !content.contains("PermissionsStartOnly"),
        "Service must be a pure user-space unprivileged unit"
    );
}

#[test]
fn test_systemd_readiness_notification_lifecycle() {
    // T059: Systemd readiness notification ordering and socket dispatch
    let temp_dir = std::env::temp_dir().join(format!("watchai-test-sd-{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let sock_path = temp_dir.join("notify_test.sock");

    // Bind local receiver simulating systemd user manager
    let receiver = UnixDatagram::bind(&sock_path).expect("Failed to bind test notify socket");

    let notifier = SystemdNotifier::with_socket(sock_path.to_str().unwrap());
    assert!(notifier.is_available());

    // 1. Send status update before ready
    notifier
        .notify_status("Initializing adapters and scanning surviving sessions...")
        .unwrap();

    let mut buf = [0u8; 128];
    let (len, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(
        &buf[..len],
        b"STATUS=Initializing adapters and scanning surviving sessions..."
    );

    // 2. Send READY=1 after initialization complete
    notifier.notify_ready().unwrap();

    let (len, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..len], b"READY=1");

    // 3. Send STOPPING=1 during graceful shutdown
    notifier.notify_stopping().unwrap();

    let (len, _) = receiver.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..len], b"STOPPING=1");

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_systemd_notifier_fallback_when_unconfigured() {
    // T059: Outside of systemd, notify operations return Ok(false) without failing or panicking
    let notifier = SystemdNotifier::disabled();
    assert!(!notifier.is_available());
    assert!(!notifier.notify_ready().unwrap());
    assert!(!notifier.notify_status("test").unwrap());
    assert!(!notifier.notify_stopping().unwrap());
}

#[tokio::test]
async fn test_graceful_shutdown_releases_bus_name() {
    // T060: Verify that on shutdown, releasing the D-Bus bus name succeeds cleanly
    // and connection closing occurs without panic.
    let registry = SessionRegistry::new();
    let service = WatchAiDbusService::new(registry);

    // Request a test-isolated bus name
    let test_bus_name = format!(
        "org.freedesktop.WatchAI.TestShutdown.P{}",
        std::process::id()
    );

    let connection_res = zbus::connection::Builder::session();
    if connection_res.is_err() {
        // If headless CI has no D-Bus session bus, skip gracefully
        eprintln!("Skipping live D-Bus release test: session bus unavailable in this environment");
        return;
    }

    let connection = match connection_res
        .unwrap()
        .name(test_bus_name.as_str())
        .unwrap()
        .serve_at(OBJECT_PATH, service)
        .unwrap()
        .build()
        .await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Skipping live D-Bus release test: D-Bus connection failed: {}",
                e
            );
            return;
        }
    };

    // Verify name is owned
    assert!(connection.unique_name().is_some());

    // 1. Release bus name cleanly (T060)
    let release_res = connection.release_name(test_bus_name.as_str()).await;
    assert!(
        release_res.is_ok(),
        "Releasing D-Bus bus name on shutdown must succeed"
    );

    // 2. Close connection cleanly
    let close_res = connection.close().await;
    assert!(close_res.is_ok(), "Closing D-Bus connection must succeed");
}

#[test]
fn test_structured_logging_privacy_redaction() {
    // T061: Strict verification of prompt, token, and credential redaction
    assert!(is_sensitive("Contains sk-ant-api03-abcdef1234567890"));
    assert!(is_sensitive("Contains Bearer eyJhbGciOi..."));
    assert!(is_sensitive("Database password=secret123"));
    assert!(is_sensitive("api_key: my-secret-key"));
    assert!(is_sensitive("prompt=\"Show me the secret key\""));
    assert!(is_sensitive("diff --git a/file b/file"));

    // 1. API Keys
    let raw_key = "Loaded credentials from environment: sk-ant-api03-1234567890abcdef12345678";
    let cleaned_key = redact_sensitive_text(raw_key);
    assert!(!cleaned_key.contains("sk-ant-api03-1234567890abcdef12345678"));
    assert!(cleaned_key.contains("[REDACTED_API_KEY]"));

    // 2. Bearer tokens
    let raw_bearer = "Authorization header received: Bearer ya29.a0AfH6SMByz... token";
    let cleaned_bearer = redact_sensitive_text(raw_bearer);
    assert!(!cleaned_bearer.contains("ya29.a0AfH6SMByz"));
    assert!(cleaned_bearer.contains("Bearer [REDACTED_TOKEN]"));

    // 3. Prompt contents
    let raw_prompt = "Agent dispatched with prompt=\"Refactor authentication to bypass MFA\"";
    let cleaned_prompt = redact_sensitive_text(raw_prompt);
    assert!(!cleaned_prompt.contains("Refactor authentication to bypass MFA"));
    assert!(cleaned_prompt.contains("prompt=[REDACTED_PROMPT]"));

    // 4. Code diffs
    let raw_diff = "Patch applied:\ndiff --git a/keys.py b/keys.py\n--- a/keys.py\n+++ b/keys.py\n@@ -1 +1 @@\n-SECRET = 1\n+SECRET = 2\n";
    let cleaned_diff = redact_sensitive_text(raw_diff);
    assert!(!cleaned_diff.contains("SECRET = 1"));
    assert!(!cleaned_diff.contains("SECRET = 2"));
    assert!(cleaned_diff.contains("[REDACTED_CODE_DIFF]"));

    // 5. RedactingWriter stream verification
    let mut output = Vec::new();
    let mut writer = RedactingWriter::new(&mut output);
    use std::io::Write;
    writer
        .write_all(b"Info: session created token=xyz99999\n")
        .unwrap();

    let output_str = String::from_utf8(output).unwrap();
    assert!(!output_str.contains("xyz99999"));
    assert!(output_str.contains("token=[REDACTED]"));

    // 6. Operational metadata must NOT be corrupted
    let safe_log =
        "WatchAI Daemon is running. Monitoring agent sessions: active=2, waiting=1, error=0";
    assert_eq!(safe_log, redact_sensitive_text(safe_log));
}
