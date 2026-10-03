# Implementation Plan: Crash & Recovery

**Branch**: `003-crash-recovery` | **Date**: 2026-10-04 | **Spec**: [specs/003-crash-recovery/spec.md](spec.md)

**Input**: Feature specification from `/specs/003-crash-recovery/spec.md`, Phase 0 research (`research.md`), Data Model (`data-model.md`), Contracts (`contracts/dbus-ipc-contract.md`), and Quickstart Guide (`quickstart.md`).

---

## Summary

Phase 7 implements comprehensive crash resilience, deterministic recovery discovery, and client reconnection across WatchAI components. Building upon the multi-session priority aggregation and process liveness tracking merged in Phase 6, Phase 7 formally resolves:
1. **Deterministic Process Reconstruction**: On restart, `watchai-daemon` immediately scans `/proc` and restores surviving agent processes using identical surrogate session keys ($\text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$) in deterministic sorted order before claiming the well-known D-Bus bus name.
2. **Activity Knowledge Boundary Across Reboots**: Discovered surviving processes are initialized as `IDLE` with `adapter_status = AdapterStatus::DiscoveryRequired`. Zero false `ERROR` alerts or unobserved active states are fabricated without fresh telemetry.
3. **GNOME Shell Disconnect & Reconnection Safety**: When the daemon disconnects, the extension preserves open popover cards in a `CACHED / OFFLINE` presentation, pauses duration timers, switches the top-bar indicator to dimmed "Offline", and re-acquires the D-Bus proxy via an atomic 5-step synchronization handshake bounded by a strict 5.0-second timeout.
4. **Crash Loop Defense & Exponential Backoff**: Extension reconnection attempts double exponentially with $\pm 20\%$ randomized jitter from 1.0s up to a 30.0s ceiling, preventing D-Bus thundering herds during rapid daemon crashes without desktop notification spam.
5. **Fault Isolation & Clean Shutdown**: Failures reading unparseable or permission-denied `/proc` entries are safely isolated per process. The daemon shuts down cleanly on both `SIGINT` and `SIGTERM`, releasing its D-Bus well-known name.

---

## Technical Context

**Language/Version**:
- Rust 1.80+ (2021 edition) across workspace crates: `watchai-core`, `watchai-adapters`, `watchai-daemon`, `watchai-ipc`.
- GJS (GNOME JavaScript in GNOME Shell 45, 46, 47) using ECMAScript modules (ESM).

**Primary Dependencies**:
- Rust: `tokio` 1.40 (async runtime, intervals, signals, watch channels), `zbus` 4.4 (D-Bus), `serde` / `serde_json`, `chrono` 0.4 (UTC timestamps), `sha2` / `hex`, `tracing` / `tracing-subscriber`.
- GJS: `Gio` (asynchronous D-Bus proxying), `GLib` (timers and event loop), `Clutter` / `St` (UI styling and presentation).

**Storage Model**:
- 100% volatile in-memory storage (`SessionRegistry`). Zero session records, telemetry logs, or crash databases written to disk.

**Testing Framework**:
- `cargo test --workspace` (unit tests, property-based tests, mock `/proc` reader tests, integration tests).
- Headless GJS test runner (`gjs -m extension/tests/...`).

**Performance & Timing Targets**:
- Recovery `/proc` discovery sweep completes in **< 500 milliseconds** on daemon startup.
- GNOME Shell detects daemon departure within **< 250 milliseconds** via `NameOwnerChanged`.
- Client reconnection handshake bounded by a **5.0-second timeout**.
- State synchronization completes in **< 1.0 second** after daemon bus name claim.
- Reconnection backoff interval: **1.0s to 30.0s ceiling** with $\pm 20\%$ randomized jitter.

**Constraints**:
- **Task T022 Status**: Intentionally pending (`[ ]`); no undocumented Claude Code hooks required.
- **Contract Stability**: 100% backward-compatible with existing `org.freedesktop.WatchAI` D-Bus contract (`SessionDto` `(sssssssus)`).
- **Privacy Whitelist**: Zero collection, persistence, or transmission of prompts, code diffs, command arguments, or credentials.

---

## Constitution Check

*GATE: All items MUST pass before Phase 7 implementation begins.*

