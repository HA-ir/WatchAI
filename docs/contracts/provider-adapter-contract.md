# Contract: Provider Adapter Interface & Ingestion Pipeline

**Specification**: Multi-Provider Discovery & Capability Modeling (`004-provider-adapters`)  
**Crates**: `watchai-adapters`, `watchai-core`, `watchai-daemon`  
**Status**: Ratified  

---

## 1. Overview

This contract governs the architectural boundary between agent-specific adapters and the WatchAI core engine (Constitution Principle I).

All provider adapters MUST:
1. Implement the standard `ProviderAdapter` Rust trait.
2. Declare explicit observational capabilities via `ProviderCapabilities`.
3. Normalize provider-specific process realities into generic `DiscoveredSession` records.
4. Stream events exclusively through the bounded `SessionLifecycleEvent` channel.
5. Strictly adhere to local-first operation and zero-leakage privacy invariants.

---

## 2. The `ProviderAdapter` Trait

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Return the canonical provider slug (e.g. "claude-code", "codex-cli", "opencode").
    fn provider_id(&self) -> &'static str;

    /// Return the user-facing display name (e.g. "Claude Code", "OpenAI Codex", "OpenCode").
    fn display_name(&self) -> &'static str;

    /// Return the observational capability descriptor for this provider.
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::process_discovery_only()
    }

    /// Probe the local desktop environment (e.g. binary in $PATH) once on startup.
    async fn check_environment(&self) -> AdapterStatus;

    /// Scan local /proc filesystem to discover running agent processes.
    async fn discover_sessions(&self) -> Vec<DiscoveredSession>;
}
```

---

## 3. Discovery Rules & Process Identification

### 3.1 Binary Matching Rules
Adapters inspect `/proc/[pid]/cmdline` using conservative, testable matching:
- **`claude-code`**: Matches executable `claude`.
- **`codex-cli`**: Matches `argv[0]` ending with `/codex` or `/codex-cli`, or runner (`node`, `bun`, `python`) with `argv[1]` ending with `codex` / `codex-cli`.
- **`opencode`**: Matches `argv[0]` ending with `/opencode`, or runner (`python`, `node`) with `argv[1]` ending with `opencode`.

### 3.2 Rejection Rules (False-Positive Defense)
Adapters MUST explicitly reject:
- Any process where `argv[0]` is a shell or inspection utility (`sh`, `bash`, `zsh`, `grep`, `rg`, `ps`, `which`).
- Matches occurring solely within search patterns or command arguments (e.g. `grep -rn codex .`).
- Child worker processes spawned by agents (e.g. `git`, `cargo`, `pytest`) where the binary is not the agent runner.

### 3.3 Identity & Ordering Invariants
- **Surrogate Session ID**:
  $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
- **Positive Start Time**: Field 22 (`starttime` in clock ticks) MUST be $> 0$; otherwise the process is skipped.
- **Project Path**: Extracted from `/proc/[pid]/cwd`. If inaccessible, falls back to `/unknown/workspace`.
- **Fault Isolation**: Per-process `Result` handling guarantees that one broken `/proc` entry never aborts discovery for sibling processes or other providers.
- **Deterministic Sort**:
  1. `provider_id` ASC
  2. `project_path` ASC
  3. `process_id` ASC

---

## 4. Ingestion Channel Contract

1. **Channel Type**: Bounded asynchronous `tokio::sync::mpsc::channel(256)`.
2. **Prioritization Rules**:
   - `StateTransition` and `SessionTerminated` events use `.send().await` (guaranteed delivery, backpressures adapter).
   - `Heartbeat` events use `try_send()`. Heartbeats are coalesced or dropped when channel utilization exceeds 80% to ensure buffer headroom for critical state transitions.
3. **Consumer Verification**:
   - Validates state transitions against `LifecycleState::can_transition_to`.
   - Rejects stale transitions (`timestamp < session.state_entered_at`).
   - Increments internal monotonic `session.sequence_number` on applied transitions.
   - Broadcasts D-Bus `SessionUpdated` and throttled `AggregateStateChanged`.

---

## 5. D-Bus IPC & Client Invariance

- **Zero IPC Schema Changes**: The public D-Bus contract `org.freedesktop.WatchAI` remains 100% unchanged.
- **Session Struct**: Continues exposing the standard 9-field tuple `(sssssssus)`.
- **Internal Metadata**: `ProviderCapabilities` and internal sequence numbers remain strictly internal to the Rust daemon.
- **GNOME Shell Popover**: Automatically renders new provider sessions using their `provider_display_name` without requiring JavaScript changes.

---

## 6. Privacy & Governance Invariants

- **Prohibited Data Classes**: Adapters MUST NEVER collect, parse, serialize, or log prompts, completions, source code, git diffs, file contents, command arguments, or credentials.
- **Permitted Metadata**: `session_id`, `provider_id`, `provider_display_name`, `project_name`, `process_id`, `current_state`, timestamps.
- **Task T022 Governance**: Task `T022` (`Claude Code opt-in hook event telemetry receiver`) remains strictly reserved, untouched, and unchecked `[ ]`. Phase 8 MUST NOT implement Claude Code hooks.
