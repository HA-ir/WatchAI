use chrono::{Duration, Utc};
use watchai_core::liveness::{
    check_silence_timeout, is_retention_expired, prune_retained_sessions,
    SILENCE_TIMEOUT_STARTING_SECONDS, SILENCE_TIMEOUT_WORKING_SECONDS,
};
use watchai_core::session::{AdapterStatus, AgentSession, SessionRegistry, ToolCategory};
use watchai_core::state::LifecycleState;

fn make_session_with_last_seen(
    id: &str,
    state: LifecycleState,
    last_seen_offset_secs: i64,
    base_time: chrono::DateTime<Utc>,
) -> AgentSession {
    let mut session = AgentSession::new(
        id.to_string(),
        "claude-code",
        "Claude Code",
        format!("/home/user/project-{}", id),
        None, // Unmonitored (no PID)
        state,
        AdapterStatus::Active,
    );
    session.last_seen_at = base_time - Duration::seconds(last_seen_offset_secs);
    session
}

#[test]
fn test_adaptive_silence_timeout_working_and_starting() {
    let now = Utc::now();

    // 1. WORKING session: 250s silence (under 300s threshold) -> stays WORKING
    let mut s_working_ok = make_session_with_last_seen("w-ok", LifecycleState::Working, 250, now);
    assert!(!check_silence_timeout(&mut s_working_ok, now));
    assert_eq!(s_working_ok.current_state, LifecycleState::Working);

    // 2. WORKING session: 301s silence (over 300s threshold) -> transitions to UNKNOWN
    let mut s_working_stale =
        make_session_with_last_seen("w-stale", LifecycleState::Working, 301, now);
    assert!(check_silence_timeout(&mut s_working_stale, now));
    assert_eq!(s_working_stale.current_state, LifecycleState::Unknown);

    // 3. STARTING session: 45s silence (under 60s threshold) -> stays STARTING
    let mut s_starting_ok = make_session_with_last_seen("s-ok", LifecycleState::Starting, 45, now);
    assert!(!check_silence_timeout(&mut s_starting_ok, now));
    assert_eq!(s_starting_ok.current_state, LifecycleState::Starting);

    // 4. STARTING session: 65s silence (over 60s threshold) -> transitions to UNKNOWN
    let mut s_starting_stale =
        make_session_with_last_seen("s-stale", LifecycleState::Starting, 65, now);
    assert!(check_silence_timeout(&mut s_starting_stale, now));
    assert_eq!(s_starting_stale.current_state, LifecycleState::Unknown);

    // 5. Constants match specification requirements
    assert_eq!(SILENCE_TIMEOUT_WORKING_SECONDS, 300);
    assert_eq!(SILENCE_TIMEOUT_STARTING_SECONDS, 60);
}

#[test]
fn test_stale_recovery_telemetry_gated() {
    let now = Utc::now();

    // Start with a session that transitioned to UNKNOWN due to silence
    let mut session =
        make_session_with_last_seen("sess-stale-rec", LifecycleState::Unknown, 400, now);
    assert_eq!(session.current_state, LifecycleState::Unknown);

    // Incoming telemetry event arrives with next sequence number
    let telemetry_res = session.apply_telemetry(
        LifecycleState::Working,
        session.sequence_number + 1,
        Some(ToolCategory::FileWrite),
    );
    assert!(
        telemetry_res.is_ok(),
        "Telemetry event must recover UNKNOWN -> WORKING"
    );
    assert_eq!(session.current_state, LifecycleState::Working);
    assert_eq!(session.active_tool_category, Some(ToolCategory::FileWrite));

    // Verify that silence timeout does NOT immediately trigger again since last_seen_at updated
    assert!(!check_silence_timeout(&mut session, now));
}

#[tokio::test]
async fn test_terminal_retention_pruning_sixty_seconds() {
    let registry = SessionRegistry::new();
    let now = Utc::now();

    // 1. Session in SUCCESS entered 30 seconds ago:
    // Past 10s completion dwell, but within 60s retention -> retained!
    let mut s_retained = AgentSession::new(
        "sess-retained".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1234),
        LifecycleState::Success,
        AdapterStatus::Active,
    );
    s_retained.state_entered_at = now - Duration::seconds(30);
    assert!(!is_retention_expired(&s_retained, now));
    registry.upsert(s_retained).await;

    // 2. Session in ERROR entered 65 seconds ago -> retention expired!
    let mut s_expired_err = AgentSession::new(
        "sess-expired-err".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1235),
        LifecycleState::Error,
        AdapterStatus::Active,
    );
    s_expired_err.state_entered_at = now - Duration::seconds(65);
    assert!(is_retention_expired(&s_expired_err, now));
    registry.upsert(s_expired_err).await;

    // 3. Active session in WORKING -> never retention-expired
    let mut s_working = AgentSession::new(
        "sess-working".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1236),
        LifecycleState::Working,
        AdapterStatus::Active,
    );
    s_working.state_entered_at = now - Duration::seconds(100);
    assert!(!is_retention_expired(&s_working, now));
    registry.upsert(s_working).await;

    // Run pruning worker
    let pruned = prune_retained_sessions(&registry, now).await;
    assert_eq!(pruned, vec!["sess-expired-err".to_string()]);

    // Verify registry state:
    // Expired session is pruned from memory
    assert!(registry.get("sess-expired-err").await.is_none());
    // Retained session is still in memory and STILL in SUCCESS!
    let retained_rec = registry
        .get("sess-retained")
        .await
        .expect("Session must remain retained");
    assert_eq!(
        retained_rec.current_state,
        LifecycleState::Success,
        "Retained terminal session must NOT mutate to IDLE"
    );
    // Working session is still in memory
    assert!(registry.get("sess-working").await.is_some());
}
