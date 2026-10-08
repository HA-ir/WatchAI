mod harness;

use futures_util::StreamExt;
use harness::E2eTestFixture;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_scenario2_mock_session_lifecycle_and_sixty_second_dwell() {
    // T161 / Scenario 2: Complete lifecycle progression and authentic 60-second completion dwell
    let fixture = E2eTestFixture::start()
        .await
        .expect("E2E fixture startup must succeed");

    let proxy = fixture
        .proxy()
        .await
        .expect("Failed to acquire D-Bus proxy");

    // Subscribe to D-Bus signal streams before dispatching events
    let mut session_added_stream = proxy
        .receive_session_added()
        .await
        .expect("Failed to subscribe to SessionAdded");
    let mut session_updated_stream = proxy
        .receive_session_updated()
        .await
        .expect("Failed to subscribe to SessionUpdated");
    let mut agg_changed_stream = proxy
        .receive_aggregate_state_changed()
        .await
        .expect("Failed to subscribe to AggregateStateChanged");

    // 1. Session start: Starting
    fixture
        .run_mock_cli(&[
            "session",
            "start",
            "--id",
            "sess-e2e-1",
            "--provider",
            "claude-code",
            "--project",
            "/tmp/e2e-project-1",
            "--state",
            "Starting",
        ])
        .expect("Failed to run mock session start");

    // Assert SessionAdded signal
    let added_signal = tokio::time::timeout(Duration::from_secs(2), session_added_stream.next())
        .await
        .expect("Timeout waiting for SessionAdded signal")
        .expect("SessionAdded stream ended unexpectedly");

    let added_dto = added_signal
        .args()
        .expect("Failed to parse SessionAdded signal args")
        .session;
    assert_eq!(added_dto.session_id, "sess-e2e-1");
    assert_eq!(added_dto.current_state, "STARTING");
    assert_eq!(added_dto.provider_id, "claude-code");
    assert_eq!(added_dto.provider_display_name, "Claude Code");

    // Assert AggregateStateChanged signal
    let agg1 = tokio::time::timeout(Duration::from_secs(2), agg_changed_stream.next())
        .await
        .expect("Timeout waiting for AggregateStateChanged signal")
        .expect("AggregateStateChanged stream ended unexpectedly");
    let agg_args1 = agg1
        .args()
        .expect("Failed to parse AggregateStateChanged args");
    assert_eq!(agg_args1.state, "STARTING");
    assert_eq!(agg_args1.active_session_count, 1);
    assert_eq!(agg_args1.waiting_session_count, 0);

    // 2. Transition to Working
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-e2e-1",
            "--state",
            "Working",
        ])
        .expect("Failed to run mock session transition to Working");

    let updated_signal1 =
        tokio::time::timeout(Duration::from_secs(2), session_updated_stream.next())
            .await
            .expect("Timeout waiting for SessionUpdated signal")
            .expect("Stream ended unexpectedly");
    let updated_dto1 = updated_signal1.args().unwrap().session;
    assert_eq!(updated_dto1.current_state, "WORKING");

    let agg2 = tokio::time::timeout(Duration::from_secs(2), agg_changed_stream.next())
        .await
        .expect("Timeout waiting for AggregateStateChanged signal")
        .unwrap();
    let agg_args2 = agg2.args().unwrap();
    assert_eq!(agg_args2.state, "WORKING");
    assert_eq!(agg_args2.active_session_count, 1);

    // 3. Transition to Waiting with tool ShellExecution
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-e2e-1",
            "--state",
            "Waiting",
            "--tool",
            "ShellExecution",
        ])
        .expect("Failed to run mock session transition to Waiting");

    let updated_signal2 =
        tokio::time::timeout(Duration::from_secs(2), session_updated_stream.next())
            .await
            .expect("Timeout waiting for SessionUpdated signal")
            .unwrap();
    let updated_dto2 = updated_signal2.args().unwrap().session;
    assert_eq!(updated_dto2.current_state, "WAITING");
    assert_eq!(updated_dto2.active_tool_category, "SHELL_EXECUTION");

    let agg3 = tokio::time::timeout(Duration::from_secs(2), agg_changed_stream.next())
        .await
        .expect("Timeout waiting for AggregateStateChanged signal")
        .unwrap();
    let agg_args3 = agg3.args().unwrap();
    assert_eq!(agg_args3.state, "WAITING");
    assert_eq!(agg_args3.waiting_session_count, 1);

    // 4. Resume Working after approval, then transition to Success
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-e2e-1",
            "--state",
            "Working",
        ])
        .expect("Failed to run mock session resumption to Working");

    let updated_signal_resume =
        tokio::time::timeout(Duration::from_secs(2), session_updated_stream.next())
            .await
            .expect("Timeout waiting for SessionUpdated signal on resume")
            .unwrap();
    let updated_dto_resume = updated_signal_resume.args().unwrap().session;
    assert_eq!(updated_dto_resume.current_state, "WORKING");

    let agg_resume = tokio::time::timeout(Duration::from_secs(2), agg_changed_stream.next())
        .await
        .expect("Timeout waiting for AggregateStateChanged signal on resume")
        .unwrap();
    assert_eq!(agg_resume.args().unwrap().state, "WORKING");

    // 5. Transition to Success
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            "sess-e2e-1",
            "--state",
            "Success",
        ])
        .expect("Failed to run mock session transition to Success");

    let updated_signal3 =
        tokio::time::timeout(Duration::from_secs(2), session_updated_stream.next())
            .await
            .expect("Timeout waiting for SessionUpdated signal")
            .unwrap();
    let updated_dto3 = updated_signal3.args().unwrap().session;
    assert_eq!(updated_dto3.current_state, "SUCCESS");

    let agg4 = tokio::time::timeout(Duration::from_secs(2), agg_changed_stream.next())
        .await
        .expect("Timeout waiting for AggregateStateChanged signal")
        .unwrap();
    let agg_args4 = agg4.args().unwrap();
    assert_eq!(agg_args4.state, "SUCCESS");

    // 6. Evaluate real 60-second completion dwell window without artificial test overrides
    let dwell_start = Instant::now();
    let mut transitioned_to_idle = false;
    let poll_deadline = Duration::from_secs(66); // 60s dwell + 6s timing tolerance margin

    while dwell_start.elapsed() < poll_deadline {
        let (state, active, _, _, _) = proxy.get_aggregate_state().await.unwrap();
        if state == "IDLE" {
            let elapsed = dwell_start.elapsed();
            assert!(
                elapsed >= Duration::from_millis(59800),
                "Dwell expired prematurely: elapsed {:?} < 59.8s",
                elapsed
            );
            assert!(
                elapsed <= Duration::from_millis(66000),
                "Dwell expired too late: elapsed {:?} > 66.0s",
                elapsed
            );
            assert_eq!(active, 0, "Active session count must be 0 in IDLE");
            transitioned_to_idle = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    assert!(
        transitioned_to_idle,
        "Aggregate state failed to reset smoothly to IDLE within 66.0s dwell window"
    );
}
