use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use watchai_adapters::claude_code::{
    map_claude_hook_event, map_tool_category, ClaudeCodeAdapter, ClaudeTelemetryPayload,
};
use watchai_adapters::registry::AdapterRegistry;
use watchai_adapters::traits::{EventSink, ProviderAdapter};
use watchai_core::session::{
    process_lifecycle_event, AdapterStatus, AgentSession, SessionRegistry, ToolCategory,
};
use watchai_core::state::LifecycleState;
use watchai_daemon::hook_installer::{
    install_claude_hooks, status_claude_hooks, uninstall_claude_hooks,
};
use watchai_daemon::telemetry_socket::run_telemetry_listener;

#[test]
fn test_claude_wire_format_parsing_and_sanitization() {
    // 1. Valid payload deserialization
    let json_str = r#"{
        "pid": 1450709,
        "claude_session_id": "cd86568a-4cee-4029-bd3e-3b3d44e483ec",
        "cwd": "/home/user/Projects/WatchAI",
        "hook_event": "PreToolUse",
        "tool_name": "Bash"
    }"#;
    let mut payload: ClaudeTelemetryPayload = serde_json::from_str(json_str).unwrap();
    assert_eq!(payload.pid, 1450709);
    assert_eq!(
        payload.claude_session_id.as_deref(),
        Some("cd86568a-4cee-4029-bd3e-3b3d44e483ec")
    );
    assert_eq!(payload.cwd.as_deref(), Some("/home/user/Projects/WatchAI"));
    assert_eq!(payload.hook_event, "PreToolUse");
    assert_eq!(payload.tool_name.as_deref(), Some("Bash"));
    assert_eq!(payload.exit_reason, None);

    // 2. Length bounding and sanitization
    payload.claude_session_id = Some("s".repeat(300));
    payload.cwd = Some("w".repeat(2000));
    payload.tool_name = Some("t".repeat(200));
    payload.hook_event = "h".repeat(100);
    payload.sanitize();
    assert_eq!(payload.claude_session_id.unwrap().len(), 128);
    assert_eq!(payload.cwd.unwrap().len(), 1024);
    assert_eq!(payload.tool_name.unwrap().len(), 64);
    assert_eq!(payload.hook_event.len(), 64);

    // 3. Malformed JSON rejection
    assert!(serde_json::from_str::<ClaudeTelemetryPayload>("{ invalid json }").is_err());
    assert!(serde_json::from_str::<ClaudeTelemetryPayload>(r#"{"pid": "not-a-number"}"#).is_err());
}

#[test]
fn test_claude_hook_event_state_mapping() {
    // UserPromptSubmit -> Working
    let (state, tool) =
        map_claude_hook_event("UserPromptSubmit", None, None, LifecycleState::Idle).unwrap();
    assert_eq!(state, LifecycleState::Working);
    assert_eq!(tool, None);

    // PreToolUse with tools
    assert_eq!(
        map_claude_hook_event("PreToolUse", Some("Bash"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, Some(ToolCategory::ShellExecution))
    );
    assert_eq!(
        map_claude_hook_event("PreToolUse", Some("Read"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, Some(ToolCategory::FileRead))
    );
    assert_eq!(
        map_claude_hook_event("PreToolUse", Some("Edit"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, Some(ToolCategory::FileWrite))
    );
    assert_eq!(
        map_claude_hook_event("PreToolUse", Some("Write"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, Some(ToolCategory::FileWrite))
    );
    assert_eq!(
        map_claude_hook_event("PreToolUse", Some("Grep"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, Some(ToolCategory::Search))
    );
    assert_eq!(
        map_claude_hook_event("PreToolUse", Some("Agent"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, Some(ToolCategory::ModelThinking))
    );
    assert_eq!(
        map_claude_hook_event(
            "PreToolUse",
            Some("CustomTool"),
            None,
            LifecycleState::Working
        )
        .unwrap(),
        (LifecycleState::Working, None)
    );

    // PostToolUse -> Working without active tool
    assert_eq!(
        map_claude_hook_event("PostToolUse", Some("Bash"), None, LifecycleState::Working).unwrap(),
        (LifecycleState::Working, None)
    );

    // PermissionRequest -> Waiting
    assert_eq!(
        map_claude_hook_event(
            "PermissionRequest",
            Some("Bash"),
            None,
            LifecycleState::Working
        )
        .unwrap(),
        (LifecycleState::Waiting, None)
    );

    // PermissionDenied -> Ignored / None (no blind waiting)
    assert!(map_claude_hook_event(
        "PermissionDenied",
        Some("Bash"),
        None,
        LifecycleState::Waiting
    )
    .is_none());

    // PostToolUseFailure -> Error
    assert_eq!(
        map_claude_hook_event(
            "PostToolUseFailure",
            Some("Bash"),
            None,
            LifecycleState::Working
        )
        .unwrap(),
        (LifecycleState::Error, None)
    );

    // Stop -> Success
    assert_eq!(
        map_claude_hook_event("Stop", None, None, LifecycleState::Working).unwrap(),
        (LifecycleState::Success, None)
    );

    // StopFailure -> Error
    assert_eq!(
        map_claude_hook_event("StopFailure", None, None, LifecycleState::Working).unwrap(),
        (LifecycleState::Error, None)
    );

    // SessionEnd -> Success / Cancelled / Error
    assert_eq!(
        map_claude_hook_event(
            "SessionEnd",
            None,
            Some("prompt_input_exit"),
            LifecycleState::Working
        )
        .unwrap(),
        (LifecycleState::Success, None)
    );
    assert_eq!(
        map_claude_hook_event("SessionEnd", None, Some("error"), LifecycleState::Working).unwrap(),
        (LifecycleState::Error, None)
    );
}

#[tokio::test]
async fn test_fsm_recovery_through_subsequent_events() {
    let registry = SessionRegistry::new();
    let session = AgentSession::new(
        "proc-recovery-1".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/test",
        Some(5555),
        LifecycleState::Idle,
        AdapterStatus::Active,
    );
    registry.upsert(session).await;

    let adapter = ClaudeCodeAdapter::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(20);
    adapter.attach_event_sink(EventSink::new(tx, None));

    // 1. Prompt submit: Idle -> Working
    let p1 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p1).await.unwrap();
    let ev1 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev1).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Working
    );

    // 2. PermissionRequest: Working -> Waiting
    let p2 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "PermissionRequest".to_string(),
        tool_name: Some("Bash".to_string()),
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p2).await.unwrap();
    let ev2 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev2).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Waiting
    );

    // 3. User approves, tool executes: Waiting -> Working (Recovery from Waiting)
    let p3 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "PreToolUse".to_string(),
        tool_name: Some("Bash".to_string()),
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p3).await.unwrap();
    let ev3 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev3).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Working
    );

    // 4. Tool fails: Working -> Error
    let p4 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "PostToolUseFailure".to_string(),
        tool_name: Some("Bash".to_string()),
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p4).await.unwrap();
    let ev4 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev4).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Error
    );

    // 5. User provides follow-up prompt: Error -> Working (Recovery from Error)
    let p5 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p5).await.unwrap();
    let ev5 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev5).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Working
    );

    // 6. Turn completes: Working -> Success
    let p6 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "Stop".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p6).await.unwrap();
    let ev6 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev6).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Success
    );

    // 7. User enters second prompt while in Success: Success -> Working (Multi-turn continuity)
    let p7 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p7).await.unwrap();
    let ev7 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev7).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Working
    );

    // 7b. Silence timeout occurs: Working -> Unknown
    let mut s = registry.get("proc-recovery-1").await.unwrap();
    s.transition_to(LifecycleState::Unknown, s.sequence_number + 1, None)
        .unwrap();
    registry.upsert(s).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Unknown
    );

    // 7c. Turn completes while in Unknown: Unknown -> Success (Authoritative recovery)
    let p7c = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "Stop".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, p7c).await.unwrap();
    let ev7c = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev7c).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Success
    );

    // 8. Session terminates cleanly: SessionEnd -> SessionTerminated -> Success
    let p8 = ClaudeTelemetryPayload {
        pid: 5555,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "SessionEnd".to_string(),
        tool_name: None,
        exit_reason: Some("prompt_input_exit".to_string()),
    };
    adapter.handle_telemetry(&registry, p8).await.unwrap();
    let ev8 = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev8).await;
    assert_eq!(
        registry.get("proc-recovery-1").await.unwrap().current_state,
        LifecycleState::Success
    );
    assert!(registry.get("proc-recovery-1").await.unwrap().is_terminated);
}

