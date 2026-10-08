use watchai_core::session::{AdapterStatus, AgentSession, SessionRegistry};
use watchai_core::state::LifecycleState;
use watchai_ipc::dbus_service::WatchAiDbusService;
use watchai_ipc::protocol::SessionDto;

#[tokio::test]
async fn test_dbus_service_initial_state() {
    let registry = SessionRegistry::new();
    let service = WatchAiDbusService::new(registry);

    let agg = service.aggregate();
    let r = agg.read().await;
    assert_eq!(r.state, "IDLE");
    assert_eq!(r.active_session_count, 0);
    assert_eq!(r.waiting_session_count, 0);
    assert_eq!(r.error_session_count, 0);
}

#[tokio::test]
async fn test_dbus_service_session_queries() {
    let registry = SessionRegistry::new();
    let session = AgentSession::new(
        "sess-ipc-100".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/my-cool-project",
        Some(7777),
        LifecycleState::Working,
        AdapterStatus::Active,
    );

    registry.upsert(session.clone()).await;

    let _service = WatchAiDbusService::new(registry.clone());

    // Query all sessions
    let list = registry.list().await;
    assert_eq!(list.len(), 1);
    let dto = SessionDto::from(&list[0]);
    assert_eq!(dto.session_id, "sess-ipc-100");
    assert_eq!(dto.provider_id, "claude-code");
    assert_eq!(dto.current_state, "WORKING");
    assert_eq!(dto.process_id, 7777);
    assert_eq!(dto.project_name, "my-cool-project");

    // Remove session
    registry.remove("sess-ipc-100").await;
    assert_eq!(registry.count().await, 0);
}
