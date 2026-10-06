mod harness;

use chrono::DateTime;
use harness::E2eTestFixture;
use std::time::Duration;

#[tokio::test]
async fn test_scenario1_daemon_startup_and_dbus_registration() {
    // T160 / Scenario 1: Daemon startup, D-Bus registration, initial IDLE state, and graceful SIGTERM shutdown
    let mut fixture = E2eTestFixture::start()
        .await
        .expect("E2E fixture startup must succeed within 2.0s");

    let conn = fixture.connection.clone();
    let proxy = harness::WatchAiServiceProxy::new(&conn)
        .await
        .expect("Failed to acquire D-Bus proxy");

    // 1. Assert initial aggregate state
    let (state, active, waiting, error, updated_at) = proxy
        .get_aggregate_state()
        .await
        .expect("GetAggregateState call failed");

    assert_eq!(state, "IDLE", "Initial state must be IDLE");
    assert_eq!(active, 0, "Initial active count must be 0");
    assert_eq!(waiting, 0, "Initial waiting count must be 0");
    assert_eq!(error, 0, "Initial error count must be 0");
    assert!(
        DateTime::parse_from_rfc3339(&updated_at).is_ok(),
        "updated_at must be RFC3339 timestamp: {}",
        updated_at
    );

    // 2. Assert initial sessions list is empty
    let sessions = proxy.get_sessions().await.expect("GetSessions call failed");
    assert!(
        sessions.is_empty(),
        "Initial sessions vector must be empty: {:?}",
        sessions
    );

    // 3. Assert graceful shutdown on SIGTERM within 5.0s deadline
    let status = fixture
        .stop_daemon(Duration::from_secs(5))
        .await
        .expect("Daemon failed to shut down within 5.0s on SIGTERM");

    assert!(
        status.success(),
        "Daemon must exit with success (code 0) on SIGTERM: {:?}",
        status
    );

    // 4. Verify D-Bus service is no longer responding
    let call_after_shutdown = proxy.get_aggregate_state().await;
    assert!(
        call_after_shutdown.is_err(),
        "D-Bus calls must fail after daemon has released name and exited"
    );
}
