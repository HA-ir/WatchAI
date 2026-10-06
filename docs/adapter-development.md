# Third-Party AI Provider Adapter Integration Guide

This guide details the architectural contracts, implementation patterns, and privacy requirements for developing new AI coding agent adapters in WatchAI.

---

## 1. Architectural Overview

WatchAI is architected around a strict layered separation of concerns:

```
┌────────────────────────────────────────────────────────┐
│             Third-Party AI Agent Process               │
│          (Claude Code, Codex CLI, OpenCode, ...)       │
└───────────────────────────┬────────────────────────────┘
                            │ /proc inspection / logs
                            ▼
┌────────────────────────────────────────────────────────┐
│             Provider Adapters Layer                    │
│      (crates/watchai-adapters: ProviderAdapter)        │
└───────────────────────────┬────────────────────────────┘
                            │ SessionLifecycleEvent
                            ▼
┌────────────────────────────────────────────────────────┐
│             Daemon Core & State Machine                │
│    (crates/watchai-core: FSM, Registry, Liveness)      │
└───────────────────────────┬────────────────────────────┘
                            │ D-Bus (org.freedesktop.WatchAI)
                            ▼
┌────────────────────────────────────────────────────────┐
│             GNOME Shell Extension UI                   │
│        (extension/: Indicator, Popover, Settings)      │
└────────────────────────────────────────────────────────┘
```

Adapters are responsible **only** for observing external agent processes or telemetry and converting vendor-specific realities into the canonical domain models defined in `watchai-core`. Adapters do not manage UI, persist D-Bus objects, or dictate global state aggregation.

---

## 2. The `ProviderAdapter` Trait

All adapters implement the asynchronous `ProviderAdapter` trait defined in `crates/watchai-adapters/src/traits.rs`:

```rust
use async_trait::async_trait;
use crate::traits::{DiscoveredSession, EventSink, ProviderCapabilities};
use watchai_core::session::AdapterStatus;

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Return the canonical provider slug (e.g. "claude-code", "codex-cli", "opencode").
    fn provider_id(&self) -> &'static str;

    /// Return the human-readable display name (e.g. "Claude Code", "OpenAI Codex").
    fn display_name(&self) -> &'static str;

    /// Return the observational capability descriptor for this provider.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::process_discovery_only()
    }

    /// Attach an EventSink handle to the adapter for streaming live lifecycle events.
    fn attach_event_sink(&self, _sink: EventSink) {}

    /// Probe the local environment for executable presence, config files, or readiness.
    async fn check_environment(&self) -> AdapterStatus;

    /// Scan local system processes or workspaces to discover active sessions.
    async fn discover_sessions(&self) -> Vec<DiscoveredSession>;
}
```

### Telemetry Tiers & Capabilities

The `capabilities()` method returns a `ProviderCapabilities` struct declaring the observation mechanism:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub telemetry_tier: TelemetryTier,
    pub supports_tool_categories: bool,
    pub supports_activity_events: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TelemetryTier {
    /// Non-invasive /proc inspection. Can only detect process presence.
    /// Sessions initialize strictly in IDLE with AdapterStatus::DiscoveryRequired.
    ProcessDiscoveryOnly,
    /// Passive filesystem log or FIFO tailing (for adapters observing local logs).
    PassiveLogTailing,
    /// Opt-in hook receiver or local IPC socket.
    OptInHookTelemetry,
}
```

If your adapter only discovers processes via `/proc`, use `ProviderCapabilities::process_discovery_only()`.

### Adapter Status

The `check_environment()` probe returns one of three operational states:

```rust
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
```

---

## 3. Process Discovery & Heuristics

Process-discovery adapters leverage `ProcessScanner` (`crates/watchai-adapters/src/discovery.rs`) to scan `/proc` without elevated privileges.

### How `ProcessScanner` Inspects `/proc`

1. **Tokenizing Command Lines**:
   The scanner reads null-delimited bytes from `/proc/[pid]/cmdline` via `parse_cmdline_args`:
   ```rust
   pub fn parse_cmdline_args(cmdline_bytes: &[u8]) -> Vec<String>
   ```
2. **Determining Working Directory**:
   The session's project directory is resolved from the `/proc/[pid]/cwd` symlink. If the symlink is unreadable, it defaults to `/unknown/workspace`.
3. **Retrieving Process Start Time**:
   The process start time (in clock ticks since boot) is read from field 22 of `/proc/[pid]/stat`. This token is critical for PID reuse prevention.

### Mandatory False-Positive Rejection Rules

To avoid false detections, `ProcessScanner::matches_cmdline_args` enforces strict rejection filters:

1. **Rejected Executables**:
   If `argv[0]` is an inspection tool, shell, or build system, the process is immediately rejected:
   ```rust
   const REJECTED_EXECUTABLES: &[&str] = &[
       "grep", "rg", "sh", "bash", "zsh", "git", "cargo", "ps", "which",
       "whereis", "find", "sed", "awk", "xargs", "make", "ninja", "strace",
   ];
   ```
   *(e.g., `grep claude` or `git commit -m "claude"` will never be detected as a Claude session).*

2. **Direct Executable Matching**:
   Matches if `exe_name == target` or `exe_name == format!("{}-cli", target)`.

3. **Script Runner Matching**:
   When agents are launched via language interpreters (`node`, `bun`, `python`, `python3`, `deno`, `ts-node`), the scanner inspects `argv[1]` to match script file names:
   ```rust
   const SCRIPT_RUNNERS: &[&str] = &["node", "bun", "python", "python3", "deno", "ts-node"];
   ```
   Matches are accepted if `argv[1]` equals the target binary, equals `{target}-cli`, or begins with `{target}.` / `{target}-cli.` (e.g., `python3 /usr/local/bin/codex.py`).

### Deterministic Session ID Derivation

Every discovered session must generate a stable, deterministic session ID via:

```rust
use watchai_core::session::derive_process_session_id;

