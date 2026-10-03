# Contract: Provider Adapter Interface

**Feature**: Baseline Agent Monitoring & GNOME Shell Indicator (`001-agent-status-indicator`)  
**Layer**: Agent Integration Layer (Layer 1)  
**Status**: Draft  

---

## 1. Architectural Purpose
Provider adapters insulate the core WatchAI domain model from the diverse, volatile, and vendor-specific mechanisms used by different AI coding tools (Claude Code, OpenAI Codex CLI, OpenCode).

The adapter contract enforces:
1. Strict translation from provider-specific events into normalized `SessionLifecycleEvent` records.
2. Zero leakage of prompts, code diffs, tokens, or command outputs.
3. Explicit discovery state reporting (`ACTIVE`, `DISCOVERY_REQUIRED`, `UNAVAILABLE`).

---

## 2. Core Adapter Trait / Interface (Rust Pseudocode)

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Unique provider identifier (e.g., "claude-code", "codex-cli", "opencode")
    fn provider_id(&self) -> &'static str;

    /// Human-readable display name
    fn display_name(&self) -> &'static str;

    /// Check system environment for binary availability, configuration files, and hook status
    async fn check_environment(&self) -> AdapterStatus;

    /// Scan system processes and workspace locks to discover active unmanaged sessions
    async fn discover_active_sessions(&self) -> Vec<DiscoveredSession>;

    /// Subscribe to real-time events emitted by the provider (via hooks, sockets, or process monitors)
    async fn subscribe_events(&self) -> Result<BoxStream<'static, SessionLifecycleEvent>, AdapterError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterStatus {
    /// Full event telemetry verified and active
    Active,
    /// Baseline process detected, but fine-grained telemetry hooks are unverified
    DiscoveryRequired,
    /// Agent binary not installed or not detectable
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSession {
    pub session_id: String,
    pub project_path: PathBuf,
    pub process_id: Option<u32>,
    pub initial_state: LifecycleState,
    pub started_at: DateTime<Utc>,
}
```

---

## 3. Tiered Implementation Strategy per Provider

### 3.1 Claude Code Adapter (`claude-code`)
- **Discovery**: Locate `claude` executable in `$PATH` or active processes via `/proc`. Inspect working directory via `/proc/[pid]/cwd`.
- **Telemetry Mechanism**:
  - *Primary (Opt-in)*: A lightweight WatchAI hook registered in `.claude/settings.json` that emits telemetry signals directly to a local socket/pipe on `SessionStart`, `UserPromptSubmit`, `ToolUse`, `Notification`, and `Stop`.
  - *Secondary (Fallback)*: OS process liveness monitoring. If hooks are not present, session is marked `DISCOVERY_REQUIRED`, mapping running process to `WORKING` and idle prompt to `IDLE`.

### 3.2 OpenAI Codex CLI Adapter (`codex-cli`)
- **Discovery**: Locate `codex` / `codex-cli` binary.
- **Telemetry Mechanism**:
  - *Initial Validation Step*: Verify if Codex CLI provides hooks or wrapper scripts.
  - *Status*: Initial release marks status as `DISCOVERY_REQUIRED`, relying on process/workspace monitoring.

### 3.3 OpenCode Adapter (`opencode`)
- **Discovery**: Locate `opencode` binary.
- **Telemetry Mechanism**:
  - *Initial Validation Step*: Audit OpenCode extension/plugin architecture.
  - *Status*: Initial release marks status as `DISCOVERY_REQUIRED`.

---

## 4. Normalization Invariants

Adapters must strictly adhere to the following normalization rules:
- Any incoming raw event payload MUST be sanitized: arguments, file paths outside project root, prompt text, and output buffers MUST be stripped before creating a `SessionLifecycleEvent`.
- High-level tool tags must be mapped to the standardized `ToolCategory` enum (`FileRead`, `FileWrite`, `ShellExecution`, `Search`, `ModelThinking`).
- If an adapter receives an unrecognized event type or corrupted payload, it MUST NOT crash; it must emit a transition to `UNKNOWN` and log a diagnostic warning.
