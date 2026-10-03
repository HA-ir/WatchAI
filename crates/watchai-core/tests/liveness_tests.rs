use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::sync::Mutex;
use watchai_core::liveness::{
    check_session_liveness, parse_proc_stat_starttime, DeadReason, LivenessCheckResult,
    ProcStatReader,
};
use watchai_core::session::{AdapterStatus, AgentSession};
use watchai_core::state::LifecycleState;

/// Mock implementation of ProcStatReader for deterministic testing without OS processes.
struct MockProcStatReader {
    responses: Mutex<HashMap<u32, Result<String, ErrorKind>>>,
}

impl MockProcStatReader {
    fn new() -> Self {
        Self {
            responses: Mutex::new(HashMap::new()),
        }
    }

    fn set_stat(&self, pid: u32, content: &str) {
        self.responses
            .lock()
            .unwrap()
            .insert(pid, Ok(content.to_string()));
    }

    fn set_error(&self, pid: u32, kind: ErrorKind) {
        self.responses.lock().unwrap().insert(pid, Err(kind));
    }
}

impl ProcStatReader for MockProcStatReader {
    fn read_stat(&self, pid: u32) -> Result<String, Error> {
        let guard = self.responses.lock().unwrap();
        match guard.get(&pid) {
            Some(Ok(s)) => Ok(s.clone()),
            Some(Err(k)) => Err(Error::from(*k)),
            None => Err(Error::from(ErrorKind::NotFound)),
        }
    }
}

/// Helper to format a synthetic Linux /proc/[pid]/stat string with field 22 starttime.
fn make_stat_line(pid: u32, comm: &str, starttime: u64) -> String {
    // pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt utime stime cutime cstime priority nice num_threads itrealvalue starttime ...
    format!(
        "{} ({}) S 1 1 1 0 0 4194304 100 0 0 0 10 20 0 0 20 0 4 0 {} 12345678",
        pid, comm, starttime
    )
}

#[test]
fn test_parse_proc_stat_starttime_various_comm() {
    // Normal process name
    let s1 = make_stat_line(1234, "claude", 54321);
    assert_eq!(parse_proc_stat_starttime(&s1), Some(54321));

    // Process name containing spaces and parentheses
    let s2 = make_stat_line(5678, "claude (sub) worker", 99999);
    assert_eq!(parse_proc_stat_starttime(&s2), Some(99999));

    // Empty or malformed stat line
    assert_eq!(parse_proc_stat_starttime(""), None);
    assert_eq!(
        parse_proc_stat_starttime("invalid content without comm"),
        None
    );
}

#[test]
fn test_process_survival_alive() {
    let reader = MockProcStatReader::new();
    reader.set_stat(1001, &make_stat_line(1001, "claude", 50000));

    let mut session = AgentSession::new(
        "sess-alive".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1001),
        LifecycleState::Working,
        AdapterStatus::Active,
    )
    .with_process_start_time(Some(50000));

    let res = check_session_liveness(&mut session, &reader);
    assert_eq!(res, LivenessCheckResult::Alive);
    assert_eq!(session.current_state, LifecycleState::Working);
    assert_eq!(session.consecutive_proc_failures, 0);
}

#[test]
fn test_pid_reuse_detected_immediately() {
    let reader = MockProcStatReader::new();
    // Recorded start time is 50000, but live process has start time 70000 (recycled PID!)
    reader.set_stat(1002, &make_stat_line(1002, "unrelated-app", 70000));

    let mut session = AgentSession::new(
        "sess-reuse".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1002),
        LifecycleState::Working,
        AdapterStatus::Active,
    )
    .with_process_start_time(Some(50000));

    let res = check_session_liveness(&mut session, &reader);
    assert_eq!(
        res,
        LivenessCheckResult::Dead {
            reason: DeadReason::PidReused {
                recorded_start_time: 50000,
                current_start_time: 70000,
            }
        }
    );

    // Session must immediately transition to ERROR
    assert_eq!(session.current_state, LifecycleState::Error);
}

#[test]
fn test_single_transient_failure_does_not_declare_dead() {
    let reader = MockProcStatReader::new();
    // Simulate first check failing with NotFound
    reader.set_error(1003, ErrorKind::NotFound);

    let mut session = AgentSession::new(
        "sess-transient".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1003),
        LifecycleState::Working,
        AdapterStatus::Active,
    )
    .with_process_start_time(Some(50000));

    // First check fails
    let res1 = check_session_liveness(&mut session, &reader);
    assert_eq!(res1, LivenessCheckResult::TransientFailure { failures: 1 });
    // Crucial: Single failure must NOT transition to ERROR
    assert_eq!(session.current_state, LifecycleState::Working);
    assert_eq!(session.consecutive_proc_failures, 1);

    // Second check succeeds (transient failure resolved)
    reader.set_stat(1003, &make_stat_line(1003, "claude", 50000));
    let res2 = check_session_liveness(&mut session, &reader);
    assert_eq!(res2, LivenessCheckResult::Alive);
    assert_eq!(session.current_state, LifecycleState::Working);
    // Counter must reset to 0
    assert_eq!(session.consecutive_proc_failures, 0);
}

#[test]
fn test_two_consecutive_failures_confirms_dead() {
    let reader = MockProcStatReader::new();
    reader.set_error(1004, ErrorKind::NotFound);

    let mut session = AgentSession::new(
        "sess-dead".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1004),
        LifecycleState::Working,
        AdapterStatus::Active,
    )
    .with_process_start_time(Some(50000));

    // 1st failure: transient
    let res1 = check_session_liveness(&mut session, &reader);
    assert_eq!(res1, LivenessCheckResult::TransientFailure { failures: 1 });
    assert_eq!(session.current_state, LifecycleState::Working);

    // 2nd consecutive failure: death confirmed!
    let res2 = check_session_liveness(&mut session, &reader);
    assert_eq!(
        res2,
        LivenessCheckResult::Dead {
            reason: DeadReason::ProcessTerminated {
                consecutive_failures: 2
            }
        }
    );
    // Transitioned to ERROR
    assert_eq!(session.current_state, LifecycleState::Error);
}