let session_id = derive_process_session_id(pid, start_time, &project_path);
```

This hashes `(pid, start_time, project_path)` using SHA-256 and formats it as `"proc-"` followed by the first 16 hex characters of the digest (`proc-<hex16>`). This guarantees:
- Re-scans of the same running process yield the exact same session ID.
- If the operating system recycles the PID after process termination, the new process has a different `start_time`, resulting in a distinct session ID.

### The `DiscoveredSession` Structure

```rust
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
```

---

## 4. Canonical Lifecycle State Machine

WatchAI models all agent activity through the canonical 8-state Finite State Machine defined in `crates/watchai-core/src/state.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LifecycleState {
    /// No active task processing; session is at an interactive prompt or unmonitored.
    Idle,
    /// Process initialized, bootstrapping, or loading context.
    Starting,
    /// Actively processing, executing tools, or generating code.
    Working,
    /// Blocked mid-task awaiting user permission, input, or tool approval.
    Waiting,
    /// Task execution completed successfully (dwells briefly before settling to Idle).
    Success,
    /// Session stopped or failed due to an error.
    Error,
    /// Session or active task was cancelled/interrupted by user.
    Cancelled,
    /// State cannot be reliably determined (telemetry dropped or unverified).
    Unknown,
}
```

### State Semantics

| State | Wire Name | Semantic Meaning |
| :--- | :---: | :--- |
| `LifecycleState::Idle` | `"IDLE"` | Session is idle at an interactive prompt or passive. |
| `LifecycleState::Starting` | `"STARTING"` | Initializing, bootstrapping, or loading workspace context. |
| `LifecycleState::Working` | `"WORKING"` | Actively processing, executing tools, or generating responses. |
| `LifecycleState::Waiting` | `"WAITING"` | Blocked mid-task waiting for human input, confirmation, or permission. |
| `LifecycleState::Success` | `"SUCCESS"` | Task finished successfully (subject to 10s dwell before settling to Idle). |
| `LifecycleState::Error` | `"ERROR"` | Session crashed, failed, or encountered an unhandled error. |
| `LifecycleState::Cancelled` | `"CANCELLED"` | Active task was cancelled or interrupted by the user (`Ctrl+C`). |
| `LifecycleState::Unknown` | `"UNKNOWN"` | Liveness unverified or telemetry silence timeout expired. |

### Valid FSM Transitions (`can_transition_to`)

The state machine strictly validates all transitions in `LifecycleState::can_transition_to`:

- **`Idle`** $\rightarrow$ `Starting`, `Working`, `Unknown`, `Error`
- **`Starting`** $\rightarrow$ `Working`, `Idle`, `Waiting`, `Error`, `Cancelled`, `Unknown`
- **`Working`** $\rightarrow$ `Waiting`, `Success`, `Error`, `Cancelled`, `Unknown`
- **`Waiting`** $\rightarrow$ `Working`, `Cancelled`, `Error`, `Unknown`
- **`Success`** $\rightarrow$ `Working`, `Idle`, `Cancelled`, `Starting`
- **`Error`** $\rightarrow$ `Starting`, `Working`, `Idle`
- **`Cancelled`** $\rightarrow$ `Starting`, `Working`, `Idle`
- **`Unknown`** $\rightarrow$ `Starting`, `Working`, `Waiting`, `Idle`, `Error`, `Cancelled`
- **Self-Transitions**: Updating telemetry in the same state (heartbeats) is always permitted.

### Global Aggregate Priority Ranking

When multiple agent sessions run concurrently, WatchAI resolves the top-bar indicator's aggregate state using numerical priority scores defined in `LifecycleState::priority_score()`:

$$\text{ERROR (80)} > \text{WAITING (70)} > \text{WORKING (60)} > \text{STARTING (50)} > \text{CANCELLED (40)} > \text{SUCCESS (30)} > \text{UNKNOWN (20)} > \text{IDLE (10)}$$

This ensures that critical blocking states (`ERROR`, `WAITING`) always surface in the GNOME top bar over background work (`WORKING`, `IDLE`).

### Completion Dwell & Terminal Retention

- **10-Second Completion Dwell (`COMPLETION_DWELL_SECONDS = 10`)**: When a session finishes in `SUCCESS` or `CANCELLED`, the aggregate state retains this completion status for 10 seconds before settling back to `IDLE` (if no other sessions are active).
- **Terminal Retention Window (`RETENTION_WINDOW_SECONDS = 60`)**: Terminal sessions (`Success`, `Error`, `Cancelled`) remain visible in memory for 60 seconds before being pruned by `SessionRegistry::purge_stale_sessions`.

---

## 5. Streaming Events & Tool Categorization

Adapters that capture live events stream them via `EventSink::ingest`:

```rust
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
    /// Direct registration of a new session.
    SessionRegistered {
        session_id: String,
        provider_id: String,
        provider_display_name: String,
        project_path: String,
        process_id: Option<u32>,
        initial_state: LifecycleState,
        timestamp: DateTime<Utc>,
    },
}
```

### Tool Categorization & Sanitization

When an agent is in `LifecycleState::Working`, adapters may supply an optional `ToolCategory` describing the broad nature of the activity:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolCategory {
    FileRead,
    FileWrite,
    ShellExecution,
    Search,
    ModelThinking,
}
```

