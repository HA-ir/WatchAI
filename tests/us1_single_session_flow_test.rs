use watchai_core::session::{AdapterStatus, AgentSession, SessionRegistry, ToolCategory};
use watchai_core::state::LifecycleState;
use watchai_ipc::dbus_service::WatchAiDbusService;
use watchai_ipc::protocol::SessionDto;

#[tokio::test]
async fn test_us1_single_session_lifecycle_flow() {
    let registry = SessionRegistry::new();
    let service = WatchAiDbusService::new(registry.clone());

    // 1. Initial State: IDLE
    let agg = service.aggregate();
    assert_eq!(agg.read().await.state, "IDLE");
    assert_eq!(registry.count().await, 0);

    // 2. Agent Discovered -> STARTING
    let mut session = AgentSession::new(
        "session-us1-flow".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/workspace/app",
        Some(4444),
        LifecycleState::Starting,
        AdapterStatus::Active,
    );
    registry.upsert(session.clone()).await;
    assert_eq!(registry.count().await, 1);

    // Verify DTO representation
    let dto = SessionDto::from(&session);
    assert_eq!(dto.current_state, "STARTING");
    assert_eq!(dto.provider_display_name, "Claude Code");
    assert_eq!(dto.project_name, "app");

    // 3. Agent begins working -> WORKING
    assert!(session
        .transition_to(LifecycleState::Working, 2, None)
        .is_ok());
    registry.upsert(session.clone()).await;
    assert_eq!(
        registry
            .get("session-us1-flow")
            .await
            .unwrap()
            .current_state,
        LifecycleState::Working
    );

    // 4. Agent pauses for user confirmation -> WAITING
    assert!(session
        .transition_to(LifecycleState::Waiting, 3, Some(ToolCategory::FileWrite))
        .is_ok());
    registry.upsert(session.clone()).await;
    let waiting_session = registry.get("session-us1-flow").await.unwrap();
    assert_eq!(waiting_session.current_state, LifecycleState::Waiting);
    assert_eq!(
        waiting_session.active_tool_category,
        Some(ToolCategory::FileWrite)
    );

    // 5. User grants permission -> WORKING
    assert!(session
        .transition_to(LifecycleState::Working, 4, None)
        .is_ok());
    registry.upsert(session.clone()).await;
    assert_eq!(
        registry
            .get("session-us1-flow")
            .await
            .unwrap()
            .current_state,
        LifecycleState::Working
    );

    // 6. Task completes successfully -> SUCCESS
    assert!(session
        .transition_to(LifecycleState::Success, 5, None)
        .is_ok());
    registry.upsert(session.clone()).await;
    assert_eq!(
        registry
            .get("session-us1-flow")
            .await
            .unwrap()
            .current_state,
        LifecycleState::Success
    );

    // 7. Prompt readiness / Dwell completion -> IDLE
    assert!(session.transition_to(LifecycleState::Idle, 6, None).is_ok());
    registry.upsert(session.clone()).await;
    assert_eq!(
        registry
            .get("session-us1-flow")
            .await
            .unwrap()
            .current_state,
        LifecycleState::Idle
    );
}
