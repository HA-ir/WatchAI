use chrono::{Duration, Utc};
use watchai_core::aggregate::{compute_aggregate_state, within_dwell};
use watchai_core::session::{AdapterStatus, AgentSession};
use watchai_core::state::LifecycleState;

fn make_session(id: &str, state: LifecycleState, entered_offset_secs: i64) -> AgentSession {
    let mut session = AgentSession::new(
        id.to_string(),
        "claude-code",
        "Claude Code",
        format!("/home/user/project-{}", id),
        Some(1000),
        state,
        AdapterStatus::Active,
    );
    session.state_entered_at = Utc::now() - Duration::seconds(entered_offset_secs);
    session
}

#[test]
fn test_zero_sessions_aggregation() {
    let now = Utc::now();
    let result = compute_aggregate_state(&[], now);

    assert_eq!(result.aggregate_state, LifecycleState::Idle);
    assert_eq!(result.active_session_count, 0);
    assert_eq!(result.waiting_session_count, 0);
    assert_eq!(result.error_session_count, 0);
    assert_eq!(result.focused_session_id, None);
}

#[test]
fn test_single_session_aggregation() {
    let now = Utc::now();
    let states = [
        LifecycleState::Idle,
        LifecycleState::Starting,
        LifecycleState::Working,
        LifecycleState::Waiting,
        LifecycleState::Unknown,
    ];

    for state in states {
        let session = make_session("sess-1", state, 2);
        let result = compute_aggregate_state(&[session], now);
        assert_eq!(
            result.aggregate_state, state,
            "Single session must determine aggregate state"
        );
        assert_eq!(result.focused_session_id.as_deref(), Some("sess-1"));
    }
}

#[test]
fn test_priority_hierarchy_permutations() {
    let now = Utc::now();

    // Priority: ERROR (80) > WAITING (70) > WORKING (60) > STARTING (50) > CANCELLED (40) > SUCCESS (30) > UNKNOWN (20) > IDLE (10)
    let ordered_states = [
        LifecycleState::Error,
        LifecycleState::Waiting,
        LifecycleState::Working,
        LifecycleState::Starting,
        LifecycleState::Cancelled,
        LifecycleState::Success,
        LifecycleState::Unknown,
        LifecycleState::Idle,
    ];

    for i in 0..ordered_states.len() {
        for j in (i + 1)..ordered_states.len() {
            let high_state = ordered_states[i];
            let low_state = ordered_states[j];

            // Both within 2 seconds (well within 10s completion dwell)
            let s1 = make_session("high", high_state, 2);
            let s2 = make_session("low", low_state, 2);

            let res = compute_aggregate_state(&[s1, s2], now);
            assert_eq!(
                res.aggregate_state,
                high_state,
                "State {:?} (prio {}) must dominate {:?} (prio {})",
                high_state,
                high_state.priority_score(),
                low_state,
                low_state.priority_score()
            );
        }
    }
}

#[test]
fn test_counter_calculations() {
    let now = Utc::now();

    // S1: Working (active)
    let s1 = make_session("s1", LifecycleState::Working, 5);
    // S2: Waiting (active, waiting)
    let s2 = make_session("s2", LifecycleState::Waiting, 4);
    // S3: Error within dwell (active, error)
    let s3 = make_session("s3", LifecycleState::Error, 3);
    // S4: Idle (not active, not waiting, not error)
    let s4 = make_session("s4", LifecycleState::Idle, 10);
    // S5: Success past dwell (not active, not waiting, not error)
    let s5 = make_session("s5", LifecycleState::Success, 15);

    let res = compute_aggregate_state(&[s1, s2, s3, s4, s5], now);

    assert_eq!(res.aggregate_state, LifecycleState::Error);
    assert_eq!(res.active_session_count, 3); // s1, s2, s3 (in dwell)
    assert_eq!(res.waiting_session_count, 1); // s2
    assert_eq!(res.error_session_count, 1); // s3
}

#[test]
fn test_contextual_tie_breaking_recency_and_lexicographical() {
    let now = Utc::now();

    // Two WAITING sessions with different state_entered_at: most recent must win
    let s_older = make_session("sess-a", LifecycleState::Waiting, 20);
    let s_newer = make_session("sess-b", LifecycleState::Waiting, 5);

    let res = compute_aggregate_state(&[s_older.clone(), s_newer.clone()], now);
    assert_eq!(res.aggregate_state, LifecycleState::Waiting);
    assert_eq!(
        res.focused_session_id.as_deref(),
        Some("sess-b"),
        "Most recent state_entered_at must be focused"
    );

    // Two WAITING sessions with identical state_entered_at: lexicographical session_id must win
    let mut s_lex1 = make_session("sess-alpha", LifecycleState::Waiting, 10);
    let mut s_lex2 = make_session("sess-beta", LifecycleState::Waiting, 10);
    let common_time = now - Duration::seconds(10);
    s_lex1.state_entered_at = common_time;
    s_lex2.state_entered_at = common_time;

    let res2 = compute_aggregate_state(&[s_lex2.clone(), s_lex1.clone()], now);
    assert_eq!(
        res2.focused_session_id.as_deref(),
        Some("sess-alpha"),
        "Lexicographical session_id must break ties when timestamps match"
    );
}

#[test]
fn test_ten_second_completion_dwell() {
    let now = Utc::now();

    // 1. Session in SUCCESS entered 5 seconds ago -> within dwell
    let s_active_dwell = make_session("sess-success", LifecycleState::Success, 5);
    assert!(within_dwell(&s_active_dwell, now));

    let res_dwell = compute_aggregate_state(&[s_active_dwell], now);
    assert_eq!(res_dwell.aggregate_state, LifecycleState::Success);
    assert_eq!(res_dwell.active_session_count, 1);

    // 2. Session in SUCCESS entered 11 seconds ago -> dwell expired
    let s_expired_dwell = make_session("sess-success-expired", LifecycleState::Success, 11);
    assert!(!within_dwell(&s_expired_dwell, now));

    let res_expired = compute_aggregate_state(std::slice::from_ref(&s_expired_dwell), now);
    assert_eq!(
        res_expired.aggregate_state,
        LifecycleState::Idle,
        "Aggregate state must settle to IDLE after 10s dwell expires"
    );
    assert_eq!(
        res_expired.active_session_count, 0,
        "Expired dwell session must not count towards active_session_count"
    );
    // Crucial invariant: The session entity itself still retains its terminal state!
    assert_eq!(s_expired_dwell.current_state, LifecycleState::Success);

    // 3. Error session also follows the 10s completion dwell
    let s_error_dwell = make_session("sess-err", LifecycleState::Error, 3);
    let res_err = compute_aggregate_state(&[s_error_dwell], now);
    assert_eq!(res_err.aggregate_state, LifecycleState::Error);
    assert_eq!(res_err.active_session_count, 1);
    assert_eq!(res_err.error_session_count, 1);

    let s_error_expired = make_session("sess-err-exp", LifecycleState::Error, 12);
    let res_err_exp = compute_aggregate_state(std::slice::from_ref(&s_error_expired), now);
    assert_eq!(
        res_err_exp.aggregate_state,
        LifecycleState::Idle,
        "Aggregate state must settle to IDLE after 10s dwell even for ERROR"
    );
    assert_eq!(res_err_exp.active_session_count, 0);
    // But error_session_count still reflects that there is a session in ERROR in the registry
    assert_eq!(res_err_exp.error_session_count, 1);
    assert_eq!(s_error_expired.current_state, LifecycleState::Error);
}
