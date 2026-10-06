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
   The session's project directory is resolved from the `/proc/[pid]/cwd` symlink. If the symlink is unreadable, it defaults to the user's home directory.
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

2. **Script Runner Matching**:
   When agents are launched via language interpreters (`node`, `bun`, `python`, `python3`, `deno`, `ts-node`), the scanner inspects `argv[1]` to match script file names:
   ```rust
   const SCRIPT_RUNNERS: &[&str] = &["node", "bun", "python", "python3", "deno", "ts-node"];
   ```
   Matches are accepted if `argv[1]` equals the target binary or begins with the target binary name followed by an extension (e.g., `node /path/to/claude.js`).

### Deterministic Session ID Derivation

Every discovered session must generate a stable, deterministic session ID via:

```rust
use watchai_core::session::derive_process_session_id;

let session_id = derive_process_session_id(provider_id, pid, process_start_time);
```

This hashes `"<provider_id>:<pid>:<process_start_time>"` into a SHA-256 digest formatted as `<provider_id>:<hex_prefix>`. This guarantees:
- Re-scans of the same running process yield the exact same session ID.
- If the operating system recycles the PID after process termination, the new process has a different `process_start_time`, resulting in a distinct session ID.

---

## 4. Lifecycle State Machine & Normalization

WatchAI models all agent activity through an 8-state Finite State Machine (`LifecycleState` in `watchai_core::state`):

```
       ┌──────────┐
       │   IDLE   │◄───────────────────────┐
       └────┬─────┘                        │
            │                              │
            ▼                              │
       ┌──────────┐                        │
       │  ACTIVE  │                        │
       └────┬─────┘                        │
            │                              │
     ┌──────┴───────┐                      │
     ▼              ▼                      │
┌──────────┐  ┌───────────────┐            │
│ THINKING │  │EXECUTING_TOOL │            │
└────┬─────┘  └───────┬───────┘            │
     │                │                    │
     └──────┬─────────┘                    │
            ▼                              │
 ┌──────────────────────┐                  │
 │  ATTENTION_REQUIRED  │                  │
 └──────────┬───────────┘                  │
            │                              │
            ▼                              │
       ┌───────────┐                       │
       │ COMPLETED │───────────────────────┘
       └─────┬─────┘    (after 10s dwell)
             │
      ┌──────┴──────┐
      ▼             ▼
┌────────────┐ ┌────────┐
│ TERMINATED │ │ FAILED │
└────────────┘ └────────┘
```

### State Definitions

| State | Semantic Meaning |
| :--- | :--- |
| `IDLE` | Process is running and waiting for user input or prompts. |
| `ACTIVE` | Agent is processing a request or orchestrating tasks. |
| `THINKING` | LLM inference in progress (model reasoning/generating response). |
| `EXECUTING_TOOL` | Running a local tool (file modification, shell command, search). |
| `ATTENTION_REQUIRED` | Blocked waiting for human approval, permission, or feedback. |
| `COMPLETED` | Task successfully completed (subject to 10s aggregate dwell). |
| `TERMINATED` | Process ended normally or cleanly exited. |
| `FAILED` | Process crashed, errored, or exited with an unhandled failure. |

### Global Aggregate Priority

When multiple agents run concurrently, WatchAI determines the top-bar indicator state by selecting the session with the highest priority:

$$\text{ATTENTION\_REQUIRED (8)} > \text{EXECUTING\_TOOL (7)} > \text{THINKING (6)} > \text{ACTIVE (5)} > \text{IDLE (4)} > \text{COMPLETED (3)} > \text{FAILED (2)} > \text{TERMINATED (1)}$$

### Completion Dwell & Terminal Immunity

- **10-Second Completion Dwell**: When a session moves to `COMPLETED`, the aggregate indicator preserves the completion state for 10 seconds before reverting to `IDLE` (if no other sessions are active).
- **Terminal Immunity**: Once a session enters `TERMINATED` or `FAILED`, it cannot legally transition back to an active state.

---

## 5. Tool Categorization & Sanitization

For adapters that observe tool executions (`TelemetryTier::OptInHookTelemetry`), tool names must be normalized to canonical, sanitized `ToolCategory` variants:

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

### Mapping Matrix

| Provider Tool | Canonical Category | Serialized D-Bus String |
| :--- | :--- | :--- |
| `cat`, `read_file`, `View` | `ToolCategory::FileRead` | `"FILE_READ"` |
| `edit_file`, `replace`, `Write` | `ToolCategory::FileWrite` | `"FILE_WRITE"` |
| `bash`, `sh`, `exec_command` | `ToolCategory::ShellExecution` | `"SHELL_EXECUTION"` |
| `grep`, `glob`, `find_by_name` | `ToolCategory::Search` | `"SEARCH"` |
| `reasoning`, `think` | `ToolCategory::ModelThinking` | `"MODEL_THINKING"` |

**Security Boundary**: Never expose raw shell commands, file contents, or tool arguments in the tool category string. The D-Bus wire protocol expects only the sanitized category name.

---

## 6. Liveness Monitoring & Hysteresis

The daemon polls running sessions periodically (default interval: 2 seconds). To handle transient scheduling lags or procfs delays, WatchAI uses a **2-cycle failure hysteresis**:

1. **Cycle 1 Missing**: If a session's PID cannot be verified in `/proc` on poll $N$, the session is marked with a missed count of 1, but its status is **not** immediately terminated.
2. **Cycle 2 Missing**: If the PID remains unverified on poll $N+1$, the session is officially transitioned to `TERMINATED`.
3. **PID Validation**: The reader verifies that the process start time recorded at discovery matches the current process start time in `/proc/[pid]/stat`. If the PID was recycled by the OS for a different process, the session is terminated immediately.

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
  - Abstract lifecycle state.
  - Abstract tool category (`ToolCategory`).
  - Timestamps and durations.

Any adapter that attempts to capture or transmit prohibited data violates the project Constitution and will be rejected.

---

## 8. Step-by-Step Tutorial: Implementing a New Adapter

Below is an example of creating a new adapter for an AI coding assistant named `aider`.

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
