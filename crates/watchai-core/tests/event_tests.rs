use chrono::{Duration, Utc};
use watchai_core::session::{AdapterStatus, AgentSession, SessionLifecycleEvent, ToolCategory};
use watchai_core::state::LifecycleState;

fn create_test_session() -> AgentSession {
    AgentSession::new(
        "test-sess-123".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/test",
        Some(1234),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    )
}

#[test]
fn test_valid_state_transition_via_lifecycle_event() {
    // T115: Legal state transition Idle -> Working
    let mut session = create_test_session();
    let initial_seq = session.sequence_number;
    let now = Utc::now() + Duration::milliseconds(10);

    let event = SessionLifecycleEvent::StateTransition {
        session_id: session.session_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: Some(ToolCategory::ModelThinking),
        timestamp: now,
    };

    let result = session.apply_lifecycle_event(&event);
    assert!(result.is_ok(), "Legal transition must succeed");
    assert_eq!(session.current_state, LifecycleState::Working);
    assert_eq!(
        session.active_tool_category,
        Some(ToolCategory::ModelThinking)
    );
    assert_eq!(
        session.sequence_number,
        initial_seq + 1,
        "Internal sequence number must increment"
    );
    assert_eq!(session.state_entered_at, now);
}

#[test]
fn test_stale_event_rejected_by_timestamp() {
    // T115: Event timestamp older than last_seen_at must be rejected
    let mut session = create_test_session();
    let old_time = session.last_seen_at - Duration::seconds(10);

    let event = SessionLifecycleEvent::StateTransition {
        session_id: session.session_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: None,
        timestamp: old_time,
    };

    let result = session.apply_lifecycle_event(&event);
    assert!(
        result.is_err(),
        "Event older than last_seen_at must be rejected"
    );
    assert_eq!(
        session.current_state,
        LifecycleState::Idle,
        "State must not change on stale event"
    );
}

#[test]
fn test_older_state_transition_relative_to_last_seen_at_rejected() {
    // Audit Finding 3: state_entered_at = 10, last_seen_at = 20, event timestamp = 15
    let mut session = create_test_session();
    let t10 = Utc::now();
    session
        .apply_lifecycle_event(&SessionLifecycleEvent::StateTransition {
            session_id: session.session_id.clone(),
            new_state: LifecycleState::Working,
            tool_category: None,
            timestamp: t10,
        })
        .unwrap();

    let t20 = t10 + Duration::seconds(10);
    session
        .apply_lifecycle_event(&SessionLifecycleEvent::Heartbeat {
            session_id: session.session_id.clone(),
            timestamp: t20,
        })
        .unwrap();
    assert_eq!(session.state_entered_at, t10);
    assert_eq!(session.last_seen_at, t20);

    // Event timestamp = t15 (between state_entered_at and last_seen_at)
    let t15 = t10 + Duration::seconds(5);
    let stale_event = SessionLifecycleEvent::StateTransition {
        session_id: session.session_id.clone(),
        new_state: LifecycleState::Waiting,
        tool_category: None,
        timestamp: t15,
    };

    let result = session.apply_lifecycle_event(&stale_event);
    assert!(
        result.is_err(),
        "Event with timestamp < last_seen_at must be rejected even if >= state_entered_at"
    );
    assert_eq!(
        session.current_state,
        LifecycleState::Working,
        "State must not be modified by temporally stale event"
    );
}

#[test]
fn test_older_heartbeat_and_termination_rejected() {
    let mut session = create_test_session();
    let t10 = Utc::now();
    session.last_seen_at = t10;

    let t_past = t10 - Duration::seconds(5);
    // Older heartbeat rejected
    let stale_hb = SessionLifecycleEvent::Heartbeat {
        session_id: session.session_id.clone(),
        timestamp: t_past,
    };
    assert!(session.apply_lifecycle_event(&stale_hb).is_err());
    assert_eq!(session.last_seen_at, t10);

    // Older termination rejected
    let stale_term = SessionLifecycleEvent::SessionTerminated {
        session_id: session.session_id.clone(),
        exit_code: Some(0),
        timestamp: t_past,
    };
    assert!(session.apply_lifecycle_event(&stale_term).is_err());
    assert_eq!(session.current_state, LifecycleState::Idle);
}

#[test]
fn test_equal_timestamp_state_transition_accepted() {
    let mut session = create_test_session();
    let now = session.last_seen_at;

    let event = SessionLifecycleEvent::StateTransition {
        session_id: session.session_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: None,
        timestamp: now,
    };

    let result = session.apply_lifecycle_event(&event);
    assert!(result.is_ok(), "Equal timestamp must be accepted");
    assert_eq!(session.current_state, LifecycleState::Working);
}

