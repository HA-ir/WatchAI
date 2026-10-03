# Implementation Plan: Multi-Session Aggregation & Liveness

**Branch**: `002-multi-session-liveness` | **Date**: 2026-10-03 | **Spec**: [specs/002-multi-session-liveness/spec.md](spec.md)

**Input**: Feature specification from `/specs/002-multi-session-liveness/spec.md`, Phase 0 research (`research.md`), Data Model (`data-model.md`), and WatchAI Constitution (`.specify/memory/constitution.md`).

---

## Summary

Phase 6 implements the authoritative multi-session priority aggregation engine and hybrid process liveness checker for WatchAI. Building directly upon the foundational architecture established in Phases 1–4 and the multi-session popover UI merged in Phase 5, Phase 6 formally resolves:
1. **Mathematical Multi-Session Priority Aggregation**: Evaluates 0, 1, or $N$ concurrent sessions and computes the top-bar aggregate state matching the total order $\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$.
2. **Process Liveness & PID Recycling Defense**: Periodically inspects `/proc/[pid]/stat` every 2 seconds, verifying that field 22 (`starttime`) matches the session's recorded start ticks, preventing recycled PIDs from hijacking dead sessions.
3. **Transient Failure Hysteresis & Crash Transition**: Enforces a 2-consecutive-check failure requirement (confirming ungraceful crashes in 2–4 seconds, strictly bounded by 5 seconds max) before transitioning dead processes to `ERROR`.
4. **Decoupled 10-Second Aggregate Dwell vs. 60-Second Popover Retention**: Completed sessions remain visible in `SUCCESS`/`ERROR`/`CANCELLED` for 60 seconds in the popover before being pruned, while the top-bar indicator smoothly resets to `IDLE` 10 seconds after task completion.
5. **Activity Knowledge Boundary**: Uninstrumented `/proc` processes are classified as `IDLE` (`DISCOVERY_REQUIRED`); active execution states (`WORKING`, `WAITING`) strictly require telemetry events.

---

## Technical Context

**Language/Version**: Rust 1.80+ (2021 edition) across workspace crates:
- `watchai-core`: Aggregation math, liveness checking, and retention tasks.
- `watchai-adapters`: Process discovery with start time extraction.
- `watchai-daemon`: Background loop orchestration and signal emission throttling.
- `watchai-ipc`: Public D-Bus DTOs and service (100% backward-compatible).

**Primary Dependencies**:
- `tokio` (async runtime, intervals, sleep), `zbus 4.4` (D-Bus), `serde` / `serde_json`, `chrono` (UTC timestamps), `sha2` / `hex` (deterministic ID hashing), `tracing` (structured diagnostics).

**Storage Model**:
- 100% volatile in-memory storage (`SessionRegistry`). Zero session records written to disk.

**Testing Framework**:
- `cargo test --workspace` (unit, aggregation property tests, PID reuse tests, liveness tests).
- Standalone GJS tests (`gjs -m extension/tests/test_popover.js`, `gjs extension/tests/test_indicator.js`).

**Performance & Timing Targets**:
- Aggregation calculation completes in **< 1 millisecond** for up to 50 concurrent sessions.
- Liveness check interval: **2 seconds**.
- Crash confirmation hysteresis: **2 consecutive read failures** (2–4 seconds, bounded by 5s max).
- Top-bar completion dwell: **10 seconds**.
- Popover session retention: **60 seconds**.

**Constraints**:
- **Task T022 Status**: Intentionally pending; no undocumented Claude Code APIs are required.
- **Contract Stability**: Zero breaking changes to `org.freedesktop.WatchAI` D-Bus interface.
- **Privacy Whitelist**: Zero collection, persistence, or transmission of prompts, code diffs, command arguments, or credentials.

---

## Constitution Check

*GATE: All items MUST pass before Phase 6 implementation begins.*

| Principle | Requirement | Plan Compliance Status | Verification Strategy |
| :--- | :--- | :---: | :--- |
| **I. Provider-Agnostic Core** | Core domain & aggregation must not depend on vendor-specific concepts. | **PASS** | `compute_aggregate_state()` operates on generic `LifecycleState` enum values. |
| **II. Strict Layered Separation** | Unidirectional dependencies: Adapters $\rightarrow$ Core $\rightarrow$ IPC $\rightarrow$ Extension. | **PASS** | Liveness checking and aggregation reside strictly in `watchai-core`. |
| **III. Explicit State Machines** | Formal FSM modeling; no ad-hoc boolean flags. | **PASS** | All state changes validate against `LifecycleState::can_transition_to()`. |
| **IV. Local-First & Zero-Leakage** | Zero remote calls; zero prompt/code/token persistence. | **PASS** | Volatile in-memory registry; only whitelisted non-sensitive metadata is processed. |
| **V. GNOME Shell Stability** | Extension must be non-blocking and compositor-safe. | **PASS** | Popover relies on existing Phase 5 D-Bus signals; zero blocking operations. |
| **VI. Determinism & Testing** | Comprehensive test pyramid with mock adapters. | **PASS** | Property tests for aggregation, simulated `/proc` tests for PID reuse and crash detection. |
| **VII. Contract Stability** | Versioned IPC and living documentation. | **PASS** | Preserves existing `org.freedesktop.WatchAI` D-Bus contract without breaking changes. |

