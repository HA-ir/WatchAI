use crate::discovery::ProcessScanner;
use crate::traits::{
    DiscoveredSession, EventSink, IngestionError, ProviderAdapter, ProviderCapabilities,
    TelemetryTier,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, trace};
use watchai_core::session::{
    derive_process_session_id, AdapterStatus, AgentSession, SessionLifecycleEvent, SessionRegistry,
    ToolCategory,
};
use watchai_core::state::LifecycleState;

/// Sanitized, metadata-only telemetry event emitted by hook forwarders or wrappers.
/// Strictly non-invasive: contains no prompts, tool inputs, responses, or secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClaudeTelemetryPayload {
    /// Operating system Process ID of the agent instance.
    pub pid: u32,
    /// Canonical provider slug (e.g. "claude-code", "opencode", "codex-cli").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Provider-internal session UUID (from session_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    /// Active project directory path (from cwd / project directory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Hook or wrapper event name (e.g. "UserPromptSubmit", "PreToolUse", "PermissionRequest", "Stop", "SessionEnd").
    pub hook_event: String,
    /// Tool name for tool-use events (e.g. "Bash", "Edit", "Read").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Session termination reason if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}

/// Generic alias for multi-provider telemetry payloads.
pub type ProviderTelemetryPayload = ClaudeTelemetryPayload;

impl ClaudeTelemetryPayload {
    /// Returns the resolved provider slug, defaulting to "claude-code".
    pub fn effective_provider_id(&self) -> &str {
        self.provider_id.as_deref().unwrap_or("claude-code")
    }

    /// Enforces maximum length bounds on string fields to prevent memory bloat.
    pub fn sanitize(&mut self) {
        if let Some(ref mut p) = self.provider_id {
            if p.len() > 64 {
                p.truncate(64);
            }
        }
        if let Some(ref mut s) = self.claude_session_id {
            if s.len() > 128 {
                s.truncate(128);
            }
        }
        if let Some(ref mut c) = self.cwd {
            if c.len() > 1024 {
                c.truncate(1024);
            }
        }
        if self.hook_event.len() > 64 {
            self.hook_event.truncate(64);
        }
        if let Some(ref mut t) = self.tool_name {
            if t.len() > 64 {
                t.truncate(64);
            }
        }
        if let Some(ref mut r) = self.exit_reason {
            if r.len() > 64 {
                r.truncate(64);
            }
        }
    }
}

/// Maps tool names across providers to high-level sanitized ToolCategory variants.
pub fn map_tool_category(tool_name: &str) -> Option<ToolCategory> {
    match tool_name.to_lowercase().as_str() {
        "bash" | "shell" | "terminal" | "exec" | "command" | "cmd" | "sh" => {
            Some(ToolCategory::ShellExecution)
        }
        "read" | "fileread" | "view" | "cat" | "read_file" => Some(ToolCategory::FileRead),
        "edit" | "write" | "notebookedit" | "filewrite" | "write_file" | "apply_patch"
        | "patch" => Some(ToolCategory::FileWrite),
        "glob" | "grep" | "search" | "find" | "find_files" | "search_files" => {
            Some(ToolCategory::Search)
        }
        "agent" | "askuserquestion" | "think" | "thinking" | "plan" => {
            Some(ToolCategory::ModelThinking)
        }
        _ => None,
    }
}

/// Maps native Claude Code hook events to WatchAI LifecycleState and optional ToolCategory.
/// Respects existing WatchAI FSM transition legality.
pub fn map_claude_hook_event(
    hook_event: &str,
    tool_name: Option<&str>,
    exit_reason: Option<&str>,
    _current_state: LifecycleState,
) -> Option<(LifecycleState, Option<ToolCategory>)> {
    match hook_event {
        "UserPromptSubmit" => {
            // User submitted prompt; agent turn actively begins
            Some((LifecycleState::Working, None))
        }
        "PreToolUse" => {
            // Tool is executing; map high-level tool category
            let category = tool_name.and_then(map_tool_category);
            Some((LifecycleState::Working, category))
        }
        "PostToolUse" => {
            // Tool completed; agent continuing turn without active tool
            Some((LifecycleState::Working, None))
        }
        "PermissionRequest" => {
            // Agent is blocked waiting for user approval/permission
            Some((LifecycleState::Waiting, None))
        }
        "PermissionDenied" => {
            // Do NOT blindly set WAITING; subsequent authoritative events dictate state
            None
        }
        "PostToolUseFailure" => {
            // Tool execution failure
            Some((LifecycleState::Error, None))
        }
        "Stop" => {
            // Turn completed cleanly. Enters Success and participates in completion dwell.
            Some((LifecycleState::Success, None))
        }
        "StopFailure" => {
            // Turn completed with failure/error
            Some((LifecycleState::Error, None))
        }
        "SessionEnd" => {
            let state = match exit_reason {
                Some("prompt_input_exit") | Some("clear") | Some("logout") => {
                    LifecycleState::Success
                }
                Some("error") => LifecycleState::Error,
                _ => LifecycleState::Success,
            };
            Some((state, None))
        }
        "SessionStart" => {
            // Session established; initialize in Idle
            Some((LifecycleState::Idle, None))
        }
        _ => None,
    }
}

