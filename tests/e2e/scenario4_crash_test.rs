mod harness;

use harness::E2eTestFixture;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_scenario4_worker_crash_and_liveness_hysteresis() {
    // T163 / Scenario 4: Real process discovery, abrupt SIGKILL termination, and 2-check failure hysteresis detection
    let fixture = E2eTestFixture::start()
        .await
        .expect("E2E fixture startup must succeed");

    let proxy = fixture
        .proxy()
        .await
        .expect("Failed to acquire D-Bus proxy");

    let project_dir = fixture.temp_dir.path().join("crash-test-workspace");
    fs::create_dir_all(&project_dir).unwrap();

    // 1. Spawn realistic mock worker: claude.py script runner matching ProcessScanner
    let worker_output = fixture
        .run_mock_cli(&[
            "worker",
            "run",
            "--provider",
            "claude-code",
            "--project",
            project_dir.to_str().unwrap(),
            "--hold-seconds",
            "60",
        ])
        .expect("Failed to spawn mock worker process");

    // Extract PID from output: "Worker spawned: pid=<PID>, ..."
    let worker_pid: u32 = worker_output
        .split("pid=")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .expect("Failed to parse worker PID from output");

    assert!(worker_pid > 0, "Spawned PID must be positive");
    assert!(
        PathBuf::from(format!("/proc/{}", worker_pid)).exists(),
        "Worker process must be active in /proc"
    );

    // 2. Poll D-Bus until daemon's background /proc discovery sweep registers the session (within 4.0s)
    let discovery_start = Instant::now();
    let mut discovered_session_id: Option<String> = None;

    while discovery_start.elapsed() < Duration::from_millis(4000) {
        if let Ok(sessions) = proxy.get_sessions().await {
            if let Some(s) = sessions.iter().find(|s| s.process_id == worker_pid) {
                discovered_session_id = Some(s.session_id.clone());
                assert_eq!(
                    s.current_state, "IDLE",
                    "Discovered session initial state must be IDLE"
                );
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let session_id =
        discovered_session_id.expect("Daemon failed to discover worker process within 4.0s");

    // 3. Transition discovered session to WORKING via mock event socket
    fixture
        .run_mock_cli(&[
            "session",
            "transition",
            "--id",
            &session_id,
            "--state",
            "Working",
        ])
        .expect("Failed to transition session to Working");

    // Verify session is now WORKING
    let s_dto = proxy
        .get_session(session_id.clone())
        .await
        .expect("Session must exist");
    assert_eq!(s_dto.current_state, "WORKING");

    // 4. Terminate worker abruptly with SIGKILL (kill -9)
    let kill_time = Instant::now();
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(worker_pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    )
    .expect("Failed to send SIGKILL to worker process");

    // 5. Poll D-Bus and verify 2-check failure hysteresis (2.0s–4.0s, strictly bounded by 5.0s max)
    let mut error_detected = false;
    let hysteresis_ceiling = Duration::from_secs(5);

    while kill_time.elapsed() < hysteresis_ceiling {
        if let Ok(s) = proxy.get_session(session_id.clone()).await {
            if s.current_state == "ERROR" {
                let elapsed = kill_time.elapsed();
                assert!(
                    elapsed >= Duration::from_millis(1800),
                    "Hysteresis triggered prematurely: elapsed {:?} < 1.8s (must require 2 checks)",
                    elapsed
                );
                assert!(
                    elapsed <= Duration::from_millis(5000),
                    "Crash detection exceeded 5.0s ceiling: elapsed {:?}",
                    elapsed
                );
                error_detected = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        error_detected,
        "Daemon failed to detect ungraceful crash and transition to ERROR within 5.0s"
    );

    // 6. Verify aggregate state also transitioned to ERROR
    let (agg_state, active, _, _, _, error_count, _) = proxy
        .get_aggregate_state()
        .await
        .expect("GetAggregateState call failed after crash");
    assert_eq!(agg_state, "ERROR");
    assert_eq!(active, 1);
    assert_eq!(error_count, 1);

    // 7. Verify daemon remains fully responsive to subsequent calls
    let sessions_after = proxy
        .get_sessions()
        .await
        .expect("GetSessions must succeed after crash");
    assert_eq!(sessions_after.len(), 1);
}
