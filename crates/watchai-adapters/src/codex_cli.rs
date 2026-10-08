use crate::claude_code::{map_tool_category, ProviderTelemetryPayload};
use crate::discovery::ProcessScanner;
use crate::traits::{
    DiscoveredSession, EventSink, IngestionError, ProviderAdapter, ProviderCapabilities,
    TelemetryTier,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tracing::{debug, trace};
use watchai_core::session::{
    derive_process_session_id, AdapterStatus, AgentSession, SessionLifecycleEvent, SessionRegistry,
    ToolCategory,
};
use watchai_core::state::LifecycleState;

/// Inspects ~/.codex/sessions for the latest rollout log matching a project directory
/// and extracts the authoritative turn state (Working, Success, Error, or Idle).
pub fn read_codex_rollout_state(
    project_path: &Path,
    now: DateTime<Utc>,
) -> Option<(LifecycleState, Option<ToolCategory>)> {
    let home = std::env::var("HOME").ok()?;
    let sessions_dir = PathBuf::from(home).join(".codex").join("sessions");
    if !sessions_dir.is_dir() {
        return None;
    }

    let mut files = Vec::new();
    let mut dirs_to_visit = vec![sessions_dir];
    while let Some(dir) = dirs_to_visit.pop() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs_to_visit.push(path);
                } else if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                            if let Ok(meta) = entry.metadata() {
                                if let Ok(mtime) = meta.modified() {
                                    files.push((path, mtime));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    files.sort_by(|a, b| b.1.cmp(&a.1));

    let canonical_proj =
        std::fs::canonicalize(project_path).unwrap_or_else(|_| project_path.to_path_buf());

    for (fpath, _) in files.iter().take(5) {
        if let Ok(file) = std::fs::File::open(fpath) {
            let mut reader = BufReader::new(file);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).is_ok() {
                if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&first_line) {
                    let file_cwd = meta_json
                        .get("payload")
                        .and_then(|p| p.get("cwd"))
                        .and_then(|c| c.as_str());

                    if let Some(fc) = file_cwd {
                        let fc_canon =
                            std::fs::canonicalize(fc).unwrap_or_else(|_| PathBuf::from(fc));
                        if fc_canon == canonical_proj {
                            let mut last_type = None;
                            let mut last_timestamp = None;

                            let mut lines = Vec::new();
                            let mut line = String::new();
                            while reader.read_line(&mut line).is_ok() && !line.is_empty() {
                                lines.push(line.clone());
                                line.clear();
                            }

                            for l in lines.iter().rev() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                                    if let Some(ptype) = v
                                        .get("payload")
                                        .and_then(|p| p.get("type"))
                                        .and_then(|t| t.as_str())
                                    {
                                        if ptype == "task_started"
                                            || ptype == "task_complete"
                                            || ptype == "task_failed"
                                        {
                                            last_type = Some(ptype.to_string());
                                            if let Some(ts_str) =
                                                v.get("timestamp").and_then(|t| t.as_str())
                                            {
                                                if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str)
                                                {
                                                    last_timestamp = Some(dt.with_timezone(&Utc));
                                                }
                                            }
                                            break;
                                        }
                                    }
                                }
                            }

                            if let Some(event_type) = last_type {
                                match event_type.as_str() {
                                    "task_started" => {
                                        return Some((LifecycleState::Working, None));
                                    }
                                    "task_complete" => {
                                        if let Some(ts) = last_timestamp {
                                            if (now - ts).num_seconds()
                                                <= watchai_core::aggregate::COMPLETION_DWELL_SECONDS
                                            {
                                                return Some((LifecycleState::Success, None));
                                            }
                                        }
                                        return Some((LifecycleState::Idle, None));
                                    }
                                    "task_failed" => {
                                        if let Some(ts) = last_timestamp {
                                            if (now - ts).num_seconds()
                                                <= watchai_core::aggregate::COMPLETION_DWELL_SECONDS
                                            {
                                                return Some((LifecycleState::Error, None));
                                            }
                                        }
                                        return Some((LifecycleState::Idle, None));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Maps OpenAI Codex CLI hook/wrapper events to WatchAI LifecycleState and optional ToolCategory.
pub fn map_codex_event(
    event: &str,
    tool_name: Option<&str>,
    exit_reason: Option<&str>,
    _current_state: LifecycleState,
) -> Option<(LifecycleState, Option<ToolCategory>)> {
    match event {
        "UserPromptSubmit" | "CommandStart" | "TurnStart" => Some((LifecycleState::Working, None)),
        "SessionStart" => Some((LifecycleState::Idle, None)),
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
        let mut sessions = ProcessScanner::scan_processes_multi(
            &["codex", "codex-cli"],
            self.provider_id(),
            self.display_name(),
        );

        let now = Utc::now();
        for s in &mut sessions {
            if let Some((state, tool)) = read_codex_rollout_state(&s.project_path, now) {
                s.initial_state = state;
                s.adapter_status = AdapterStatus::Active;

                let sink = self.event_sink.read().unwrap().clone();
                if let Some(sink) = sink {
                    let event = SessionLifecycleEvent::StateTransition {
                        session_id: s.session_id.clone(),
                        new_state: state,
                        tool_category: tool,
                        timestamp: now,
                    };
                    let _ = sink.ingest(event).await;
                }
            }
        }

        sessions
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

    #[test]
    fn test_map_codex_session_start_is_idle() {
        let (state, _) =
            map_codex_event("SessionStart", None, None, LifecycleState::Unknown).unwrap();
        assert_eq!(state, LifecycleState::Idle);
    }
}
