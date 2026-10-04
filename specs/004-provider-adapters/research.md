# Research: Phase 8 — Multi-Provider Discovery & Capability Modeling

**Feature**: Multi-Provider Discovery & Capability Modeling (`004-provider-adapters`)  
**Date**: 2026-10-04  
**Status**: Completed  

---

## 1. Sequence Number Architectural Audit

### Problem Statement
The preliminary clarification proposed dropping events where `sequence_number <= current`. However, external sequence numbers were not part of the original adapter architecture and raise critical questions:
- Who generates the sequence number?
- What happens when a provider has no native sequence numbers (like `/proc` scanners)?
- How is ordering guaranteed across asynchronous adapters without tight coupling?

### Analysis of Questions 1–10
1. **Generator**: Process scanners cannot generate meaningful sequence numbers; they only inspect snapshots.
2. **Scope**: If per-provider, a crash or restart resets it. If global, it requires synchronized atomic clocks across threads.
3. **Initialization**: On daemon restart, historical sequence numbers are lost for discovery-only sessions.
4. **Native Telemetry**: Neither Codex CLI nor OpenCode emits native monotonic sequence numbers in process titles or arguments.
5. **D-Bus Boundary**: Public D-Bus clients consume `SessionDto` with state timestamps, not sequence numbers.

### Decision
**Remove provider-generated sequence numbers from `SessionLifecycleEvent`.**
- Ingestion events carry `timestamp: DateTime<Utc>` representing the event observation time.
- The daemon's `SessionRegistry` maintains an internal monotonic counter (`sequence_number: u64`) on `AgentSession`, incremented sequentially upon applying verified state transitions.
- Stale event detection uses timestamp comparison (`event.timestamp < session.state_entered_at`) rather than requiring adapters to invent synthetic sequence integers.
- This maintains clean provider-agnostic decoupling without speculative protocol baggage.

---

## 2. Epistemically Honest Capability Modeling

### Problem Statement
The preliminary specification proposed `supported_states = [Idle, Unknown, Error]` for `ProcessDiscoveryOnly`. 

### Epistemic Flaw Analysis
1. Observing `/proc` proves only **process presence** or **process absence**.
2. Process presence without telemetry proves only that the agent is running in the background. It does **not** prove `WORKING` or `WAITING`.
3. Process absence does **not** prove `ERROR` or `SUCCESS`. An unmonitored process could exit cleanly or crash; the adapter cannot distinguish an ungraceful exit from a successful completion without exit codes or telemetry.
4. `UNKNOWN` is not an adapter capability; it is a daemon-level timeout fallback when telemetry goes silent.
5. Claiming an adapter "supports observing Error and Unknown" is therefore an epistemic falsehood.