#[tokio::test]
async fn test_session_correlation_and_isolation() {
    let registry = SessionRegistry::new();
    let session_a = AgentSession::new(
        "proc-sess-a".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/shared",
        Some(1111),
        LifecycleState::Idle,
        AdapterStatus::Active,
    );
    let session_b = AgentSession::new(
        "proc-sess-b".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/shared", // Same project, different PID!
        Some(2222),
        LifecycleState::Idle,
        AdapterStatus::Active,
    );
    registry.upsert(session_a).await;
    registry.upsert(session_b).await;

    let adapter = ClaudeCodeAdapter::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    adapter.attach_event_sink(EventSink::new(tx, None));

    // Send event for Session A
    let payload_a = ClaudeTelemetryPayload {
        pid: 1111,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: Some("/workspace/shared".to_string()),
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    let res_a = adapter
        .handle_telemetry(&registry, payload_a)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(res_a.session_id(), "proc-sess-a");

    let ev = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev).await;

    // Verify Session A is Working, Session B remains Idle
    assert_eq!(
        registry.get("proc-sess-a").await.unwrap().current_state,
        LifecycleState::Working
    );
    assert_eq!(
        registry.get("proc-sess-b").await.unwrap().current_state,
        LifecycleState::Idle
    );

    // Send event with unknown PID -> safely ignored without mutating registry
    let payload_unknown = ClaudeTelemetryPayload {
        pid: 999999,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    let res_un = adapter
        .handle_telemetry(&registry, payload_unknown)
        .await
        .unwrap();
    assert!(res_un.is_none());
}

#[tokio::test]
async fn test_activity_knowledge_boundary_preserved() {
    let registry = SessionRegistry::new();
    // 1. Process discovery creates session in IDLE
    let session = AgentSession::new(
        "proc-discovery-only".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/test",
        Some(7777),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );
    registry.upsert(session).await;

    assert_eq!(
        registry
            .get("proc-discovery-only")
            .await
            .unwrap()
            .current_state,
        LifecycleState::Idle,
        "Discovered process MUST strictly initialize in IDLE"
    );

    // 2. Only authoritative telemetry can transition to WORKING
    let adapter = ClaudeCodeAdapter::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    adapter.attach_event_sink(EventSink::new(tx, None));

    let payload = ClaudeTelemetryPayload {
        pid: 7777,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "PreToolUse".to_string(),
        tool_name: Some("Bash".to_string()),
        exit_reason: None,
    };
    adapter.handle_telemetry(&registry, payload).await.unwrap();
    let ev = rx.recv().await.unwrap();
    process_lifecycle_event(&registry, &ev).await;

    assert_eq!(
        registry
            .get("proc-discovery-only")
            .await
            .unwrap()
            .current_state,
        LifecycleState::Working,
        "Session transitions to WORKING only upon verified telemetry"
    );
}

#[tokio::test]
async fn test_telemetry_socket_listener_and_oversized_rejection() {
    let temp_dir = std::env::temp_dir().join(format!("watchai-sock-full-{}", std::process::id()));
    let sock_path = temp_dir.join("events.sock");

    let mut adapters = AdapterRegistry::default_registry();
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    adapters.attach_event_sink(EventSink::new(tx, None));
    let srv_adapter = Arc::new(adapters);

    let registry = SessionRegistry::new();
    let session = AgentSession::new(
        "proc-sock-test".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/sock",
        Some(8888),
        LifecycleState::Idle,
        AdapterStatus::Active,
    );
    registry.upsert(session).await;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let srv_path = sock_path.clone();
    let srv_registry = registry.clone();

    let srv_handle = tokio::spawn(async move {
        run_telemetry_listener(srv_path, srv_adapter, srv_registry, shutdown_rx).await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Send valid payload
    let mut client = std::os::unix::net::UnixStream::connect(&sock_path).unwrap();
    let payload = ClaudeTelemetryPayload {
        pid: 8888,
        provider_id: Some("claude-code".to_string()),
        claude_session_id: None,
        cwd: None,
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    writeln!(client, "{}", serde_json::to_string(&payload).unwrap()).unwrap();
    drop(client);

    let ev = rx.recv().await.unwrap();
    assert_eq!(ev.session_id(), "proc-sock-test");

    // Send oversized line (> 64 KiB)
    let mut bad_client = std::os::unix::net::UnixStream::connect(&sock_path).unwrap();
    let oversized = "X".repeat(70 * 1024) + "\n";
    let _ = bad_client.write_all(oversized.as_bytes());
    drop(bad_client);

    // Ensure no spurious events arrived from oversized line
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .is_err());

    shutdown_tx.send(true).unwrap();
    let _ = srv_handle.await;
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_hook_installer_and_status() {
    assert_eq!(
        map_tool_category("Bash"),
        Some(ToolCategory::ShellExecution)
    );

    let temp_dir =
        std::env::temp_dir().join(format!("watchai-claude-inst-test-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let settings_file = temp_dir.join("settings.json");
    let daemon_bin = PathBuf::from("/usr/bin/watchai-daemon");

    let rep = install_claude_hooks(&settings_file, &daemon_bin).unwrap();
    assert_eq!(rep.events_registered.len(), 8);

    let statuses = status_claude_hooks(&settings_file, &daemon_bin);
    assert_eq!(statuses.len(), 8);
    for (_evt, present) in statuses {
        assert!(present);
    }

    let un_rep = uninstall_claude_hooks(&settings_file, &daemon_bin).unwrap();
    assert_eq!(un_rep.events_removed.len(), 8);

    let _ = fs::remove_dir_all(&temp_dir);
}