#### Mapping Matrix

| Provider Tool | Canonical Category | Serialized D-Bus String |
| :--- | :--- | :--- |
| `read`, `view`, `cat` | `ToolCategory::FileRead` | `"FILE_READ"` |
| `write`, `edit`, `patch` | `ToolCategory::FileWrite` | `"FILE_WRITE"` |
| `bash`, `sh`, `exec` | `ToolCategory::ShellExecution` | `"SHELL_EXECUTION"` |
| `grep`, `glob`, `find` | `ToolCategory::Search` | `"SEARCH"` |
| `thinking`, `reasoning` | `ToolCategory::ModelThinking` | `"MODEL_THINKING"` |

**Security Boundary**: Never expose raw shell commands, file contents, or tool arguments in the tool category string. The D-Bus wire protocol expects only the sanitized category name.

---

## 6. Liveness Monitoring & Hysteresis

The daemon polls running sessions periodically (default interval: `LIVENESS_CHECK_INTERVAL_SECONDS = 2`). To handle transient scheduling lags or procfs delays, WatchAI uses a **2-cycle failure hysteresis**:

1. **Cycle 1 Missing**: If a session's PID cannot be verified in `/proc` on poll $N$, the session records a failure count of 1, but its status is **not** immediately terminated (`LivenessCheckResult::TransientFailure`).
2. **Cycle 2 Missing**: If the PID remains unverified on poll $N+1$, the session is officially transitioned to `LifecycleState::Error` with `DeadReason::ProcessTerminated`.
3. **PID Validation**: The reader verifies that the process start time recorded at discovery matches the current process start time in field 22 of `/proc/[pid]/stat`. If the PID was recycled by the OS for a different process, the session is transitioned immediately with `DeadReason::PidReused`.
4. **Silence Timeouts**:
   - Sessions in `WORKING` without telemetry updates for 300 seconds (`SILENCE_TIMEOUT_WORKING_SECONDS`) transition to `LifecycleState::Unknown`.
   - Sessions in `STARTING` without telemetry updates for 60 seconds (`SILENCE_TIMEOUT_STARTING_SECONDS`) transition to `LifecycleState::Unknown`.

