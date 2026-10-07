use crate::claude_code::{map_tool_category, ProviderTelemetryPayload};
use crate::discovery::ProcessScanner;
use crate::traits::{
    DiscoveredSession, EventSink, IngestionError, ProviderAdapter, ProviderCapabilities,
    TelemetryTier,
};
use async_trait::async_trait;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, trace};
use watchai_core::session::{
    derive_process_session_id, AdapterStatus, AgentSession, SessionLifecycleEvent, SessionRegistry,
    ToolCategory,
};
use watchai_core::state::LifecycleState;

/// Maps OpenAI Codex CLI hook/wrapper events to WatchAI LifecycleState and optional ToolCategory.
pub fn map_codex_event(
    event: &str,
    tool_name: Option<&str>,
    exit_reason: Option<&str>,
    _current_state: LifecycleState,
) -> Option<(LifecycleState, Option<ToolCategory>)> {
    match event {
        "UserPromptSubmit" | "CommandStart" | "TurnStart" | "SessionStart" => {
            Some((LifecycleState::Working, None))
        }
        "PreToolUse" | "ToolExecute" | "ToolStart" => {
            let category = tool_name.and_then(map_tool_category);
            Some((LifecycleState::Working, category))
        }
        "PostToolUse" | "ToolEnd" => Some((LifecycleState::Working, None)),
        "PermissionRequest" | "ConfirmAction" | "PromptUser" => {
            Some((LifecycleState::Waiting, None))
        }
        "ToolFailure" | "PostToolUseFailure" | "Error" => Some((LifecycleState::Error, None)),
        "Stop" | "TurnEnd" | "Complete" | "Done" => Some((LifecycleState::Success, None)),
        "StopFailure" => Some((LifecycleState::Error, None)),
        "SessionEnd" => {
            let state = match exit_reason {
                Some("error") => LifecycleState::Error,
                _ => LifecycleState::Success,
            };
            Some((state, None))
        }
        _ => None,
    }
}

/// Provider adapter for OpenAI Codex CLI agent.
pub struct CodexCliAdapter {
    event_sink: RwLock<Option<EventSink>>,
}

impl CodexCliAdapter {
    pub fn new() -> Self {
        Self {
            event_sink: RwLock::new(None),
        }
    }

    /// Processes an incoming telemetry payload for OpenAI Codex CLI.
    pub async fn handle_telemetry(
        &self,
        registry: &SessionRegistry,
        mut payload: ProviderTelemetryPayload,
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
            .find(|s| s.process_id == Some(payload.pid) && s.provider_id == "codex-cli");

        let session = match existing {
            Some(s) => s,
            None => {
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
        let (target_state, tool_category) = match map_codex_event(
            &payload.hook_event,
            payload.tool_name.as_deref(),
            payload.exit_reason.as_deref(),
            session.current_state,
        ) {
            Some(mapping) => mapping,
            None => {
                trace!(
                    "Ignored unmapped Codex event '{}' for PID {}",
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

impl Default for CodexCliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderAdapter for CodexCliAdapter {
    fn provider_id(&self) -> &'static str {
        "codex-cli"
    }

    fn display_name(&self) -> &'static str {
        "OpenAI Codex"
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
        // Check if `codex` or `codex-cli` binary is in PATH
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                if dir.join("codex").is_file() || dir.join("codex-cli").is_file() {
                    return AdapterStatus::Active;
                }
            }
        }

        // Also check default local user bin paths
        if let Ok(home) = std::env::var("HOME") {
            let home_bin = std::path::Path::new(&home).join(".local/bin");
            if home_bin.join("codex").is_file() || home_bin.join("codex-cli").is_file() {
                return AdapterStatus::Active;
            }
        }

        debug!("Codex CLI binary not found in standard paths");
        AdapterStatus::DiscoveryRequired
    }

    async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
        ProcessScanner::scan_processes_multi(
            &["codex", "codex-cli"],
            self.provider_id(),
            self.display_name(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_adapter_retains_attached_event_sink() {
        let adapter = CodexCliAdapter::new();
        assert!(adapter.event_sink.read().unwrap().is_none());

        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let sink = EventSink::new(tx, None);

        adapter.attach_event_sink(sink);
        assert!(adapter.event_sink.read().unwrap().is_some());
    }
}
