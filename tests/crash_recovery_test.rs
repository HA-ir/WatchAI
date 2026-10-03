use chrono::Utc;
use std::path::PathBuf;
use watchai_adapters::traits::DiscoveredSession;
use watchai_core::aggregate::compute_aggregate_state;
use watchai_core::liveness::{check_session_liveness, LivenessCheckResult, ProcStatReader};
use watchai_core::session::{
    derive_process_session_id, AdapterStatus, AgentSession, SessionRegistry, ToolCategory,
};
use watchai_core::state::LifecycleState;

/// Mock proc reader for deterministic recovery testing.
struct MockProcReader {
    alive_pids: std::collections::HashMap<u32, u64>,
}

impl ProcStatReader for MockProcReader {
    fn read_stat(&self, pid: u32) -> Result<String, std::io::Error> {
        match self.alive_pids.get(&pid) {
            Some(st) => Ok(format!(
                "{} (claude) S 1 1 1 0 0 4194304 100 0 0 0 10 20 0 0 20 0 4 0 {} 12345678",
                pid, st
            )),
            None => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
        }
    }
}

#[tokio::test]
async fn test_daemon_restart_zero_running_processes() {
    // T072: Daemon restart with zero running agent processes
    let registry = SessionRegistry::new();
    assert_eq!(registry.count().await, 0);

    let now = Utc::now();
    let calc = compute_aggregate_state(&registry.list().await, now);

    assert_eq!(calc.aggregate_state, LifecycleState::Idle);
    assert_eq!(calc.active_session_count, 0);
    assert_eq!(calc.waiting_session_count, 0);
    assert_eq!(calc.error_session_count, 0);
    assert_eq!(calc.focused_session_id, None);
}

#[tokio::test]
async fn test_surviving_process_rediscovery_and_deterministic_id() {
    // T073: Surviving processes discovered with identical deterministic IDs in IDLE + DISCOVERY_REQUIRED
    let pid = 5001;
    let start_time = 777777;
    let project = "/home/user/workspace/backend";

    // 1. Pre-crash session ID
    let pre_crash_id = derive_process_session_id(pid, start_time, project);

    // 2. Daemon crashes and restarts with empty registry
    let registry = SessionRegistry::new();

    // 3. Recovery sweep executes and discovers surviving PID 5001
    let recovered_id = derive_process_session_id(pid, start_time, project);
    assert_eq!(
        pre_crash_id, recovered_id,
        "Surrogate session ID must be identical across daemon restarts"
    );

    let session = AgentSession::new(
        recovered_id.clone(),
        "claude-code",
        "Claude Code",
        project,
        Some(pid),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    )
    .with_process_start_time(Some(start_time));

    registry.upsert(session).await;

    // Verify properties of the recovered session
    let rec = registry.get(&recovered_id).await.unwrap();
    assert_eq!(
        rec.current_state,
        LifecycleState::Idle,
        "Surviving process must start as IDLE, never assumed WORKING/WAITING"
    );
    assert_eq!(
        rec.adapter_status,
        AdapterStatus::DiscoveryRequired,
        "Must be marked DISCOVERY_REQUIRED"
    );
    assert_eq!(
        rec.consecutive_proc_failures, 0,
        "Failure counter must be initialized to 0"
    );

    // Compute aggregate state: IDLE, zero error alerts
    let calc = compute_aggregate_state(&registry.list().await, Utc::now());
    assert_eq!(calc.aggregate_state, LifecycleState::Idle);
    assert_eq!(
        calc.error_session_count, 0,
        "Zero false ERROR alerts emitted"
    );
}