---

## 7. Zero-Leakage Privacy Invariant

WatchAI enforces a strict **Zero-Leakage Privacy Policy**:

- **Prohibited Data**:
  - User prompts or instructions.
  - LLM response text or conversation histories.
  - File paths outside the project root directory.
  - Source code lines or git diffs.
  - Command-line arguments or parameters that may contain API keys, authorization tokens, or secrets.
- **Allowed Data**:
  - Process ID (`pid`).
  - Project directory (`project_path`).
  - Provider identifier and display name.
  - Abstract lifecycle state (`LifecycleState`).
  - Abstract tool category (`ToolCategory`).
  - Timestamps, durations, and sequence numbers.

Any adapter that attempts to capture or transmit prohibited data violates the project Constitution and will be rejected.

---

## 8. Step-by-Step Tutorial: Implementing a New Adapter

Below is an example of creating a new adapter for an AI coding assistant named `aider`.

> **Normative Contracts vs. Illustrative Structure**:
> The `ProviderAdapter` trait definition, methods, lifecycle state machine, and data models documented in this guide are **normative architectural contracts**. Conversely, internal adapter state management—such as wrapping `Option<EventSink>` in an `RwLock` in the tutorial below—is **illustrative structure**. Adapter authors may structure internal synchronization and worker loops as best fits their provider's runtime.

### Step 1: Create `crates/watchai-adapters/src/aider.rs`

```rust
use crate::discovery::ProcessScanner;
use crate::traits::{DiscoveredSession, EventSink, ProviderAdapter, ProviderCapabilities};
use async_trait::async_trait;
use std::sync::RwLock;
use watchai_core::session::AdapterStatus;

pub struct AiderAdapter {
    event_sink: RwLock<Option<EventSink>>,
}

impl AiderAdapter {
    pub fn new() -> Self {
        Self {
            event_sink: RwLock::new(None),
        }
    }
}

impl Default for AiderAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderAdapter for AiderAdapter {
    fn provider_id(&self) -> &'static str {
        "aider"
    }

    fn display_name(&self) -> &'static str {
        "Aider"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::process_discovery_only()
    }

    fn attach_event_sink(&self, sink: EventSink) {
        if let Ok(mut guard) = self.event_sink.write() {
            *guard = Some(sink);
        }
    }

    async fn check_environment(&self) -> AdapterStatus {
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                if dir.join("aider").is_file() {
                    return AdapterStatus::Active;
                }
            }
        }
        AdapterStatus::DiscoveryRequired
    }

    async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
        ProcessScanner::scan_processes("aider", self.provider_id(), self.display_name())
    }
}
```

### Step 2: Register the Adapter

1. Export the module in `crates/watchai-adapters/src/lib.rs`:
   ```rust
   pub mod aider;
   ```
2. Register the adapter in `AdapterRegistry::default_registry()` in `crates/watchai-adapters/src/registry.rs`:
   ```rust
   use crate::aider::AiderAdapter;

   pub fn default_registry() -> Self {
       let mut reg = Self::new();
       reg.register(Arc::new(ClaudeCodeAdapter::new()));
       reg.register(Arc::new(CodexCliAdapter::new()));
       reg.register(Arc::new(OpenCodeAdapter::new()));
       reg.register(Arc::new(AiderAdapter::new())); // Newly added adapter
       reg
   }
   ```

### Step 3: Add Unit Tests

Add unit tests verifying command-line matching and false-positive rejection:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::ProcessScanner;

    #[test]
    fn test_aider_cmdline_matching() {
        assert!(ProcessScanner::matches_cmdline_args(
            &["aider".into(), "--model".into(), "claude-3-opus".into()],
            &["aider"]
        ));
        assert!(ProcessScanner::matches_cmdline_args(
            &["python3".into(), "/usr/local/bin/aider".into()],
            &["aider"]
        ));
        // Verify rejection of grep and shell wrappers
        assert!(!ProcessScanner::matches_cmdline_args(
            &["grep".into(), "aider".into()],
            &["aider"]
        ));
    }
}
```

---

## 9. Verification & Testing Standards

Before submitting a new adapter:
1. Ensure all workspace tests pass:
   ```bash
   cargo test --workspace
   ```
2. Verify formatting and linting:
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```
3. Test under live or mocked `/proc` scenarios using `watchai-mock worker run`.
