use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::sync::Mutex;
use watchai_core::aggregate::compute_aggregate_state;
use watchai_core::liveness::{
    check_session_liveness, is_retention_expired, prune_retained_sessions, DeadReason,
    LivenessCheckResult, ProcStatReader,
};
use watchai_core::session::{
    derive_process_session_id, AdapterStatus, AgentSession, SessionRegistry,
};
use watchai_core::state::LifecycleState;

struct MockStatReader {
    stats: Mutex<HashMap<u32, Result<String, ErrorKind>>>,
}

impl MockStatReader {
    fn new() -> Self {
        Self {
            stats: Mutex::new(HashMap::new()),
        }
    }

    fn set_stat(&self, pid: u32, comm: &str, starttime: u64) {
        let content = format!(
            "{} ({}) S 1 1 1 0 0 4194304 100 0 0 0 10 20 0 0 20 0 4 0 {} 12345678",
            pid, comm, starttime
        );
        self.stats.lock().unwrap().insert(pid, Ok(content));
    }

    fn set_error(&self, pid: u32, kind: ErrorKind) {
        self.stats.lock().unwrap().insert(pid, Err(kind));
    }
}

impl ProcStatReader for MockStatReader {
    fn read_stat(&self, pid: u32) -> Result<String, Error> {
        let guard = self.stats.lock().unwrap();
        match guard.get(&pid) {
            Some(Ok(s)) => Ok(s.clone()),
            Some(Err(k)) => Err(Error::from(*k)),
            None => Err(Error::from(ErrorKind::NotFound)),
        }
    }
}

#[tokio::test]
async fn test_quickstart_scenario_1_multi_session_priority_resolution() {
    let now = Utc::now();

    // S1 in WORKING
    let s1 = AgentSession::new(
        "S1".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/proj-s1",
        Some(1001),
        LifecycleState::Working,
        AdapterStatus::Active,
    );

    // S2 in WAITING
    let s2 = AgentSession::new(
        "S2".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/proj-s2",
        Some(1002),
        LifecycleState::Waiting,
        AdapterStatus::Active,
    );

    // S3 in SUCCESS entered 2 seconds ago (within dwell)
    let mut s3 = AgentSession::new(
        "S3".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/proj-s3",
        Some(1003),
        LifecycleState::Success,
        AdapterStatus::Active,
    );
    s3.state_entered_at = now - Duration::seconds(2);

    let calc = compute_aggregate_state(&[s1, s2, s3], now);

    // WAITING (70) > WORKING (60) > SUCCESS (30)
    assert_eq!(calc.aggregate_state, LifecycleState::Waiting);
    assert_eq!(calc.active_session_count, 3);
    assert_eq!(calc.waiting_session_count, 1);
    assert_eq!(calc.error_session_count, 0);
    assert_eq!(calc.focused_session_id.as_deref(), Some("S2"));
}

#[tokio::test]
async fn test_quickstart_scenario_2_dwell_vs_retention() {
    let registry = SessionRegistry::new();
    let base_time = Utc::now();

    // S1 finishes task and transitions to SUCCESS (terminated session)
    let mut s1 = AgentSession::new(
        "S1".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/proj-s1",
        Some(1001),
        LifecycleState::Success,
        AdapterStatus::Active,
    );
    s1.is_terminated = true;
    s1.state_entered_at = base_time;
    registry.upsert(s1.clone()).await;

    // At T = 30s: inside dwell window (30s <= 60s)
    let t_30s = base_time + Duration::seconds(30);
    let calc_30s = compute_aggregate_state(&registry.list().await, t_30s);
    assert_eq!(calc_30s.aggregate_state, LifecycleState::Success);
    assert_eq!(calc_30s.active_session_count, 1);
    assert!(!is_retention_expired(&s1, t_30s));

    // At T = 61s: 60-second dwell expired, aggregate state settles to IDLE!
    let t_61s = base_time + Duration::seconds(61);
    let calc_61s = compute_aggregate_state(&registry.list().await, t_61s);
    assert_eq!(
        calc_61s.aggregate_state,
        LifecycleState::Idle,
        "Top-bar aggregate state must settle to IDLE after 60s"
    );
    assert_eq!(calc_61s.active_session_count, 0);

    // But before pruning runs, S1 is STILL in registry and STILL in SUCCESS!
    let session_61s = registry.get("S1").await.unwrap();
    assert_eq!(session_61s.current_state, LifecycleState::Success);

    // 60-second retention expired for terminated session -> pruned!
    assert!(is_retention_expired(&session_61s, t_61s));
    let pruned = prune_retained_sessions(&registry, t_61s).await;
    assert_eq!(pruned, vec!["S1".to_string()]);
    assert!(registry.get("S1").await.is_none());
}

