# Data Model: Phase 8 — Multi-Provider Discovery & Capability Modeling

**Feature**: Multi-Provider Discovery & Capability Modeling (`004-provider-adapters`)  
**Date**: 2026-10-04  
**Status**: Completed  

---

## 1. Domain Entities & Type Definitions

### 1.1 `TelemetryTier`
Defines the depth and architectural mechanism through which an adapter observes agent lifecycles.

```rust
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
```

### 1.2 `ProviderCapabilities`
Describes the runtime observational capabilities of an adapter.

```rust
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
```

### 1.3 `SessionLifecycleEvent`
Standardized, provider-agnostic domain event streamed from adapters to the daemon ingestion engine.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
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
        matches!(self, Self::StateTransition { .. } | Self::SessionTerminated { .. })
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
```

---

## 2. Updated Trait Contracts

### 2.1 `ProviderAdapter` Trait
Extended with `capabilities(&self)` while maintaining complete provider agnosticism.

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Canonical provider slug (e.g. "claude-code", "codex-cli", "opencode").
    fn provider_id(&self) -> &'static str;

    /// Human-friendly display name (e.g. "Claude Code", "OpenAI Codex", "OpenCode").
    fn display_name(&self) -> &'static str;

    /// Return the observational capabilities of this adapter.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Probe local environment for executable presence in PATH on startup.
    async fn check_environment(&self) -> AdapterStatus;

    /// Scan local system processes to discover active agent sessions.
    async fn discover_sessions(&self) -> Vec<DiscoveredSession>;
}
```

---

## 3. Provider Identity Matrix (Phase 8 Baseline)

| Provider | Provider ID (`provider_id`) | Display Name (`display_name`) | Binary Matches | Telemetry Tier | Tool Categories | Activity Events |
|:---|:---|:---|:---|:---|:---:|:---:|
| **Claude Code** | `claude-code` | `Claude Code` | `claude` | `ProcessDiscoveryOnly` | No | No |
| **OpenAI Codex CLI** | `codex-cli` | `OpenAI Codex` | `codex`, `codex-cli`, `node .../codex` | `ProcessDiscoveryOnly` | No | No |
| **OpenCode** | `opencode` | `OpenCode` | `opencode`, `python .../opencode` | `ProcessDiscoveryOnly` | No | No |

---

## 4. Ingestion Channel Contract

```text
Adapters (Producers)
  ├── ClaudeCodeAdapter
  ├── CodexCliAdapter       ────► mpsc::Sender<SessionLifecycleEvent> (capacity: 256)
  └── OpenCodeAdapter
                                             │
                                             ▼
                                Daemon Ingestion Consumer Task
                                             │
                                ┌────────────┴────────────┐
                                │ Validate FSM Transition │
                                │ Update SessionRegistry  │
                                │ Emit D-Bus Signals      │
                                └─────────────────────────┘
```

- **Buffer Size**: 256 items.
- **Priority Guard**:
  - `is_critical() == true`: `sender.send(event).await` (blocks adapter on saturation, zero dropped transitions).
  - `is_critical() == false` (`Heartbeat`): `sender.try_send(event)` (drops or coalesces when buffer $> 80\%$ full, guaranteeing headroom for critical transitions).