| Principle | Requirement | Plan Compliance Status | Verification Strategy |
| :--- | :--- | :---: | :--- |
| **I. Provider-Agnostic Core** | Core domain & recovery must not depend on vendor-specific concepts. | **PASS** | Recovery sweep operates on generic process attributes (`PID`, `starttime`, `cwd`). |
| **II. Strict Layered Separation** | Unidirectional dependencies: Adapters $\rightarrow$ Core $\rightarrow$ IPC $\rightarrow$ Extension. | **PASS** | Reconnection logic resides in `extension/`, recovery in `daemon` and `adapters`. |
| **III. Explicit State Machines** | Formal FSM modeling; no ad-hoc boolean flags. | **PASS** | Client connection state machine (`Disconnected`, `Connecting`, `Connected`, `Reconnecting`). |
| **IV. Local-First & Zero-Leakage** | Zero remote calls; zero prompt/code/token persistence. | **PASS** | 100% volatile in-memory registry; only whitelisted non-sensitive metadata processed. |
| **V. GNOME Shell Stability** | Extension must be non-blocking and compositor-safe. | **PASS** | 5.0-second asynchronous handshake timeout; all timers cleaned up on disconnect. |
| **VI. Determinism & Testing** | Comprehensive test pyramid with mock adapters. | **PASS** | Deterministic sorting for discovery; simulated D-Bus disconnect tests for extension. |
| **VII. Contract Stability** | Versioned IPC and living documentation. | **PASS** | Preserves existing `org.freedesktop.WatchAI` D-Bus interface without breaking changes. |

---

## Project Structure & Target Files

```text
specs/003-crash-recovery/
├── spec.md                     # Approved Phase 7 specification
├── plan.md                     # This implementation plan
├── research.md                 # Technical research & decisions
├── data-model.md               # Schemas, formulas, and state machines
├── quickstart.md               # Runnable validation scenarios
├── contracts/
│   └── dbus-ipc-contract.md    # D-Bus IPC contract confirmation
└── checklists/
    └── requirements.md         # Specification quality checklist

WatchAI/
├── crates/
│   ├── watchai-adapters/
│   │   ├── src/
│   │   │   └── discovery.rs    # Modify: Fault isolation per process; require start_time > 0
│   │   └── tests/
│   │       └── discovery_tests.rs # New: Unit tests for corrupt/missing stat skip
│   │
│   ├── watchai-daemon/
│   │   ├── src/
│   │   │   ├── main.rs         # Modify: Recovery sweep before D-Bus claim; sorted insertion; SIGTERM
│   │   │   └── sync.rs         # Modify: Reconnection state sync and signal throttling
│   │   └── tests/
│   │
│   └── watchai-core/
│       ├── src/
│       │   ├── liveness.rs     # Preserve: Immunity of terminal sessions; silence timeouts
│       │   └── session.rs      # Preserve: Separate last_proc_check_at from last_seen_at
│       └── tests/
│
├── extension/
│   ├── dbus_client.js          # Modify: NameOwnerChanged watcher; 5s handshake timeout; backoff
│   ├── indicator.js            # Modify: Offline visual appearance and AT-SPI description
│   ├── popover.js              # Modify: Cached card presentation mode; pause live duration timers
│   └── tests/
│       └── test_reconnect.js   # New: Unit tests for jittered backoff & handshake timeout
│
└── tests/
    └── crash_recovery_test.rs  # New: Integration test for daemon crash and deterministic rediscovery
```

---

## Detailed Implementation Architecture

### 1. Daemon Startup Recovery & Ordering (`crates/watchai-daemon/src/main.rs`)
- **Pre-Registration Discovery Sweep**:
  - Before calling `connection.serve_at()` or claiming the `org.freedesktop.WatchAI` bus name, the daemon invokes `adapter_registry.discover_all().await`.
  - Discovered surviving processes are sorted deterministically:
    ```rust
    discovered.sort_by(|a, b| {
        a.provider_id.cmp(&b.provider_id)
            .then_with(|| a.project_path.cmp(&b.project_path))
            .then_with(|| a.process_id.cmp(&b.process_id))
    });
    ```
  - Surviving processes are registered with `initial_state = LifecycleState::Idle` and `adapter_status = AdapterStatus::DiscoveryRequired`.
  - The daemon computes the initial aggregate state via `sync_aggregate_state` *before* advertising the service on D-Bus.