#[tokio::test]
async fn test_quickstart_scenario_3_crash_detection_hysteresis() {
    let reader = MockStatReader::new();
    reader.set_error(2001, ErrorKind::NotFound);

    let mut session = AgentSession::new(
        "S-crash".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(2001),
        LifecycleState::Working,
        AdapterStatus::Active,
    )
    .with_process_start_time(Some(50000));

    // Check 1: 1st failure (transient) -> stays WORKING
    let res1 = check_session_liveness(&mut session, &reader);
    assert_eq!(res1, LivenessCheckResult::TransientFailure { failures: 1 });
    assert_eq!(session.current_state, LifecycleState::Working);

    // Check 2: 2nd consecutive failure -> confirmed termination -> transitions to ERROR
    let res2 = check_session_liveness(&mut session, &reader);
    assert_eq!(
        res2,
        LivenessCheckResult::Dead {
            reason: DeadReason::ProcessTerminated {
                consecutive_failures: 2
            }
        }
    );
    assert_eq!(session.current_state, LifecycleState::Error);
}

#[tokio::test]
async fn test_quickstart_scenario_4_pid_reuse_protection() {
    let reader = MockStatReader::new();
    // Recorded PID 3001 had start time 40000. Recycled process has start time 80000.
    reader.set_stat(3001, "unrelated-proc", 80000);

    let mut session = AgentSession::new(
        "S-pid-reuse".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(3001),
        LifecycleState::Working,
        AdapterStatus::Active,
    )
    .with_process_start_time(Some(40000));

    let res = check_session_liveness(&mut session, &reader);
    assert_eq!(
        res,
        LivenessCheckResult::Dead {
            reason: DeadReason::PidReused {
                recorded_start_time: 40000,
                current_start_time: 80000,
            }
        }
    );
    assert_eq!(session.current_state, LifecycleState::Error);
}

#[tokio::test]
async fn test_quickstart_scenario_5_daemon_restart_recovery() {
    let pid = 4001;
    let start_time = 123456;
    let project = "/home/user/workspace/app";

    // 1. Before daemon restart: session ID derived from PID, start time, and project path
    let expected_id = derive_process_session_id(pid, start_time, project);

    // 2. Daemon restarts with empty volatile registry
    let fresh_registry = SessionRegistry::new();
    assert_eq!(fresh_registry.count().await, 0);

    // 3. Discovery sweep runs on startup: finds surviving process
    let recovered_id = derive_process_session_id(pid, start_time, project);
    assert_eq!(
        expected_id, recovered_id,
        "Surrogate session ID must be identical across daemon restarts"
    );

    // Register surviving session: initial state IDLE + DISCOVERY_REQUIRED
    let recovered_session = AgentSession::new(
        recovered_id.clone(),
        "claude-code",
        "Claude Code",
        project,
        Some(pid),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    )
    .with_process_start_time(Some(start_time));

    fresh_registry.upsert(recovered_session).await;
    assert_eq!(fresh_registry.count().await, 1);

    let rec = fresh_registry.get(&recovered_id).await.unwrap();
    assert_eq!(rec.current_state, LifecycleState::Idle);
    assert_eq!(rec.adapter_status, AdapterStatus::DiscoveryRequired);
    assert_eq!(rec.process_start_time, Some(start_time));
}
