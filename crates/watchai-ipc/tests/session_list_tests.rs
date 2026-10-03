use watchai_core::session::{AdapterStatus, AgentSession, SessionRegistry, ToolCategory};
use watchai_core::state::LifecycleState;
use watchai_ipc::dbus_service::WatchAiDbusService;
use watchai_ipc::protocol::SessionDto;

#[tokio::test]
async fn test_get_sessions_empty_list() {
    let registry = SessionRegistry::new();
    let service = WatchAiDbusService::new(registry.clone());

    // With zero sessions, list should be empty
    let sessions = registry.list().await;
    assert!(sessions.is_empty());
    assert_eq!(registry.count().await, 0);

    let agg = service.aggregate();
    assert_eq!(agg.read().await.active_session_count, 0);
}

#[tokio::test]
async fn test_get_sessions_with_multiple_states() {
    let registry = SessionRegistry::new();

    // Active session 1: WORKING
    let mut s1 = AgentSession::new(
        "sess-1-working".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/backend-service",
        Some(1010),
        LifecycleState::Starting,
        AdapterStatus::Active,
    );
    s1.transition_to(LifecycleState::Working, 2, Some(ToolCategory::FileRead))
        .unwrap();
    registry.upsert(s1).await;

    // Active session 2: WAITING
    let mut s2 = AgentSession::new(
        "sess-2-waiting".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/frontend-app",
        Some(2020),
        LifecycleState::Starting,
        AdapterStatus::Active,
    );
    s2.transition_to(LifecycleState::Working, 2, None).unwrap();
    s2.transition_to(
        LifecycleState::Waiting,
        3,
        Some(ToolCategory::ShellExecution),
    )
    .unwrap();
    registry.upsert(s2).await;

    // Completed session 3: SUCCESS
    let mut s3 = AgentSession::new(
        "sess-3-success".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/docs-site",
        Some(3030),
        LifecycleState::Starting,
        AdapterStatus::Active,
    );
    s3.transition_to(LifecycleState::Working, 2, None).unwrap();
    s3.transition_to(LifecycleState::Success, 3, None).unwrap();
    registry.upsert(s3).await;

    assert_eq!(registry.count().await, 3);

    let list = registry.list().await;
    let dtos: Vec<SessionDto> = list.iter().map(SessionDto::from).collect();

    assert_eq!(dtos.len(), 3);

    // Verify DTO fields
    let waiting_dto = dtos
        .iter()
        .find(|d| d.session_id == "sess-2-waiting")
        .unwrap();
    assert_eq!(waiting_dto.current_state, "WAITING");
    assert_eq!(waiting_dto.project_name, "frontend-app");
    assert_eq!(waiting_dto.process_id, 2020);
    assert_eq!(waiting_dto.active_tool_category, "SHELL_EXECUTION");

    let success_dto = dtos
        .iter()
        .find(|d| d.session_id == "sess-3-success")
        .unwrap();
    assert_eq!(success_dto.current_state, "SUCCESS");
    assert_eq!(success_dto.project_name, "docs-site");

    let working_dto = dtos
        .iter()
        .find(|d| d.session_id == "sess-1-working")
        .unwrap();
    assert_eq!(working_dto.current_state, "WORKING");
    assert_eq!(working_dto.project_name, "backend-service");
    assert_eq!(working_dto.active_tool_category, "FILE_READ");
}
