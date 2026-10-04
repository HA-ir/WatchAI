use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::state::LifecycleState;

/// High-level, sanitized activity category representing what an agent is currently doing.
/// Strictly non-invasive: never includes file paths, command names, or prompt data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolCategory {
    FileRead,
    FileWrite,
    ShellExecution,
    Search,
    ModelThinking,
}

impl ToolCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileRead => "FILE_READ",
            Self::FileWrite => "FILE_WRITE",
            Self::ShellExecution => "SHELL_EXECUTION",
            Self::Search => "SEARCH",
            Self::ModelThinking => "MODEL_THINKING",
        }
    }
}

/// Operational status of a provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AdapterStatus {
    /// Fine-grained event telemetry verified and actively emitting.
    Active,
    /// Baseline process detected, but fine-grained telemetry hooks are unverified.
    DiscoveryRequired,
    /// Provider binary or environment is not available.
    Unavailable,
}

/// Represents an individual observed AI coding agent session.
/// Enforces strict data sanitization: no prompts, code, tokens, or credentials allowed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    /// Unique, stable identifier for the lifetime of this session.
    pub session_id: String,
    /// Canonical provider slug (e.g., "claude-code", "codex-cli", "opencode").
    pub provider_id: String,
    /// Human-friendly display name (e.g., "Claude Code").
    pub provider_display_name: String,
    /// User-friendly project workspace folder name (basename of directory).
    pub project_name: String,
    /// Full path to workspace (kept in memory, stripped to basename in standard IPC).
    pub project_path: PathBuf,
    /// Current lifecycle state in the FSM.
    pub current_state: LifecycleState,
    /// Monotonically increasing sequence number to drop out-of-order events.
    pub sequence_number: u64,
    /// Timestamp when this session was initially created/discovered.
    pub started_at: DateTime<Utc>,
    /// Timestamp when the current state was entered.
    pub state_entered_at: DateTime<Utc>,
    /// Timestamp of most recent telemetry event or activity update.
    pub last_seen_at: DateTime<Utc>,
    /// Timestamp of most recent successful process liveness verification.
    #[serde(skip)]
    pub last_proc_check_at: Option<DateTime<Utc>>,
    /// OS Process ID (PID) if safely discoverable.
    pub process_id: Option<u32>,
    /// Active high-level sanitized tool category.
    pub active_tool_category: Option<ToolCategory>,
    /// Adapter discovery status.
    pub adapter_status: AdapterStatus,
    /// Process start time in clock ticks since boot (/proc/[pid]/stat field 22).
    /// Used to distinguish PID reuse from legitimate session survival across restarts.
    #[serde(skip)]
    pub process_start_time: Option<u64>,
    /// Count of consecutive failed /proc checks.
    /// Used to implement hysteresis before declaring an ungraceful crash (Phase 6).
    #[serde(skip)]
    pub consecutive_proc_failures: u32,
}

impl AgentSession {
    /// Create a new session with initial state.
    pub fn new(
        session_id: String,
        provider_id: impl Into<String>,
        provider_display_name: impl Into<String>,
        project_path: impl AsRef<Path>,
        process_id: Option<u32>,
        initial_state: LifecycleState,
        adapter_status: AdapterStatus,
    ) -> Self {
        let now = Utc::now();
        let path_buf = project_path.as_ref().to_path_buf();
        let project_name = path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace")
            .to_string();

        Self {
            session_id,
            provider_id: provider_id.into(),
            provider_display_name: provider_display_name.into(),
            project_name,
            project_path: path_buf,
            current_state: initial_state,
            sequence_number: 0,
            started_at: now,
            state_entered_at: now,
            last_seen_at: now,
            last_proc_check_at: None,
            process_id,
            active_tool_category: None,
            adapter_status,
            process_start_time: None,
            consecutive_proc_failures: 0,
        }
    }

    /// Builder method to attach process start time ticks.
    pub fn with_process_start_time(mut self, start_time: Option<u64>) -> Self {
        self.process_start_time = start_time;
        self
    }

    /// Attempt to transition to a new lifecycle state at current UTC time.
    pub fn transition_to(
        &mut self,
        target_state: LifecycleState,
        sequence_number: u64,
        tool_category: Option<ToolCategory>,
    ) -> Result<(), String> {
        self.transition_to_at(target_state, sequence_number, tool_category, Utc::now())
    }

    /// Attempt to transition to a new lifecycle state at a specified timestamp.
    /// Returns Ok(()) if the transition was valid and applied, or Err with description if rejected.
    pub fn transition_to_at(
        &mut self,
        target_state: LifecycleState,
        sequence_number: u64,
        tool_category: Option<ToolCategory>,
        at: DateTime<Utc>,
    ) -> Result<(), String> {
        // Discard out-of-order or duplicate events based on sequence number
        if sequence_number <= self.sequence_number {
            return Err(format!(
                "Dropped out-of-order event: received seq {} <= current seq {}",
                sequence_number, self.sequence_number
            ));
        }

        if !self.current_state.can_transition_to(target_state) {
            return Err(format!(
                "Invalid state transition: cannot transition from {} to {}",
                self.current_state, target_state
            ));
        }

        if self.current_state != target_state {
            self.current_state = target_state;
            self.state_entered_at = at;
        }

        self.sequence_number = sequence_number;
        self.last_seen_at = at;
        self.active_tool_category = tool_category;
        Ok(())
    }

