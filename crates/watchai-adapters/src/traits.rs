use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use watchai_core::session::AdapterStatus;
use watchai_core::state::LifecycleState;

/// Representation of an agent session discovered from process/filesystem scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub provider_id: String,
    pub provider_display_name: String,
    pub project_path: PathBuf,
    pub process_id: Option<u32>,
    pub initial_state: LifecycleState,
    pub started_at: DateTime<Utc>,
    pub adapter_status: AdapterStatus,
    pub process_start_time: Option<u64>,
}

impl DiscoveredSession {
    /// Deterministic total order comparator for recovery discovery:
    /// 1. provider_id ASC
    /// 2. project_path ASC
    /// 3. process_id ASC
    pub fn deterministic_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.provider_id
            .cmp(&other.provider_id)
            .then_with(|| self.project_path.cmp(&other.project_path))
            .then_with(|| self.process_id.cmp(&other.process_id))
    }
}

/// The common trait implemented by all provider-specific adapters.
/// Enforces complete decoupling: adapters translate vendor realities into common domain models.
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Return the canonical provider slug (e.g. "claude-code", "codex-cli", "opencode").
    fn provider_id(&self) -> &'static str;

    /// Return the human-readable display name (e.g. "Claude Code").
    fn display_name(&self) -> &'static str;

    /// Probe the local environment for executable presence, config files, and hook readiness.
    async fn check_environment(&self) -> AdapterStatus;

    /// Scan local system processes or workspaces to discover active unmanaged sessions.
    async fn discover_sessions(&self) -> Vec<DiscoveredSession>;
}