#[test]
fn test_terminal_state_rejects_subsequent_events() {
    let mut session = create_test_session();
    let now = Utc::now();
    // Transition Idle -> Working -> Success
    session
        .apply_lifecycle_event(&SessionLifecycleEvent::StateTransition {
            session_id: session.session_id.clone(),
            new_state: LifecycleState::Working,
            tool_category: None,
            timestamp: now,
        })
        .unwrap();

    let term_time = now + Duration::seconds(1);
    session
        .apply_lifecycle_event(&SessionLifecycleEvent::SessionTerminated {
            session_id: session.session_id.clone(),
            exit_code: Some(0),
            timestamp: term_time,
        })
        .unwrap();
    assert_eq!(session.current_state, LifecycleState::Success);

    // Any subsequent event must be rejected
    let later = term_time + Duration::seconds(10);
    let post_terminal_transition = SessionLifecycleEvent::StateTransition {
        session_id: session.session_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: None,
        timestamp: later,
    };
    assert!(
        session
            .apply_lifecycle_event(&post_terminal_transition)
            .is_err(),
        "Terminal session must reject subsequent state transitions"
    );

    let post_terminal_hb = SessionLifecycleEvent::Heartbeat {
        session_id: session.session_id.clone(),
        timestamp: later,
    };
    assert!(
        session.apply_lifecycle_event(&post_terminal_hb).is_err(),
        "Terminal session must reject subsequent heartbeats"
    );
    assert_eq!(session.current_state, LifecycleState::Success);
}

#[test]
fn test_invalid_fsm_transition_rejected() {
    // T115: Transition that violates FSM edge (e.g. Idle directly to Success) must be rejected
    let mut session = create_test_session();

    let event = SessionLifecycleEvent::StateTransition {
        session_id: session.session_id.clone(),
        new_state: LifecycleState::Success,
        tool_category: None,
        timestamp: Utc::now(),
    };

    let result = session.apply_lifecycle_event(&event);
    assert!(result.is_err(), "Illegal FSM transition must be rejected");
    assert_eq!(session.current_state, LifecycleState::Idle);
}

#[test]
fn test_heartbeat_event_updates_last_seen_at_only() {
    // T115: Heartbeat touches last_seen_at without mutating current_state
    let mut session = create_test_session();
    let initial_state = session.current_state;
    let initial_entered_at = session.state_entered_at;
    let heartbeat_time = Utc::now() + Duration::seconds(5);

    let event = SessionLifecycleEvent::Heartbeat {
        session_id: session.session_id.clone(),
        timestamp: heartbeat_time,
    };

    let result = session.apply_lifecycle_event(&event);
    assert!(result.is_ok());
    assert_eq!(
        session.current_state, initial_state,
        "State must remain unchanged"
    );
    assert_eq!(
        session.state_entered_at, initial_entered_at,
        "State entry time must remain unchanged"
    );
    assert_eq!(
        session.last_seen_at, heartbeat_time,
        "last_seen_at must be updated"
    );
}

#[test]
fn test_session_terminated_event_exit_code_mapping() {
    // T115: Working -> Terminated mappings
    let mut session = create_test_session();
    let now = Utc::now() + Duration::milliseconds(5);
    // Transition to Working first
    session
        .apply_lifecycle_event(&SessionLifecycleEvent::StateTransition {
            session_id: session.session_id.clone(),
            new_state: LifecycleState::Working,
            tool_category: None,
            timestamp: now,
        })
        .unwrap();

    let term_time = now + Duration::milliseconds(5);
    // 1. Exit code Some(0) -> Success
    let mut s1 = session.clone();
    s1.apply_lifecycle_event(&SessionLifecycleEvent::SessionTerminated {
        session_id: s1.session_id.clone(),
        exit_code: Some(0),
        timestamp: term_time,
    })
    .unwrap();
    assert_eq!(s1.current_state, LifecycleState::Success);

    // 2. Exit code Some(1) -> Error
    let mut s2 = session.clone();
    s2.apply_lifecycle_event(&SessionLifecycleEvent::SessionTerminated {
        session_id: s2.session_id.clone(),
        exit_code: Some(1),
        timestamp: term_time,
    })
    .unwrap();
    assert_eq!(s2.current_state, LifecycleState::Error);

    // 3. Exit code None -> Cancelled
    let mut s3 = session.clone();
    s3.apply_lifecycle_event(&SessionLifecycleEvent::SessionTerminated {
        session_id: s3.session_id.clone(),
        exit_code: None,
        timestamp: term_time,
    })
    .unwrap();
    assert_eq!(s3.current_state, LifecycleState::Cancelled);
}
