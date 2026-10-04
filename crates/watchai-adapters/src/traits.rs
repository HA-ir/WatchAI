pub use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use watchai_core::session::{AdapterStatus, SessionLifecycleEvent};
use watchai_core::state::LifecycleState;

/// Defines the architectural mechanism through which an adapter observes agent lifecycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TelemetryTier {
    /// Non-invasive /proc inspection. Can only detect process presence.
    /// Sessions initialize strictly in IDLE with AdapterStatus::DiscoveryRequired.
    ProcessDiscoveryOnly,
    /// Passive filesystem log or FIFO tailing (reserved for future adapters).
    PassiveLogTailing,
    /// Opt-in hook receiver or local IPC socket (reserved for future adapters).
    OptInHookTelemetry,
}

/// Describes the observational capability boundaries of an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Telemetry mechanism tier.
    pub telemetry_tier: TelemetryTier,
    /// Whether the adapter natively supports tool category extraction.
    pub supports_tool_categories: bool,
    /// Whether the adapter natively streams live activity events (WORKING / WAITING).
    pub supports_activity_events: bool,
}

impl ProviderCapabilities {
    /// Default capability descriptor for process-discovery-only adapters.
    pub fn process_discovery_only() -> Self {
        Self {
            telemetry_tier: TelemetryTier::ProcessDiscoveryOnly,
            supports_tool_categories: false,
            supports_activity_events: false,
        }
    }
}

/// Result of an attempt to ingest a lifecycle event into the daemon channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestionError {
    /// Heartbeat was dropped or coalesced under channel saturation.
    HeartbeatShed,
    /// Channel was closed.
    ChannelClosed,
    /// Ingestion was aborted due to daemon shutdown.
    Shutdown,
}

/// An asynchronous event sink handle connecting adapters to the daemon ingestion engine.
#[derive(Clone, Debug)]
pub struct EventSink {
    tx: tokio::sync::mpsc::Sender<SessionLifecycleEvent>,
    shutdown_rx: Option<tokio::sync::watch::Receiver<bool>>,
}

impl EventSink {
    /// Construct a new EventSink wrapping a bounded MPSC sender and shutdown watch receiver.
    pub fn new(
        tx: tokio::sync::mpsc::Sender<SessionLifecycleEvent>,
        shutdown_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Self {
        Self { tx, shutdown_rx }
    }

    /// Ingest a SessionLifecycleEvent with prioritized backpressure.
    /// Critical events (StateTransition, SessionTerminated) await channel reservation
    /// or cooperative cancellation on shutdown.
    /// Heartbeats drop/coalesce when channel utilization exceeds 80% capacity to preserve buffer headroom.
    pub async fn ingest(&self, event: SessionLifecycleEvent) -> Result<(), IngestionError> {
        if event.is_critical() {
            if let Some(mut shutdown) = self.shutdown_rx.clone() {
                tokio::select! {
                    res = self.tx.send(event) => {
                        res.map_err(|_| IngestionError::ChannelClosed)
                    }
                    _ = shutdown.changed() => {
                        Err(IngestionError::Shutdown)
                    }
                }
            } else {
                self.tx
                    .send(event)
                    .await
                    .map_err(|_| IngestionError::ChannelClosed)
            }
        } else {
            let cap = self.tx.capacity();
            if cap < 50 {
                return Err(IngestionError::HeartbeatShed);
            }
            match self.tx.try_send(event) {
                Ok(()) => Ok(()),
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    Err(IngestionError::HeartbeatShed)
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    Err(IngestionError::ChannelClosed)
                }
            }
        }
    }

    /// Return remaining channel capacity.
    pub fn capacity(&self) -> usize {
        self.tx.capacity()
    }
}

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

    /// Return the observational capability descriptor for this provider.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::process_discovery_only()
    }

    /// Attach an EventSink handle to the adapter.
    /// Default no-op for ProcessDiscoveryOnly adapters.
    fn attach_event_sink(&self, _sink: EventSink) {}

    /// Probe the local environment for executable presence, config files, and hook readiness.
    async fn check_environment(&self) -> AdapterStatus;

    /// Scan local system processes or workspaces to discover active unmanaged sessions.
    async fn discover_sessions(&self) -> Vec<DiscoveredSession>;
}
