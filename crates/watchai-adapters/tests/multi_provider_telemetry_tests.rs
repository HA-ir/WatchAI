use watchai_adapters::claude_code::ProviderTelemetryPayload;
use watchai_adapters::codex_cli::{map_codex_event, CodexCliAdapter};
use watchai_adapters::opencode::{map_opencode_event, OpenCodeAdapter};
use watchai_adapters::registry::AdapterRegistry;
use watchai_adapters::traits::{EventSink, ProviderAdapter};
use watchai_core::session::{AgentSession, SessionLifecycleEvent, SessionRegistry, ToolCategory};
use watchai_core::state::LifecycleState;

#[test]
fn test_opencode_event_mapping() {
    // OpenCode events mapping to WatchAI states
    let (state, tool) =
        map_opencode_event("CommandStart", None, None, LifecycleState::Idle).unwrap();
    assert_eq!(state, LifecycleState::Working);
    assert_eq!(tool, None);

    let (state, tool) = map_opencode_event(
        "ToolExecute",
        Some("read_file"),
        None,
        LifecycleState::Working,
    )
    .unwrap();
    assert_eq!(state, LifecycleState::Working);
    assert_eq!(tool, Some(ToolCategory::FileRead));

    let (state, tool) =
        map_opencode_event("PromptUser", None, None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Waiting);
    assert_eq!(tool, None);

    let (state, tool) =
        map_opencode_event("ToolFailure", None, None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Error);
    assert_eq!(tool, None);

    let (state, tool) = map_opencode_event("Done", None, None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Success);
    assert_eq!(tool, None);

    let (state, _) =
        map_opencode_event("SessionEnd", None, Some("error"), LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Error);

    let (state, _) = map_opencode_event("SessionEnd", None, None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Success);
}

#[test]
fn test_codex_event_mapping() {
    // Codex events mapping to WatchAI states
    let (state, tool) = map_codex_event("TurnStart", None, None, LifecycleState::Idle).unwrap();
    assert_eq!(state, LifecycleState::Working);
    assert_eq!(tool, None);

    let (state, tool) =
        map_codex_event("ToolExecute", Some("bash"), None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Working);
    assert_eq!(tool, Some(ToolCategory::ShellExecution));

    let (state, tool) =
        map_codex_event("ConfirmAction", None, None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Waiting);
    assert_eq!(tool, None);

    let (state, tool) = map_codex_event("Stop", None, None, LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Success);
    assert_eq!(tool, None);

    let (state, _) =
        map_codex_event("SessionEnd", None, Some("error"), LifecycleState::Working).unwrap();
    assert_eq!(state, LifecycleState::Error);
}

#[tokio::test]
async fn test_opencode_handle_telemetry_flow() {
    let registry = SessionRegistry::new();
    let session = AgentSession::new(
        "proc-opencode-1".to_string(),
        "opencode",
        "OpenCode",
        "/home/user/repo",
        Some(7771),
        LifecycleState::Idle,
        watchai_core::session::AdapterStatus::Active,
    );
    registry.upsert(session).await;

    let adapter = OpenCodeAdapter::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    adapter.attach_event_sink(EventSink::new(tx, None));

    // Send ToolExecute telemetry
    let payload = ProviderTelemetryPayload {
        pid: 7771,
        provider_id: Some("opencode".to_string()),
        claude_session_id: None,
        cwd: Some("/home/user/repo".to_string()),
        hook_event: "ToolExecute".to_string(),
        tool_name: Some("write_file".to_string()),
        exit_reason: None,
    };

    let event = adapter
        .handle_telemetry(&registry, payload)
        .await
        .unwrap()
        .unwrap();
    match event {
        SessionLifecycleEvent::StateTransition {
            session_id,
            new_state,
            tool_category,
            ..
        } => {
            assert_eq!(session_id, "proc-opencode-1");
            assert_eq!(new_state, LifecycleState::Working);
            assert_eq!(tool_category, Some(ToolCategory::FileWrite));
        }
        _ => panic!("Expected StateTransition"),
    }

    let received = rx.recv().await.unwrap();
    assert_eq!(received.session_id(), "proc-opencode-1");
}

#[tokio::test]
async fn test_codex_handle_telemetry_flow() {
    let registry = SessionRegistry::new();
    let session = AgentSession::new(
        "proc-codex-1".to_string(),
        "codex-cli",
        "OpenAI Codex",
        "/home/user/workspace",
        Some(8881),
        LifecycleState::Idle,
        watchai_core::session::AdapterStatus::Active,
    );
    registry.upsert(session).await;

    let adapter = CodexCliAdapter::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    adapter.attach_event_sink(EventSink::new(tx, None));

    // Send PermissionRequest telemetry
    let payload = ProviderTelemetryPayload {
        pid: 8881,
        provider_id: Some("codex-cli".to_string()),
        claude_session_id: None,
        cwd: Some("/home/user/workspace".to_string()),
        hook_event: "PermissionRequest".to_string(),
        tool_name: None,
        exit_reason: None,
    };

    let event = adapter
        .handle_telemetry(&registry, payload)
        .await
        .unwrap()
        .unwrap();
    match event {
        SessionLifecycleEvent::StateTransition {
            session_id,
            new_state,
            tool_category,
            ..
        } => {
            assert_eq!(session_id, "proc-codex-1");
            assert_eq!(new_state, LifecycleState::Waiting);
            assert_eq!(tool_category, None);
        }
        _ => panic!("Expected StateTransition"),
    }

    let received = rx.recv().await.unwrap();
    assert_eq!(received.session_id(), "proc-codex-1");
}

#[tokio::test]
async fn test_adapter_registry_routes_typed_adapters() {
    let registry = AdapterRegistry::default_registry();
    assert!(registry.claude_adapter().is_some());
    assert!(registry.opencode_adapter().is_some());
    assert!(registry.codex_adapter().is_some());

    assert_eq!(
        registry.claude_adapter().unwrap().provider_id(),
        "claude-code"
    );
    assert_eq!(
        registry.opencode_adapter().unwrap().provider_id(),
        "opencode"
    );
    assert_eq!(registry.codex_adapter().unwrap().provider_id(), "codex-cli");
}
