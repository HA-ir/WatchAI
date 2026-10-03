use watchai_core::session::{AdapterStatus, AgentSession, ToolCategory};
use watchai_core::state::LifecycleState;

#[test]
fn test_valid_lifecycle_transitions() {
    let mut session = AgentSession::new(
        "test-session-1".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/project",
        Some(1234),
        LifecycleState::Starting,
        AdapterStatus::Active,
    );

    assert_eq!(session.current_state, LifecycleState::Starting);

    // Starting -> Working
    assert!(session
        .transition_to(LifecycleState::Working, 2, None)
        .is_ok());
    assert_eq!(session.current_state, LifecycleState::Working);

    // Working -> Waiting (e.g. requesting tool permission)
    assert!(session
        .transition_to(LifecycleState::Waiting, 3, Some(ToolCategory::FileWrite))
        .is_ok());
    assert_eq!(session.current_state, LifecycleState::Waiting);
    assert_eq!(session.active_tool_category, Some(ToolCategory::FileWrite));

    // Waiting -> Working (permission granted)
    assert!(session
        .transition_to(LifecycleState::Working, 4, None)
        .is_ok());
    assert_eq!(session.current_state, LifecycleState::Working);

    // Working -> Success
    assert!(session
        .transition_to(LifecycleState::Success, 5, None)
        .is_ok());
    assert_eq!(session.current_state, LifecycleState::Success);

    // Success -> Idle (after dwell)
    assert!(session.transition_to(LifecycleState::Idle, 6, None).is_ok());
    assert_eq!(session.current_state, LifecycleState::Idle);
}

#[test]
fn test_invalid_transitions_rejected() {
    let mut session = AgentSession::new(
        "test-session-2".to_string(),
        "codex-cli",
        "OpenAI Codex CLI",
        "/home/user/app",
        Some(5678),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    // Idle cannot transition directly to Success without Working or Starting
    let result = session.transition_to(LifecycleState::Success, 2, None);
    assert!(result.is_err());
    assert_eq!(session.current_state, LifecycleState::Idle);

    // Idle cannot transition directly to Waiting
    let result = session.transition_to(LifecycleState::Waiting, 3, None);
    assert!(result.is_err());
    assert_eq!(session.current_state, LifecycleState::Idle);
}

#[test]
fn test_out_of_order_events_dropped() {
    let mut session = AgentSession::new(
        "test-session-3".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/repo",
        Some(9999),
        LifecycleState::Starting,
        AdapterStatus::Active,
    );

    // Advance to seq 10
    assert!(session
        .transition_to(LifecycleState::Working, 10, None)
        .is_ok());

    // Event with older seq 5 must be dropped
    let older_event = session.transition_to(LifecycleState::Waiting, 5, None);
    assert!(older_event.is_err());
    assert_eq!(session.current_state, LifecycleState::Working);

    // Duplicate seq 10 must also be dropped
    let duplicate_event = session.transition_to(LifecycleState::Waiting, 10, None);
    assert!(duplicate_event.is_err());
    assert_eq!(session.current_state, LifecycleState::Working);

    // Newer event seq 11 accepted
    assert!(session
        .transition_to(LifecycleState::Waiting, 11, None)
        .is_ok());
    assert_eq!(session.current_state, LifecycleState::Waiting);
}

#[test]
fn test_priority_scores() {
    assert!(LifecycleState::Error.priority_score() > LifecycleState::Waiting.priority_score());
    assert!(LifecycleState::Waiting.priority_score() > LifecycleState::Working.priority_score());
    assert!(LifecycleState::Working.priority_score() > LifecycleState::Starting.priority_score());
    assert!(LifecycleState::Starting.priority_score() > LifecycleState::Cancelled.priority_score());
    assert!(LifecycleState::Cancelled.priority_score() > LifecycleState::Success.priority_score());
    assert!(LifecycleState::Success.priority_score() > LifecycleState::Unknown.priority_score());
    assert!(LifecycleState::Unknown.priority_score() > LifecycleState::Idle.priority_score());
}