/// Provider adapter for Anthropic's Claude Code CLI agent.
pub struct ClaudeCodeAdapter {
    event_sink: RwLock<Option<EventSink>>,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self {
            event_sink: RwLock::new(None),
        }
    }

    /// Processes an incoming telemetry payload from the Claude Code hook forwarder.
    /// Correlates to an active session by PID, validates FSM transition, and ingests via EventSink.
    pub async fn handle_telemetry(
        &self,
        registry: &SessionRegistry,
        mut payload: ClaudeTelemetryPayload,
    ) -> Result<Option<SessionLifecycleEvent>, IngestionError> {
        if payload.pid == 0 {
            debug!("Rejected telemetry payload with invalid PID 0");
            return Ok(None);
        }
        payload.sanitize();

        // 1. Correlate to an existing session in SessionRegistry by PID
        let sessions = registry.list().await;
        let existing = sessions
            .into_iter()
            .find(|s| s.process_id == Some(payload.pid) && s.provider_id == "claude-code");

        let session = match existing {
            Some(s) => s,
            None => {
                // Case C: Hook arrived before periodic 2s /proc discovery sweep ran.
                // Reconcile via /proc inspection for this PID:
                let proc_root = watchai_core::liveness::resolve_proc_root();
                let proc_path = proc_root.join(payload.pid.to_string());
                if !proc_path.is_dir() {
                    debug!(
                        "Rejected telemetry for non-existent /proc PID {}",
                        payload.pid
                    );
                    return Ok(None);
                }

                let start_time = match ProcessScanner::read_process_start_time(&proc_path) {
                    Some(st) if st > 0 => st,
                    _ => {
                        debug!("Could not read valid start time for PID {}", payload.pid);
                        return Ok(None);
                    }
                };

                let project_path = match std::fs::read_link(proc_path.join("cwd")) {
                    Ok(target) => target,
                    Err(_) => {
                        if let Some(cwd) = &payload.cwd {
                            PathBuf::from(cwd)
                        } else {
                            PathBuf::from("/unknown/workspace")
                        }
                    }
                };

                let session_id = derive_process_session_id(payload.pid, start_time, &project_path);
                let mut new_session = AgentSession::new(
                    session_id.clone(),
                    self.provider_id(),
                    self.display_name(),
                    &project_path,
                    Some(payload.pid),
                    LifecycleState::Idle,
                    AdapterStatus::Active,
                );
                new_session = new_session.with_process_start_time(Some(start_time));
                registry.upsert(new_session.clone()).await;
                new_session
            }
        };

        // 2. Map hook event to LifecycleState and ToolCategory
        let (target_state, tool_category) = match map_claude_hook_event(
            &payload.hook_event,
            payload.tool_name.as_deref(),
            payload.exit_reason.as_deref(),
            session.current_state,
        ) {
            Some(mapping) => mapping,
            None => {
                trace!(
                    "Ignored unmapped hook event '{}' for PID {}",
                    payload.hook_event,
                    payload.pid
                );
                return Ok(None);
            }
        };

        // 3. Construct canonical SessionLifecycleEvent
        let event = if payload.hook_event == "SessionEnd" {
            let exit_code = match payload.exit_reason.as_deref() {
                Some("error") => Some(1),
                _ => Some(0),
            };
            SessionLifecycleEvent::SessionTerminated {
                session_id: session.session_id,
                exit_code,
                timestamp: Utc::now(),
            }
        } else {
            SessionLifecycleEvent::StateTransition {
                session_id: session.session_id,
                new_state: target_state,
                tool_category,
                timestamp: Utc::now(),
            }
        };

        // 4. Ingest into attached EventSink
        let sink = self.event_sink.read().unwrap().clone();
        if let Some(sink) = sink {
            sink.ingest(event.clone()).await?;
        }

        Ok(Some(event))
    }
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderAdapter for ClaudeCodeAdapter {
    fn provider_id(&self) -> &'static str {
        "claude-code"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            telemetry_tier: TelemetryTier::OptInHookTelemetry,
            supports_tool_categories: true,
            supports_activity_events: true,
        }
    }

    fn attach_event_sink(&self, sink: EventSink) {
        if let Ok(mut guard) = self.event_sink.write() {
            *guard = Some(sink);
        }
    }

    async fn check_environment(&self) -> AdapterStatus {
        // Check if `claude` binary is in PATH
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                if dir.join("claude").is_file() {
                    return AdapterStatus::Active;
                }
            }
        }

        // Also check default npm/homebrew paths
        if let Ok(home) = std::env::var("HOME") {
            let home_bin = std::path::Path::new(&home).join(".local/bin/claude");
            if home_bin.is_file() {
                return AdapterStatus::Active;
            }
        }

        debug!("Claude Code binary not found in standard paths");
        AdapterStatus::DiscoveryRequired
    }

    async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
        // Use non-invasive /proc scanner targeting "claude"
        ProcessScanner::scan_processes("claude", self.provider_id(), self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_adapter_retains_attached_event_sink() {
        let adapter = ClaudeCodeAdapter::new();
        assert!(adapter.event_sink.read().unwrap().is_none());

        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let sink = EventSink::new(tx, None);

        adapter.attach_event_sink(sink);
        assert!(adapter.event_sink.read().unwrap().is_some());
    }

    #[test]
    fn test_claude_adapter_capabilities_reflect_opt_in_hook_telemetry() {
        let adapter = ClaudeCodeAdapter::new();
        let caps = adapter.capabilities();
        assert_eq!(caps.telemetry_tier, TelemetryTier::OptInHookTelemetry);
        assert!(caps.supports_tool_categories);
        assert!(caps.supports_activity_events);
    }

    #[test]
    fn test_map_tool_category() {
        assert_eq!(
            map_tool_category("Bash"),
            Some(ToolCategory::ShellExecution)
        );
        assert_eq!(map_tool_category("Read"), Some(ToolCategory::FileRead));
        assert_eq!(map_tool_category("Edit"), Some(ToolCategory::FileWrite));
        assert_eq!(map_tool_category("Write"), Some(ToolCategory::FileWrite));
        assert_eq!(
            map_tool_category("NotebookEdit"),
            Some(ToolCategory::FileWrite)
        );
        assert_eq!(map_tool_category("Glob"), Some(ToolCategory::Search));
        assert_eq!(map_tool_category("Grep"), Some(ToolCategory::Search));
        assert_eq!(
            map_tool_category("Agent"),
            Some(ToolCategory::ModelThinking)
        );
        assert_eq!(
            map_tool_category("AskUserQuestion"),
            Some(ToolCategory::ModelThinking)
        );
        assert_eq!(map_tool_category("UnknownCustomTool"), None);
    }

    #[test]
    fn test_map_claude_hook_event_state_transitions() {
        // UserPromptSubmit -> Working
        let (state, tool) =
            map_claude_hook_event("UserPromptSubmit", None, None, LifecycleState::Idle).unwrap();
        assert_eq!(state, LifecycleState::Working);
        assert_eq!(tool, None);

        // PreToolUse -> Working with tool category
        let (state, tool) =
            map_claude_hook_event("PreToolUse", Some("Bash"), None, LifecycleState::Working)
                .unwrap();
        assert_eq!(state, LifecycleState::Working);
        assert_eq!(tool, Some(ToolCategory::ShellExecution));

        // PostToolUse -> Working without active tool
        let (state, tool) =
            map_claude_hook_event("PostToolUse", Some("Bash"), None, LifecycleState::Working)
                .unwrap();
        assert_eq!(state, LifecycleState::Working);
        assert_eq!(tool, None);

        // PermissionRequest -> Waiting
        let (state, tool) = map_claude_hook_event(
            "PermissionRequest",
            Some("Bash"),
            None,
            LifecycleState::Working,
        )
        .unwrap();
        assert_eq!(state, LifecycleState::Waiting);
        assert_eq!(tool, None);

        // PermissionDenied -> Ignored / None
        assert!(map_claude_hook_event(
            "PermissionDenied",
            Some("Bash"),
            None,
            LifecycleState::Waiting
        )
        .is_none());

        // PostToolUseFailure -> Error
        let (state, tool) = map_claude_hook_event(
            "PostToolUseFailure",
            Some("Bash"),
            None,
            LifecycleState::Working,
        )
        .unwrap();
        assert_eq!(state, LifecycleState::Error);
        assert_eq!(tool, None);

        // Stop -> Success (turn completed successfully)
        let (state, tool) =
            map_claude_hook_event("Stop", None, None, LifecycleState::Working).unwrap();
        assert_eq!(state, LifecycleState::Success);
        assert_eq!(tool, None);

        // StopFailure -> Error (turn completed with failure)
        let (state, tool) =
            map_claude_hook_event("StopFailure", None, None, LifecycleState::Working).unwrap();
        assert_eq!(state, LifecycleState::Error);
        assert_eq!(tool, None);

        // SessionStart -> Idle
        let (state, tool) =
            map_claude_hook_event("SessionStart", None, None, LifecycleState::Unknown).unwrap();
        assert_eq!(state, LifecycleState::Idle);
        assert_eq!(tool, None);
    }

    #[tokio::test]
    async fn test_handle_telemetry_correlates_by_pid_and_updates_state() {
        let registry = SessionRegistry::new();
        let session = AgentSession::new(
            "proc-test-123".to_string(),
            "claude-code",
            "Claude Code",
            "/home/user/project",
            Some(4242),
            LifecycleState::Idle,
            AdapterStatus::DiscoveryRequired,
        );
        registry.upsert(session).await;

        let adapter = ClaudeCodeAdapter::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        adapter.attach_event_sink(EventSink::new(tx, None));

        let payload = ClaudeTelemetryPayload {
            pid: 4242,
            provider_id: Some("claude-code".to_string()),
            claude_session_id: Some("uuid-1".to_string()),
            cwd: Some("/home/user/project".to_string()),
            hook_event: "UserPromptSubmit".to_string(),
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
                assert_eq!(session_id, "proc-test-123");
                assert_eq!(new_state, LifecycleState::Working);
                assert_eq!(tool_category, None);
            }
            _ => panic!("Expected StateTransition event"),
        }

        // Verify event was received in sink
        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id(), "proc-test-123");
    }

    #[tokio::test]
    async fn test_handle_telemetry_isolates_independent_sessions() {
        let registry = SessionRegistry::new();
        let session_a = AgentSession::new(
            "proc-session-a".to_string(),
            "claude-code",
            "Claude Code",
            "/workspace/proj",
            Some(1001),
            LifecycleState::Idle,
            AdapterStatus::DiscoveryRequired,
        );
        let session_b = AgentSession::new(
            "proc-session-b".to_string(),
            "claude-code",
            "Claude Code",
            "/workspace/proj", // Same project, different PID!
            Some(1002),
            LifecycleState::Idle,
            AdapterStatus::DiscoveryRequired,
        );
        registry.upsert(session_a).await;
        registry.upsert(session_b).await;

        let adapter = ClaudeCodeAdapter::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        adapter.attach_event_sink(EventSink::new(tx, None));

        // Telemetry for PID 1001 only
        let payload = ClaudeTelemetryPayload {
            pid: 1001,
            provider_id: Some("claude-code".to_string()),
            claude_session_id: None,
            cwd: Some("/workspace/proj".to_string()),
            hook_event: "PreToolUse".to_string(),
            tool_name: Some("Bash".to_string()),
            exit_reason: None,
        };

        let event = adapter
            .handle_telemetry(&registry, payload)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.session_id(), "proc-session-a");

        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id(), "proc-session-a");

        // Verify session_b in registry remains untouched
        let b = registry.get("proc-session-b").await.unwrap();
        assert_eq!(b.current_state, LifecycleState::Idle);
    }

    #[test]
    fn test_payload_sanitization_bounds_field_lengths() {
        let mut payload = ClaudeTelemetryPayload {
            pid: 500,
            provider_id: Some("a".repeat(100)),
            claude_session_id: Some("a".repeat(200)),
            cwd: Some("b".repeat(2000)),
            hook_event: "c".repeat(100),
            tool_name: Some("d".repeat(100)),
            exit_reason: Some("e".repeat(100)),
        };
        payload.sanitize();
        assert_eq!(payload.provider_id.unwrap().len(), 64);
        assert_eq!(payload.claude_session_id.unwrap().len(), 128);
        assert_eq!(payload.cwd.unwrap().len(), 1024);
        assert_eq!(payload.hook_event.len(), 64);
        assert_eq!(payload.tool_name.unwrap().len(), 64);
        assert_eq!(payload.exit_reason.unwrap().len(), 64);
    }
}