    /// Update telemetry activity heartbeat without changing state.
    pub fn touch(&mut self) {
        self.last_seen_at = Utc::now();
    }

    /// Update process liveness verification timestamp.
    /// Crucial invariant: Does NOT update `last_seen_at`, preserving the telemetry silence timer.
    pub fn touch_proc_liveness(&mut self) {
        self.last_proc_check_at = Some(Utc::now());
    }

    /// Apply an incoming telemetry event to the session.
    /// Recovers sessions from UNKNOWN to WORKING or other valid states and updates timestamps.
    pub fn apply_telemetry(
        &mut self,
        target_state: LifecycleState,
        sequence_number: u64,
        tool_category: Option<ToolCategory>,
    ) -> Result<(), String> {
        self.transition_to(target_state, sequence_number, tool_category)
    }

    /// Apply an incoming SessionLifecycleEvent to the session.
    /// Validates FSM transition legality, verifies timestamp freshness,
    /// increments the internal monotonic sequence number, and updates activity timestamps.
    pub fn apply_lifecycle_event(&mut self, event: &SessionLifecycleEvent) -> Result<(), String> {
        match event {
            SessionLifecycleEvent::StateTransition {
                new_state,
                tool_category,
                timestamp,
                ..
            } => {
                if *timestamp < self.state_entered_at {
                    return Err(format!(
                        "Dropped stale event: event timestamp {} is before state_entered_at {}",
                        timestamp, self.state_entered_at
                    ));
                }
                let next_seq = self.sequence_number + 1;
                self.transition_to_at(*new_state, next_seq, *tool_category, *timestamp)
            }
            SessionLifecycleEvent::Heartbeat { timestamp, .. } => {
                if *timestamp >= self.last_seen_at {
                    self.last_seen_at = *timestamp;
                }
                Ok(())
            }
            SessionLifecycleEvent::SessionTerminated {
                exit_code,
                timestamp,
                ..
            } => {
                if *timestamp < self.state_entered_at {
                    return Err(format!(
                        "Dropped stale termination: timestamp {} is before state_entered_at {}",
                        timestamp, self.state_entered_at
                    ));
                }
                let target = match exit_code {
                    Some(0) => LifecycleState::Success,
                    Some(_) => LifecycleState::Error,
                    None => LifecycleState::Cancelled,
                };
                let next_seq = self.sequence_number + 1;
                self.transition_to_at(target, next_seq, None, *timestamp)
            }
        }
    }
}

/// Standardized, provider-agnostic domain event streamed from adapters to the daemon ingestion engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionLifecycleEvent {
    /// Explicit request to transition session lifecycle state.
    StateTransition {
        session_id: String,
        new_state: LifecycleState,
        tool_category: Option<ToolCategory>,
        timestamp: DateTime<Utc>,
    },
    /// Periodic heartbeat confirming agent process activity.
    Heartbeat {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    /// Notification that the agent session has terminated.
    SessionTerminated {
        session_id: String,
        exit_code: Option<i32>,
        timestamp: DateTime<Utc>,
    },
}

impl SessionLifecycleEvent {
    /// Whether this event is a critical state transition that MUST NOT be dropped.
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            Self::StateTransition { .. } | Self::SessionTerminated { .. }
        )
    }

    /// Associated session identifier.
    pub fn session_id(&self) -> &str {
        match self {
            Self::StateTransition { session_id, .. } => session_id,
            Self::Heartbeat { session_id, .. } => session_id,
            Self::SessionTerminated { session_id, .. } => session_id,
        }
    }

    /// Event creation timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::StateTransition { timestamp, .. } => *timestamp,
            Self::Heartbeat { timestamp, .. } => *timestamp,
            Self::SessionTerminated { timestamp, .. } => *timestamp,
        }
    }
}

/// Derive a deterministic, stable session identifier for unmanaged processes discovered via `/proc`.
pub fn derive_process_session_id(
    pid: u32,
    start_time: u64,
    project_path: impl AsRef<Path>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(pid.to_le_bytes());
    hasher.update(start_time.to_le_bytes());
    hasher.update(project_path.as_ref().to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    format!("proc-{}", &hex::encode(hash)[0..16])
}

/// Thread-safe volatile in-memory session registry.
#[derive(Debug, Default, Clone)]
pub struct SessionRegistry {
    sessions: Arc<RwLock<HashMap<String, AgentSession>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert or update a session in the volatile registry.
    pub async fn upsert(&self, session: AgentSession) {
        let mut map = self.sessions.write().await;
        map.insert(session.session_id.clone(), session);
    }

    /// Retrieve a cloned session record by its unique ID.
    pub async fn get(&self, session_id: &str) -> Option<AgentSession> {
        let map = self.sessions.read().await;
        map.get(session_id).cloned()
    }

    /// Retrieve all tracked sessions as a cloned vector.
    pub async fn list(&self) -> Vec<AgentSession> {
        let map = self.sessions.read().await;
        map.values().cloned().collect()
    }

    /// Remove a session from memory (e.g. after retention window expiration).
    pub async fn remove(&self, session_id: &str) -> Option<AgentSession> {
        let mut map = self.sessions.write().await;
        map.remove(session_id)
    }

    /// Return count of sessions in registry.
    pub async fn count(&self) -> usize {
        let map = self.sessions.read().await;
        map.len()
    }
}
