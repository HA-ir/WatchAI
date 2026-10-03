use std::collections::HashSet;
use watchai_core::session::{derive_process_session_id, AdapterStatus, AgentSession, ToolCategory};
use watchai_core::state::LifecycleState;

#[test]
fn test_data_sanitization_whitelist() {
    let session = AgentSession::new(
        "sess-sanitization-1".to_string(),
        "claude-code",
        "Claude Code",
        "/home/user/super-secret-project",
        Some(4321),
        LifecycleState::Working,
        AdapterStatus::Active,
    );

    let json_val = serde_json::to_value(&session).expect("Failed to serialize session");
    let json_obj = json_val.as_object().expect("Expected JSON object");

    // The strict whitelist of permitted schema fields
    let allowed_fields: HashSet<&str> = [
        "session_id",
        "provider_id",
        "provider_display_name",
        "project_name",
        "project_path",
        "current_state",
        "sequence_number",
        "started_at",
        "state_entered_at",
        "last_seen_at",
        "process_id",
        "active_tool_category",
        "adapter_status",
    ]
    .into_iter()
    .collect();

    // Verify all present fields are in the whitelist
    for key in json_obj.keys() {
        assert!(
            allowed_fields.contains(key.as_str()),
            "Security violation: unexpected field '{key}' in serialized AgentSession!"
        );
    }

    // Verify prohibited field classes are completely absent
    let prohibited_patterns = [
        "prompt", "code", "token", "key", "secret", "stdout", "stderr", "output", "query", "diff",
    ];
    for key in json_obj.keys() {
        for pattern in &prohibited_patterns {
            assert!(
                !key.to_lowercase().contains(pattern),
                "Security violation: key '{key}' contains sensitive pattern '{pattern}'"
            );
        }
    }
}

#[test]
fn test_tool_category_serialization() {
    let mut session = AgentSession::new(
        "sess-tool-cat".to_string(),
        "opencode",
        "OpenCode",
        "/home/user/app",
        Some(1111),
        LifecycleState::Working,
        AdapterStatus::Active,
    );

    session
        .transition_to(
            LifecycleState::Working,
            2,
            Some(ToolCategory::ShellExecution),
        )
        .unwrap();

    let json_str = serde_json::to_string(&session).unwrap();
    assert!(json_str.contains("\"active_tool_category\":\"SHELL_EXECUTION\""));
    // Verify tool arguments or command text cannot be stored
    assert!(!json_str.contains("args"));
    assert!(!json_str.contains("command"));
}

#[test]
fn test_deterministic_process_id_derivation() {
    let id1 = derive_process_session_id(12345, 1000000, "/home/user/my-project");
    let id2 = derive_process_session_id(12345, 1000000, "/home/user/my-project");
    let id3 = derive_process_session_id(12345, 1000001, "/home/user/my-project");

    assert_eq!(
        id1, id2,
        "Identical process attributes must yield identical session IDs"
    );
    assert_ne!(
        id1, id3,
        "Different process start times must yield different session IDs"
    );
    assert!(id1.starts_with("proc-"));
}