### Decision
Replace `supported_states: Vec<LifecycleState>` with an explicit, minimalist capability structure:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelemetryTier {
    /// Pure non-invasive /proc scanner.
    /// Can only detect process presence (initializes strictly in IDLE + DISCOVERY_REQUIRED).
    ProcessDiscoveryOnly,
    /// Passive filesystem log or FIFO tailing (future).
    PassiveLogTailing,
    /// Opt-in hook receiver or IPC socket (future).
    OptInHookTelemetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub telemetry_tier: TelemetryTier,
    /// Whether the adapter natively supports tool category extraction.
    pub supports_tool_categories: bool,
    /// Whether the adapter natively streams live activity events (WORKING / WAITING).
    pub supports_activity_events: bool,
}
```
For Phase 8:
- `ClaudeCodeAdapter`, `CodexCliAdapter`, and `OpenCodeAdapter` all report:
  - `telemetry_tier = TelemetryTier::ProcessDiscoveryOnly`
  - `supports_tool_categories = false`
  - `supports_activity_events = false`
- When `supports_activity_events == false`, the engine enforces that discovered sessions strictly initialize in `IDLE + DISCOVERY_REQUIRED`.

---

## 3. Provider Process Matching Contracts

### Requirements
Define conservative, testable `/proc` discovery contracts for Codex CLI and OpenCode that minimize false positives and reject unrelated processes (grep, shell commands, IDE extensions, build tools, child subprocesses).

### Research Findings & Matching Rules

#### 1. Codex CLI (`codex-cli`)
- **Canonical Binary Names**: `codex`, `codex-cli`.
- **Command Line Rules (`/proc/[pid]/cmdline`)**:
  - Null-delimited argument list inspected via `argv[0]`, `argv[1]`.
  - **Match Rule 1 (Direct Binary)**: `argv[0]` filename ends with `/codex` or `/codex-cli` (or equals `codex` / `codex-cli`).
  - **Match Rule 2 (Script Runner)**: `argv[0]` is a runtime runner (`node`, `bun`, `python`, `python3`) AND `argv[1]` ends with `codex` or `codex-cli` (e.g. `node /usr/bin/codex`).
  - **Rejection Rules**:
    - Any command where `argv[0]` is `grep`, `rg`, `sh`, `bash`, `zsh`, `which`, `ps`, or `whereis`.
    - Arguments containing `codex` only in flags or search patterns (e.g. `grep -rn codex .`).
    - Child subprocesses spawned by codex (e.g. `git status`, `cargo check`) where the executable is git or cargo.
- **Provider Metadata**:
  - `provider_id = "codex-cli"`
  - `provider_display_name = "OpenAI Codex"`

#### 2. OpenCode (`opencode`)
- **Canonical Binary Names**: `opencode`.
- **Command Line Rules (`/proc/[pid]/cmdline`)**:
  - **Match Rule 1 (Direct Binary)**: `argv[0]` filename ends with `/opencode` (or equals `opencode`).
  - **Match Rule 2 (Script Runner)**: `argv[0]` is a runtime runner (`python`, `python3`, `node`) AND `argv[1]` ends with `opencode`.
  - **Rejection Rules**:
    - Generic substrings in search arguments, build scripts, compilers.
    - Helper binaries like `opencode-helper` or `opencode-language-server`.
- **Provider Metadata**:
  - `provider_id = "opencode"`
  - `provider_display_name = "OpenCode"`

#### 3. Common Discovery Invariants (All Providers)
- **Project Path**: Read from `/proc/[pid]/cwd` symlink. If unreadable (permissions/exit), defaults to `/unknown/workspace`.
- **Start Time**: Extracted from `/proc/[pid]/stat` field 22 (`starttime` in clock ticks). Must be $> 0$.
- **Fault Isolation**: Per-process `Result` handling skips any unreadable process with `debug!` logging without interrupting directory traversal.
- **Surrogate Session ID**:
  $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
- **Deterministic Sort**:
  `provider_id` ASC $\rightarrow$ `project_path` ASC $\rightarrow$ `process_id` ASC.

---

## 4. Event Channel Capacity, Buffer Size, and Prioritized Backpressure

### Problem Statement
The daemon requires an asynchronous ingestion seam (`tokio::sync::mpsc::channel`) so adapters can ingest lifecycle events without tight coupling. The queue must be bounded to prevent memory leaks, but critical state transitions must never be dropped due to queue saturation from high-frequency heartbeats.

### Architecture & Backpressure Policy
- **Channel Capacity**: 256 events (tested implementation parameter).
- **Prioritization / Saturation Semantics**:
  1. **Critical Events (`StateTransition`, `SessionTerminated`)**:
     - MUST NEVER be dropped.
     - Producers await buffer reservation (`sender.send().await`).
  2. **Non-Critical Telemetry (`Heartbeat`)**:
     - Uses non-blocking `try_send()`.
     - If the channel capacity is $> 80\%$ full (e.g. $> 204$ pending events), heartbeats are dropped or coalesced, logging a debug trace. This guarantees buffer headroom for critical lifecycle transitions.
- **Consumer Processing**:
  - Dedicated background worker task in `watchai-daemon` consumes events off the channel.
  - Validates transitions against the 8-state FSM.
  - Updates `SessionRegistry`.
  - Broadcasts D-Bus `SessionUpdated` and throttled `AggregateStateChanged` signals.
- **Clean Shutdown**:
  - Dropping all `sender` handles (or sending cooperative shutdown watch token) causes the receiver loop to process all remaining queued events and terminate cleanly.

---

## 5. Environment Health Diagnostics & Startup Caching

### Decision
- **Startup-Only Evaluation**: `check_environment()` on each registered adapter is evaluated once during daemon startup.
- **Probing Rules**:
  - Probes `$PATH` directories and standard user paths (`~/.local/bin`, `/usr/local/bin`, `/usr/bin`).
  - Returns `AdapterStatus::Active` if executable is found and readable.
  - Returns `AdapterStatus::DiscoveryRequired` if not found, with structured debug logging.
- **Dynamic Caching**:
  - Background polling for newly installed binaries is rejected as unnecessary CPU/IO overhead.
  - If a user installs a new agent CLI while the daemon is running, detection takes effect upon daemon restart (or systemd user service restart).

---

## 6. Performance Target & Reproducible Benchmarking (NFR-004)

### Analysis
An absolute requirement of `<50ms` for `/proc` scanning is subject to hardware variance and filesystem caching.

### Decision
- **Classification**: Performance Target, not a functional correctness barrier.
- **Target**: Discovery sweep across all registered providers SHOULD complete within **50 milliseconds** on a baseline Linux desktop with $\le 1000$ active processes and warm `/proc` cache.
- **Benchmarking Methodology**:
  - Algorithmic scaling: Single-pass `/proc` directory traversal ($O(P)$ where $P$ is process count).
  - Rather than scanning `/proc` three separate times for Claude, Codex, and OpenCode, `ProcessScanner` or adapter traversal evaluates matching rules against the entries collected during the sweep, avoiding duplicate filesystem I/O.
  - CI unit tests use synthetic mock directory fixtures (`MockProcReader`) with 100+ processes, asserting that discovery executes in $< 10\text{ms}$ deterministically.