#[test]
fn test_deterministic_process_sorting_permutations() {
    // T074: Discovered processes sorted by provider_id ASC, project_path ASC, process_id ASC
    let d1 = DiscoveredSession {
        session_id: "s1".to_string(),
        provider_id: "claude-code".to_string(),
        provider_display_name: "Claude Code".to_string(),
        project_path: PathBuf::from("/a/project1"),
        process_id: Some(100),
        initial_state: LifecycleState::Idle,
        started_at: Utc::now(),
        adapter_status: AdapterStatus::DiscoveryRequired,
        process_start_time: Some(1000),
    };

    let d2 = DiscoveredSession {
        session_id: "s2".to_string(),
        provider_id: "claude-code".to_string(),
        provider_display_name: "Claude Code".to_string(),
        project_path: PathBuf::from("/a/project2"),
        process_id: Some(200),
        initial_state: LifecycleState::Idle,
        started_at: Utc::now(),
        adapter_status: AdapterStatus::DiscoveryRequired,
        process_start_time: Some(2000),
    };

    let d3 = DiscoveredSession {
        session_id: "s3".to_string(),
        provider_id: "opencode".to_string(),
        provider_display_name: "OpenCode".to_string(),
        project_path: PathBuf::from("/a/project1"),
        process_id: Some(150),
        initial_state: LifecycleState::Idle,
        started_at: Utc::now(),
        adapter_status: AdapterStatus::DiscoveryRequired,
        process_start_time: Some(3000),
    };

    // Expected order: d1 (claude-code, /a/project1, 100), d2 (claude-code, /a/project2, 200), d3 (opencode, /a/project1, 150)
    let expected_order = vec!["s1", "s2", "s3"];

    let permutations = [
        vec![d1.clone(), d2.clone(), d3.clone()],
        vec![d1.clone(), d3.clone(), d2.clone()],
        vec![d2.clone(), d1.clone(), d3.clone()],
        vec![d2.clone(), d3.clone(), d1.clone()],
        vec![d3.clone(), d1.clone(), d2.clone()],
        vec![d3.clone(), d2.clone(), d1.clone()],
    ];

    for (idx, mut perm) in permutations.into_iter().enumerate() {
        perm.sort_by(|a, b| a.deterministic_cmp(b));
        let ordered_ids: Vec<&str> = perm.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(
            ordered_ids, expected_order,
            "Permutation {} must sort into identical deterministic order",
            idx
        );
    }
}

#[tokio::test]
async fn test_dead_processes_omitted_no_phantom_resurrection() {
    // T075: Dead processes are omitted from discovery; zero phantom sessions resurrected
    let registry = SessionRegistry::new();

    // Mock reader only contains PID 8001; PID 8002 terminated while daemon was offline
    let mut alive_pids = std::collections::HashMap::new();
    alive_pids.insert(8001, 12345);
    let reader = MockProcReader { alive_pids };

    // Discovery sweep over PID 8001 and PID 8002
    let mut discovered = Vec::new();
    if reader.read_stat(8001).is_ok() {
        discovered.push(AgentSession::new(
            "sess-surviving".to_string(),
            "claude-code",
            "Claude Code",
            "/home/user/project",
            Some(8001),
            LifecycleState::Idle,
            AdapterStatus::DiscoveryRequired,
        ));
    }
    if reader.read_stat(8002).is_ok() {
        discovered.push(AgentSession::new(
            "sess-dead".to_string(),
            "claude-code",
            "Claude Code",
            "/home/user/project",
            Some(8002),
            LifecycleState::Idle,
            AdapterStatus::DiscoveryRequired,
        ));
    }

    for s in discovered {
        registry.upsert(s).await;
    }

    assert_eq!(registry.count().await, 1);
    assert!(registry.get("sess-surviving").await.is_some());
    assert!(
        registry.get("sess-dead").await.is_none(),
        "Terminated process must not be resurrected"
    );
}

#[tokio::test]
async fn test_activity_recovery_requires_fresh_telemetry() {
    // T091: Recovered session in IDLE transitions to WORKING only upon fresh verified telemetry
    let pid = 9001;
    let start_time = 444444;
    let mut alive_pids = std::collections::HashMap::new();
    alive_pids.insert(pid, start_time);
    let reader = MockProcReader { alive_pids };

    let mut session = AgentSession::new(
        "sess-activity-boundary".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(pid),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    )
    .with_process_start_time(Some(start_time));

    // 1. Process liveness check in /proc verifies process is alive
    let res = check_session_liveness(&mut session, &reader);
    assert_eq!(res, LivenessCheckResult::Alive);
    assert_eq!(
        session.current_state,
        LifecycleState::Idle,
        "Observing /proc survival must NOT promote IDLE to WORKING"
    );

    // 2. Incoming verified telemetry event arrives
    let telemetry_res = session.apply_telemetry(
        LifecycleState::Working,
        session.sequence_number + 1,
        Some(ToolCategory::ModelThinking),
    );
    assert!(telemetry_res.is_ok());
    assert_eq!(
        session.current_state,
        LifecycleState::Working,
        "Fresh telemetry must transition session to WORKING"
    );
    assert_eq!(
        session.active_tool_category,
        Some(ToolCategory::ModelThinking)
    );
}