### 2. Process Scanner Fault Isolation (`crates/watchai-adapters/src/discovery.rs`)
- **Per-Process Exception Boundary**:
  - Scanning wraps individual `/proc/[pid]` operations in defensive guards.
  - If reading `/proc/[pid]/stat` or `cwd` fails, the error is logged at `debug` level and the scanner executes `continue` to proceed with the next process.
  - An unreadable start time tick never defaults to 0; the process is omitted for that cycle.

### 3. GNOME Shell Reconnection State Machine (`extension/dbus_client.js`)
- **Signal Monitoring**:
  - Connects to `org.freedesktop.DBus.NameOwnerChanged` filtering for `org.freedesktop.WatchAI`.
  - When the owner becomes empty (`new_owner == ""`), transitions to `Reconnecting`:
    - Disconnects all existing signal handlers (`disconnectSignal`).
    - Signals the popover and indicator to switch to `OFFLINE/CACHED` mode.
    - Initiates backoff timer.
  - When the owner becomes non-empty (`new_owner != ""`), immediately cancels any active backoff timer and begins the handshake.
- **5.0-Second Handshake Timeout**:
  - Starts a 5000ms `GLib.timeout_add` timer before initiating `GetAggregateState` and `GetSessions`.
  - If the timer fires before the initial state is unpacked, the pending promise is rejected, proxy reference is cleared, and the client enters exponential backoff retry.
- **Jittered Exponential Backoff**:
  - Tracks `this._consecutiveFailures`.
  - Formula: $\min(30000, 1000 \times 2^n) \times (0.80 + 0.40 \times \text{Math.random()})$.
  - On complete success, resets `this._consecutiveFailures = 0`.

### 4. Popover Menu Presentation Modes (`extension/popover.js`)
- **Cached Card Display**:
  - Adds `setOfflineMode(isOffline)` on `WatchAISessionPopover`.
  - When `isOffline == true`:
    - Pauses the live 1-second duration timer (`this._timerId`).
    - Adds `.watchai-card-cached` CSS class to each card.
    - Shows an offline notification header in the popover.
  - When `isOffline == false`:
    - Clears cached styling and rebuilds cards from authoritative `GetSessions()` data.
    - Resumes live duration timer.

### 5. Indicator Icon Offline Styling (`extension/indicator.js`)
- **Visual & Accessibility Updates**:
  - Adds `.watchai-indicator-offline` CSS class (dimmed symbolic icon).
  - Updates AT-SPI label: `"WatchAI — Daemon Offline"`.
  - Restores live icon and color accent upon successful reconnection.

---

## Testing Strategy

The implementation plan requires comprehensive automated tests:

1. **Daemon Recovery Integration Test (`tests/crash_recovery_test.rs`)**:
   - Spawns mock process with known PID and start time.
   - Verifies recovery discovery produces identical `session_id`.
   - Verifies surviving process initializes as `IDLE + DISCOVERY_REQUIRED`.
   - Verifies zero `ERROR` signals emitted.
   - Verifies dead processes are omitted from discovery.
2. **Process Scanner Fault Isolation Unit Tests (`crates/watchai-adapters/tests/discovery_tests.rs`)**:
   - Verifies missing or corrupt stat files return `None` and do not produce `Some(0)`.
   - Verifies unreadable process folders do not abort directory crawling.
3. **Extension Reconnection & Backoff Unit Tests (`extension/tests/test_reconnect.js`)**:
   - Verifies exponential backoff doubling (1s $\rightarrow$ 2s $\rightarrow$ 4s $\rightarrow$ ... $\le 30$s).
   - Verifies $\pm 20\%$ jitter bounds.
   - Verifies reset to 1.0s on successful handshake.
   - Verifies 5-second handshake timeout handling.

---

## Explicit Non-Goals & Scope Boundaries

Phase 7 will **NOT** implement:
- **Task T022** (Claude Code opt-in hook receiver remains strictly pending `[ ]` and discovery-gated).
- **OpenAI Codex CLI adapter** (Phase 8).
- **OpenCode adapter** (Phase 8).
- **systemd user service packaging** (Phase 9).
- **GSettings preference schema** (Phase 10).
- **Desktop notification popups** during crash loops.
- **Persistent on-disk databases** (SQLite, JSON logs).
- **Meson distribution build scripts** (Phase 12).
