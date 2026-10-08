mod harness;

use harness::E2eTestFixture;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_scenario3_multi_session_priority_and_tie_breaking() {
    // T162 / Scenario 3: Multi-session priority hierarchy (ERROR > WAITING > WORKING), active counters, and contextual tie-breaking
    let fixture = E2eTestFixture::start()
        .await
        .expect("E2E fixture startup must succeed");

    let proxy = fixture
        .proxy()
        .await
        .expect("Failed to acquire D-Bus proxy");

    // 1. Session 1 (WORKING) + Session 2 (WAITING) -> Aggregate is WAITING
    fixture
        .run_mock_cli(&[
            "session",
            "start",
            "--id",
            "sess-agg-1",
            "--provider",
            "claude-code",
            "--state",
            "Working",
        ])
        .unwrap();

    fixture
        .run_mock_cli(&[
            "session",
            "start",
            "--id",
            "sess-agg-2",
            "--provider",
            "codex-cli",
            "--state",
            "Waiting",
        ])
        .unwrap();

    // Verify WAITING overrides WORKING: active=2, waiting=1, error=0
    let (state1, active1, working1, waiting1, success1, error1, _) =
        proxy.get_aggregate_state().await.unwrap();
    assert_eq!(state1, "WAITING", "WAITING must override WORKING");
    assert_eq!(active1, 2, "Active count must be 2");
    assert_eq!(working1, 1, "Working count must be 1");
    assert_eq!(waiting1, 1, "Waiting count must be 1");
    assert_eq!(success1, 0, "Success count must be 0");
    assert_eq!(error1, 0, "Error count must be 0");

    // 2. Transition Session 1 to ERROR -> Aggregate immediately becomes ERROR (ERROR > WAITING)
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-agg-1",
            "--state",
            "Error",
        ])
        .unwrap();

    let (state2, active2, working2, waiting2, success2, error2, _) =
        proxy.get_aggregate_state().await.unwrap();
    assert_eq!(state2, "ERROR", "ERROR must override WAITING");
    assert_eq!(active2, 2, "Active count must be 2");
    assert_eq!(working2, 0, "Working count must be 0");
    assert_eq!(waiting2, 1, "Waiting count must be 1");
    assert_eq!(success2, 0, "Success count must be 0");
    assert_eq!(error2, 1, "Error count must be 1");

    // 3. Register Session 3 in ERROR with newer timestamp -> Contextual tie-breaking
    tokio::time::sleep(Duration::from_millis(50)).await;
    fixture
        .run_mock_cli(&[
            "session",
            "start",
            "--id",
            "sess-agg-3",
            "--provider",
            "opencode",
            "--state",
            "Error",
        ])
        .unwrap();

    let sessions = proxy.get_sessions().await.unwrap();
    assert_eq!(sessions.len(), 3, "Total sessions must be 3");

    let s1 = sessions
        .iter()
        .find(|s| s.session_id == "sess-agg-1")
        .unwrap();
    let s3 = sessions
        .iter()
        .find(|s| s.session_id == "sess-agg-3")
        .unwrap();

    assert_eq!(s1.current_state, "ERROR");
    assert_eq!(s3.current_state, "ERROR");
    assert!(
        s3.state_entered_at >= s1.state_entered_at,
        "Session 3 timestamp ({}) must be >= Session 1 timestamp ({})",
        s3.state_entered_at,
        s1.state_entered_at
    );

    // 4. Transition Session 2 to terminal state Success (resuming Working first)
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-agg-2",
            "--state",
            "Working",
        ])
        .unwrap();

    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-agg-2",
            "--state",
            "Success",
        ])
        .unwrap();

    // With Session 1 (ERROR), Session 3 (ERROR), and Session 2 (SUCCESS),
    // aggregate state must remain ERROR (ERROR 80 > SUCCESS 30) while dwelling
    let (state_term, active_term, _, _, success_term, error_term, _) =
        proxy.get_aggregate_state().await.unwrap();
    assert_eq!(state_term, "ERROR", "ERROR (80) must override SUCCESS (30)");
    assert_eq!(active_term, 3, "All 3 sessions are dwelling");
    assert_eq!(success_term, 1, "One session in success");
    assert_eq!(error_term, 2, "Two sessions in error");

    // 5. Verify completion dwell smooth reset to IDLE after all terminal states expire (60s)
    let dwell_start = Instant::now();
    let poll_deadline = Duration::from_secs(66);
    let mut idle_reached = false;

    while dwell_start.elapsed() < poll_deadline {
        let (s, a, _, _, _, _, _) = proxy.get_aggregate_state().await.unwrap();
        if s == "IDLE" {
            let elapsed = dwell_start.elapsed();
            assert!(
                elapsed >= Duration::from_millis(59800),
                "Dwell expired prematurely: elapsed {:?} < 59.8s",
                elapsed
            );
            assert_eq!(a, 0, "Active session count must be 0 in IDLE");
            idle_reached = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    assert!(
        idle_reached,
        "Multi-session dwell failed to reset to IDLE within 66.0s"
    );
}