---

## Project Structure & Target Files

```text
WatchAI/
├── crates/
│   ├── watchai-core/
│   │   ├── src/
│   │   │   ├── session.rs          # Modify: Add internal process_start_time (u64) & failure counter
│   │   │   ├── aggregate.rs        # Implement: compute_aggregate_state() & dwell logic
│   │   │   └── liveness.rs         # Implement: /proc liveness check, PID reuse, retention pruning
│   │   └── tests/
│   │       ├── aggregation_tests.rs # New: Unit tests for multi-session priority & tie-breaking
│   │       └── liveness_tests.rs    # New: Unit tests for PID reuse, 2-read crash guard, retention
│   │
│   ├── watchai-adapters/
│   │   └── src/
│   │       └── discovery.rs        # Modify: Capture starttime ticks and set initial_state = Idle
│   │
│   └── watchai-daemon/
│       └── src/
│           └── main.rs             # Modify: Integrate liveness/aggregation loop; throttle D-Bus signals
│
├── specs/002-multi-session-liveness/
│   ├── spec.md                     # Approved Phase 6 specification
│   ├── plan.md                     # This implementation plan
│   ├── research.md                 # Technical research & decisions
│   ├── data-model.md               # Schemas, formulas, and lifecycle diagrams
│   ├── quickstart.md               # Runnable validation scenarios
│   ├── contracts/                  # D-Bus IPC contract confirmation
│   └── checklists/
│       └── requirements.md         # Specification quality checklist
```

---

## Detailed Implementation Architecture

### 1. Session Liveness (`crates/watchai-core/src/liveness.rs`)
- **Process Inspection**:
  - The liveness module executes every **2 seconds** over all sessions in `SessionRegistry` where `process_id.is_some()`.
  - For each session, it reads `/proc/[pid]/stat`.
- **PID Recycling Protection**:
  - It parses field 22 (`starttime`). If `current_starttime != recorded_starttime`, the original process is dead and the PID was recycled. The session immediately transitions to `ERROR`.
- **Failure Hysteresis (2 Consecutive Reads)**:
  - If reading `/proc/[pid]/stat` fails (file missing or permission error), `session.consecutive_proc_failures` increments.
  - When `consecutive_proc_failures >= 2` (spanning 2 to 4 seconds), process death is confirmed, transitioning the session to `ERROR`.
  - If a read succeeds, `consecutive_proc_failures` resets to 0.
- **Privacy Invariant**:
  - `process_start_time: Option<u64>` and `consecutive_proc_failures: u32` are private internal fields of `AgentSession`. They are deliberately excluded from public D-Bus structs.

### 2. Activity Knowledge Boundary (`crates/watchai-adapters/src/discovery.rs`)
- **Baseline Discovery**:
  - When `/proc` discovery finds an active agent process, it captures PID and `process_start_time`.
  - The session is registered with `initial_state = LifecycleState::Idle` and `adapter_status = AdapterStatus::DiscoveryRequired`.
  - The daemon does NOT infer `WORKING` or `WAITING` without telemetry events.
- **Adaptive Silence for Unmonitored Sessions**:
  - If a session lacks PID monitoring, silence $> 300$s in `WORKING` or $> 60$s in `STARTING` transitions the session to `UNKNOWN`.
  - Recovery from `UNKNOWN` to `WORKING` strictly requires a new telemetry event confirming task execution.

### 3. Mathematical Aggregation Engine (`crates/watchai-core/src/aggregate.rs`)
- Implements `compute_aggregate_state(sessions: &[AgentSession], now: DateTime<Utc>) -> AggregateCalculation`:
  - Returns `(aggregate_state: LifecycleState, active_count: u32, waiting_count: u32, error_count: u32)`.
  - Applies the total order:
    $$\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$$
  - Handles zero sessions $\rightarrow$ `IDLE`, all counts = 0.
  - Single session $\rightarrow$ matches session state.
  - Multiple sessions $\rightarrow$ selects maximum priority score.
  - Contextual tie-breaking $\rightarrow$ selects session with most recent `state_entered_at`, then lexicographical `session_id`.

### 4. Decoupled 10s Dwell vs. 60s Retention
- **10-Second Aggregate Completion Dwell**:
  - Terminal sessions (`SUCCESS`, `CANCELLED`, `ERROR`) participate in aggregate calculation for 10 seconds after `state_entered_at`.
  - When 10 seconds elapse, their effective priority score drops to 0, allowing the aggregate state to settle to `IDLE` (if no other sessions are active).
- **60-Second Popover Session Retention**:
  - The session entity in `SessionRegistry` does NOT mutate to `IDLE`. It remains in `SUCCESS`, `CANCELLED`, or `ERROR` so the popover card displays the outcome badge.
  - Exactly 60 seconds after terminal entry, the liveness worker prunes the session from the registry and emits `SessionRemoved(session_id)`.
- **Counter Calculation**:
  - `active_session_count` includes non-terminal sessions plus terminal sessions currently within their 10s completion dwell.

### 5. Daemon Restart Recovery
- On startup, `watchai-daemon` immediately scans `/proc` before starting event loops.
- For each surviving agent process, it computes:
  $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
- Re-registers surviving processes in `IDLE` (`DISCOVERY_REQUIRED`), preventing false `ERROR` transitions and duplicate sessions.

### 6. Local D-Bus IPC & Signal Throttling (`crates/watchai-ipc/src/dbus_service.rs`)
- 100% backward-compatible with existing `org.freedesktop.WatchAI` contract:
  - `GetAggregateState() -> (s, u, u, u, s)`
  - `GetSessions() -> a(sssssssus)`
  - `GetSession(s) -> (sssssssus)`
- **Signal Throttling**:
  - The daemon caches the last emitted `AggregateStateDto`.
  - `AggregateStateChanged` is emitted **only** when `state`, `active_session_count`, `waiting_session_count`, or `error_session_count` changes.

---

## Timing & Concurrency Semantics

- **Scheduler Non-Blocking Invariant**:
  - All `/proc` file reads are performed in daemon background tasks (`tokio::task::spawn_blocking` or async I/O). The GNOME Shell extension thread never performs direct filesystem operations.
- **Crash Detection Latency SLA**:
  - Check interval: 2 seconds.
  - Two consecutive failed checks confirm termination.
  - Expected confirmation: 2.0 to 4.0 seconds (strictly bounded by 5.0 seconds under normal OS scheduler conditions).
- **Safe Registry Concurrency**:
  - `SessionRegistry` uses `tokio::sync::RwLock<HashMap<String, AgentSession>>`.
  - Aggregation reads a snapshot via read lock; updates and pruning acquire write locks briefly without holding locks across I/O.

---

## Testing Strategy

The implementation plan requires comprehensive automated tests:

1. **Aggregation Unit Tests (`crates/watchai-core/tests/aggregation_tests.rs`)**:
   - Zero sessions $\rightarrow$ `IDLE` (all counts 0).
   - Single session $\rightarrow$ exact state match.
   - Priority hierarchy permutation tests (asserting `ERROR > WAITING > WORKING > STARTING > CANCELLED > SUCCESS > UNKNOWN > IDLE`).
   - Equal-priority tie-breaking (verifying timestamp and lexicographical tie-breakers).
   - Counter verification (`active_session_count`, `waiting_session_count`, `error_session_count`).
   - 10-second dwell expiration (verifying aggregate returns to `IDLE` after 10s while session remains in terminal state).
2. **Liveness & PID Reuse Unit Tests (`crates/watchai-core/tests/liveness_tests.rs`)**:
   - Process survival verification (matching PID and `starttime`).
   - PID recycling detection (matching PID, mismatched `starttime` $\rightarrow$ transitions to `ERROR`).
   - 2-consecutive-check hysteresis (single transient failure resets counter; 2 consecutive failures triggers `ERROR`).
   - Adaptive silence timeout (unmonitored `WORKING` $> 300$s $\rightarrow$ `UNKNOWN`).
   - Stale recovery (telemetry event transitions `UNKNOWN` $\rightarrow$ `WORKING`).
   - 60-second retention pruning (session removed from registry after 60s).
3. **Daemon Integration & Signal Tests**:
   - Signal suppression: asserting `AggregateStateChanged` is suppressed when session heartbeats do not alter aggregate state or counts.
   - Immediate discovery sweep on daemon startup.

---

## Scope Boundaries & Explicit Non-Goals

Phase 6 will **NOT** implement:
- **Task T022** (Claude Code opt-in hook receiver remains strictly pending and discovery-gated).
- **OpenAI Codex CLI adapter** (Phase 8).
- **OpenCode adapter** (Phase 8).
- **systemd user service packaging** (Phase 9).
- **GSettings preference schema** (Phase 10).
- **Meson distribution build scripts** (Phase 12).
- **Redesign of GNOME Shell popover UI** (the popover implemented in Phase 5 dynamically updates via existing signals).

---

## Documentation Updates

As part of Phase 6 delivery, the following documentation will be updated:
- `docs/architecture.md`: Updated with the mathematical aggregation formula and liveness checking model.
- `docs/contracts/dbus-ipc-contract.md`: Updated with signal throttling invariants.
