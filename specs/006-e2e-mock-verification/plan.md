# Implementation Plan: Phase 11 — End-to-End Test Suite & Mock Verification Harness

**Branch**: `011-e2e-mock-verification` | **Date**: 2026-10-06 | **Spec**: [specs/006-e2e-mock-verification/spec.md](spec.md)

---

## 1. Summary

Phase 11 delivers the authoritative **End-to-End Test Suite & Mock Verification Harness** for WatchAI (Tasks `T066`–`T070` from `specs/001-agent-status-indicator/tasks.md`). It establishes:
1. **`watchai-mock` CLI Binary (`T066`)**: A deterministic command-line simulator capable of driving synthetic agent sessions and spawning realistic worker processes for `/proc` discovery and crash testing without requiring real proprietary AI binaries.
2. **Four Automated E2E Test Suites in `tests/e2e/` (`T067`–`T070`)**:
   - `scenario1_startup_test.rs` (`T067`): Daemon bootstrap, D-Bus bus name acquisition within 2.0s, readiness notification (`READY=1`), initial `IDLE` state, and bounded graceful `SIGTERM` shutdown.
   - `scenario2_lifecycle_test.rs` (`T068`): Mock session lifecycle progression (`STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS` $\rightarrow$ real 10s completion dwell $\rightarrow$ `IDLE`), validating `SessionAdded`, `SessionUpdated`, and `AggregateStateChanged` signal ordering.
   - `scenario3_aggregation_test.rs` (`T069`): Multi-session conflict resolution ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$), active/waiting/error counters, and contextual tie-breaking.
   - `scenario4_crash_test.rs` (`T070`): Abrupt process termination (`kill -9`) detection via `/proc` liveness failure hysteresis (2.0s–4.0s, bounded by 5.0s max), transitioning session to `ERROR` without daemon deadlock.
3. **Strict Production Purity & Invariants**:
   - The daemon tested is the **real production daemon binary** (`watchai-daemon`).
   - Mock event injection operates strictly through an ephemeral Unix-domain socket enabled only when `WATCHAI_TEST_SOCKET` is present in the environment; production runs create zero test sockets and expose zero test injection endpoints.
   - D-Bus interface `org.freedesktop.WatchAI` and tuple `(sssssssus)` remain 100% immutable.
   - Task `T022` remains strictly discovery-gated and unchecked `[ ]`.

---

## 2. Technical Context

- **Platform & Target**: Linux Desktop (Ubuntu 24.04+, Fedora 40+, Debian 12) with `/proc` filesystem and D-Bus.
- **Languages & Frameworks**: Rust 1.75+ (2021 edition), `tokio` 1.40 (full async runtime), `zbus` 4.4 (D-Bus client/server IPC), `chrono` 0.4 (UTC timestamps), `serde` / `serde_json` 1.0 (socket wire protocol), `tempfile` 3.10 (isolated test directories), `nix` 0.29 (process signals and process group control).
- **Execution Mode**: 100% headless automated execution (`cargo test --test '*'`), running without Mutter, X11, or Wayland display servers.
- **Process Isolation**: Each test scenario spawns its own ephemeral `dbus-daemon --session` instance and private Unix-domain sockets, guaranteeing zero bus collisions with the developer's live desktop session or parallel test runners.
- **Zero Real AI Binaries**: Simulates realistic process signatures and events without downloading or running Claude Code, Codex, or OpenCode.
- **Zero-Leakage Privacy**: Mock protocol and test assertions strictly prohibit prompt text, tool arguments, source code diffs, tokens, or credentials.

---

## 3. Constitution Check

| Principle | Status | Verification & Architectural Evidence |
| :--- | :---: | :--- |
| **I. Provider-Agnostic Architecture** | **PASS** | `watchai-mock` can simulate all registered providers (`claude-code`, `codex-cli`, `opencode`) using provider display names without provider-specific test coupling. |
| **II. Strict Layered Separation** | **PASS** | Tests exercise: `watchai-mock` $\rightarrow$ real daemon $\rightarrow$ real core FSM/aggregation $\rightarrow$ real D-Bus. Production crates gain no test-only backdoors; event injection is gated strictly by `WATCHAI_TEST_SOCKET`. |
| **III. Explicit FSM Modeling** | **PASS** | E2E tests validate all valid FSM transitions and assert that invalid transitions are rejected. Activity Knowledge Boundary is strictly verified. |
| **IV. Local-First & Zero-Leakage Privacy** | **PASS** | All mock events and D-Bus payloads strictly adhere to the metadata-only boundary. No prompts, diffs, tool parameters, or credentials enter the mock protocol. |
| **V. GNOME Extension Stability** | **PASS** | Public D-Bus contract `(sssssssus)` and interface `org.freedesktop.WatchAI` remain 100% immutable, ensuring zero regression for the Phase 10 extension. |
| **VI. Determinism & Quality Gates** | **PASS** | Bounded timeouts, deterministic tie-breaking assertions, real 10-second completion dwell evaluation, and RAII child process cleanup ensure zero test flakiness. |
| **VII. Contract Stability & Observability** | **PASS** | Task `T022` remains permanently discovery-gated and unchecked `[ ]`. Structured logging and diagnostic capture are preserved. |

---

## 4. Affected & New Files

### Production Files Requiring Minimal Changes

1. **`crates/watchai-core/src/session.rs`**:
   - Add `SessionRegistered` variant to `SessionLifecycleEvent`:
     ```rust
     SessionRegistered {
         session_id: String,
         provider_id: String,
         provider_display_name: String,
         project_path: String,
         process_id: Option<u32>,
         initial_state: LifecycleState,
         timestamp: DateTime<Utc>,
     }
     ```
   - In `process_lifecycle_event()`: Handle `SessionRegistered`. If session is not in registry, create it via `AgentSession::new()`, insert into registry, and return `EventProcessingOutcome::Applied`.
2. **`crates/watchai-daemon/src/main.rs`**:
   - In `ingest_handle`: When `SessionRegistered` is applied, emit `SessionAdded` and synchronize/emit `AggregateStateChanged`.
   - Inspect `WATCHAI_TEST_SOCKET` env var. If present, spawn `run_test_socket_listener(socket_path, event_tx, shutdown_rx)` task. If absent, spawn nothing (zero production overhead).
   - In graceful shutdown drain: include `test_socket_handle` in worker drain future.
3. **`crates/watchai-daemon/src/lib.rs`**:
   - Expose `pub mod test_socket;`.
4. **`crates/watchai-mock/Cargo.toml`**:
   - Add `serde_json.workspace = true`, `serde.workspace = true`.
5. **`tests/Cargo.toml`**:
   - Register the 4 E2E integration test binaries (`scenario1_startup_test`, `scenario2_lifecycle_test`, `scenario3_aggregation_test`, `scenario4_crash_test`).

### New Files to Create

1. **`crates/watchai-daemon/src/test_socket.rs`**:
   - Implements `run_test_socket_listener(socket_path, event_tx, shutdown_rx)`:
     - Binds `tokio::net::UnixListener` at `socket_path`.
     - Deserializes newline-delimited JSON strings into `SessionLifecycleEvent`.
     - Forwards events into `event_tx.send(event).await`.
     - Exits on shutdown and unlinks socket file.
2. **`crates/watchai-mock/src/cli.rs`**:
   - Implements CLI parsing for `session` (`start`, `transition`, `heartbeat`, `terminate`) and `worker` (`run`, `kill`).
3. **`crates/watchai-mock/src/main.rs`**:
   - Implements `watchai-mock` binary entry point, executing parsed subcommands:
     - `session`: Connects to `WATCHAI_TEST_SOCKET` and sends serialized JSON events.
     - `worker run`: Runs background sleep loop with realistic command-line signature.
     - `worker kill`: Sends requested signal (`SIGTERM` / `SIGKILL`) to target PID.
4. **`tests/e2e/harness.rs`**:
   - Implements `E2eTestFixture` and `ChildGuard`:
     - Spawns ephemeral `dbus-daemon --session --print-address`.
     - Sets `DBUS_SESSION_BUS_ADDRESS`.
     - Spawns `watchai-daemon` child process with `WATCHAI_TEST_SOCKET`.
     - Captures daemon stdout/stderr for failure diagnostics.
     - Connects `zbus::Connection` for querying methods and monitoring signals.
     - Implements RAII `Drop` killing child processes and unlinking temp directories.
5. **`tests/e2e/scenario1_startup_test.rs` (T067)**:
   - Automates Quickstart Scenario 1.
6. **`tests/e2e/scenario2_lifecycle_test.rs` (T068)**:
   - Automates Quickstart Scenario 2 (including full 10-second completion dwell).
7. **`tests/e2e/scenario3_aggregation_test.rs` (T069)**:
   - Automates Quickstart Scenario 3 (multi-session priority conflict resolution).
8. **`tests/e2e/scenario4_crash_test.rs` (T070)**:
   - Automates Quickstart Scenario 4 (abrupt process crash and liveness hysteresis).
9. **`docs/e2e-testing.md`**:
   - Developer documentation for running and understanding the E2E verification suite.

---

## 5. Architectural Design Details

### 5.1 Mock Event Injection Protocol (`WATCHAI_TEST_SOCKET`)

```
┌────────────────────────────────────────────────────────┐
│              watchai-mock CLI / Test                   │
│                                                        │
│  watchai-mock session start --id s1 --state STARTING   │
│  watchai-mock session transition --id s1 --state WORK  │
└───────────────────────────┬────────────────────────────┘
                            │ Unix Domain Stream Socket
                            │ (Newline-Delimited JSON)
┌───────────────────────────▼────────────────────────────┐
│           watchai-daemon (Real Production Binary)      │
│                                                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │  test_socket.rs (Active only if env var set)     │  │
│  └────────────────────────┬─────────────────────────┘  │
│                           │ event_tx.send(event)       │
│                           ▼                            │
│  ┌──────────────────────────────────────────────────┐  │
│  │  event_rx.recv() -> process_lifecycle_event()    │  │
│  │  (Exact same ingestion channel as adapters)      │  │
│  └────────────────────────┬─────────────────────────┘  │
│                           │                            │
│                           ▼                            │
│  ┌──────────────────────────────────────────────────┐  │
│  │  SessionRegistry (Volatile Memory)               │  │
│  │  sync_aggregate_state() (Core Math Engine)       │  │
│  └────────────────────────┬─────────────────────────┘  │
│                           │                            │
│                           ▼                            │
│  ┌──────────────────────────────────────────────────┐  │
│  │  WatchAiDbusService (zbus 4.4 IPC)               │  │
│  │  - Signals: SessionUpdated, AggregateStateChanged│  │
│  │  - Methods: GetAggregateState, GetSessions       │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────┘
```

- **Wire Format**: Newline-delimited UTF-8 JSON.
- **Example Payload (`SessionRegistered`)**:
  ```json
  {"SessionRegistered":{"session_id":"sess-1","provider_id":"claude-code","provider_display_name":"Claude Code","project_path":"/tmp/proj1","process_id":null,"initial_state":"Starting","timestamp":"2026-10-06T12:00:00Z"}}
  ```
- **Example Payload (`StateTransition`)**:
  ```json
  {"StateTransition":{"session_id":"sess-1","new_state":"Working","tool_category":null,"timestamp":"2026-10-06T12:00:01Z"}}
  ```

### 5.2 Real 10-Second Completion Dwell Verification (Scenario 2)

In `scenario2_lifecycle_test.rs`:
1. Session transitions `WAITING` $\rightarrow$ `SUCCESS`.
2. Daemon emits `AggregateStateChanged('SUCCESS', 1, 0, 0, ...)`.
3. Test captures start timestamp $T_0$.
4. Test enters polling loop: queries `GetAggregateState()` every 250ms with a 12.0s bounded timeout.
5. At $T \approx 10.0\text{s}$, the daemon's internal dwell evaluator in `sync_aggregate_state` detects dwell expiry (`COMPLETION_DWELL_SECONDS = 10`) and transitions aggregate state to `IDLE` (`active_session_count = 0`).
6. Test asserts elapsed time was $\ge 9.8\text{s}$ and $\le 12.0\text{s}$, proving the production dwell timer operated accurately in wall-clock time.

### 5.3 Process Lifecycle & RAII Cleanup Strategy (`ChildGuard`)

```rust
pub struct ChildGuard {
    child: Option<std::process::Child>,
    pgid: Option<nix::unistd::Pid>,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // 1. Try graceful SIGTERM to process group
            if let Some(pgid) = self.pgid {
                let _ = nix::sys::signal::killpg(pgid, nix::sys::signal::Signal::SIGTERM);
            }
            let _ = child.kill(); // Ensure parent process terminates
            let _ = child.wait(); // Prevent zombie process
        }
    }
}
```

- Each test scenario uses `tempfile::Builder::new().prefix("watchai-e2e-").tempdir()`.
- Spawns `dbus-daemon --session --print-address --nofork` wrapped in `ChildGuard`.
- Spawns `watchai-daemon` wrapped in `ChildGuard`.
- If an assertion fails, the test fixture dumps daemon stdout/stderr before re-panicking, ensuring full visibility in CI logs.

---

## 6. Implementation Sequencing & Dependency Graph

```
Step 1: Core Domain Event Extension
        crates/watchai-core/src/session.rs (SessionRegistered variant & outcome)
             │
             ▼
Step 2: Daemon Test Socket Listener & Ingestion Wiring
        crates/watchai-daemon/src/test_socket.rs
        crates/watchai-daemon/src/main.rs (WATCHAI_TEST_SOCKET check & drain)
             │
             ▼
Step 3: watchai-mock CLI Binary (T066)
        crates/watchai-mock/src/cli.rs
        crates/watchai-mock/src/main.rs
        crates/watchai-mock/Cargo.toml
             │
             ▼
Step 4: E2E Test Fixture & Ephemeral D-Bus Harness
        tests/e2e/harness.rs (E2eTestFixture, ChildGuard, diagnostics)
        tests/Cargo.toml
             │
             ├────────────────────────────────────────────────────────┐
             ▼                                                        ▼
Step 5: Scenario 1: Startup & Registration (T067)        Step 6: Scenario 2: Lifecycle & Dwell (T068)
        tests/e2e/scenario1_startup_test.rs                      tests/e2e/scenario2_lifecycle_test.rs
             │                                                        │
             ├────────────────────────────────────────────────────────┘
             ▼
Step 7: Scenario 3: Multi-Session Priority (T069)
        tests/e2e/scenario3_aggregation_test.rs
             │
             ▼
Step 8: Scenario 4: Process Crash & Liveness (T070)
        tests/e2e/scenario4_crash_test.rs
             │
             ▼
Step 9: Full Suite Verification & Documentation
        - cargo test --workspace (all unit, integration, and E2E tests)
        - all 6 GJS test suites
        - docs/e2e-testing.md
        - specs/001-agent-status-indicator/tasks.md synchronization
```

---

## 7. Detailed Test Plans for T067–T070

### 7.1 Scenario 1: Startup & D-Bus Registration (`scenario1_startup_test.rs`)
- **Action**: Launch daemon with `E2eTestFixture`.
- **Assertions**:
  1. Daemon claims `org.freedesktop.WatchAI` within 2.0s.
  2. `GetAggregateState()` returns `('IDLE', 0, 0, 0, ...)`.
  3. `GetSessions()` returns empty vector.
  4. Sending `SIGTERM` causes daemon to release bus name and exit with code 0 within 5.0s.

### 7.2 Scenario 2: Lifecycle Progression & Dwell (`scenario2_lifecycle_test.rs`)
- **Action**: Step session through `STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` (tool `Bash`) $\rightarrow$ `SUCCESS` via `watchai-mock`.
- **Assertions**:
  1. D-Bus emits `SessionAdded` on start.
  2. D-Bus emits `SessionUpdated` and `AggregateStateChanged` on each transition.
  3. `waiting_session_count = 1` and `active_tool_category = 'Bash'` during `WAITING`.
  4. Aggregate reflects `SUCCESS` upon completion.
  5. After 10s completion dwell elapses, aggregate state smoothly resets to `IDLE` with `active_session_count = 0`.

### 7.3 Scenario 3: Multi-Session Priority Aggregation (`scenario3_aggregation_test.rs`)
- **Action**: Register Session 1 (`WORKING`) and Session 2 (`WAITING`).
- **Assertions**:
  1. Aggregate state is `WAITING` with active=2, waiting=1, error=0.
  2. Session 1 transitions to `ERROR` $\rightarrow$ aggregate immediately becomes `ERROR` (`ERROR` > `WAITING`).
  3. Introduce Session 3 in `ERROR` with newer timestamp $\rightarrow$ contextual tie-break selects Session 3.
  4. Both sessions transition to `SUCCESS` $\rightarrow$ aggregate dwells 10s in `SUCCESS` before resetting to `IDLE`.

### 7.4 Scenario 4: Process Crash & Liveness Detection (`scenario4_crash_test.rs`)
- **Action**: Spawn mock worker process matching `claude-code`, verify discovery in `IDLE`, transition to `WORKING`, then send `SIGKILL` to worker PID.
- **Assertions**:
  1. Worker dies immediately (`kill -9`).
  2. Daemon's `/proc` liveness loop detects missing PID within 2 consecutive checks (2.0–4.0s, bounded by 5.0s max).
  3. Session transitions to `ERROR` and aggregate state transitions to `ERROR`.
  4. Daemon remains fully responsive to D-Bus method calls.

---

## 8. Risks & Mitigations

| Risk | Impact | Mitigation |
| :--- | :---: | :--- |
| **D-Bus Session Collision** | Tests failing due to conflicting name claims on developer desktop. | Ephemeral `dbus-daemon` spawned per test scenario with private address. |
| **Zombie Child Processes** | Leaked daemon or worker processes consuming CPU/RAM in CI. | RAII `ChildGuard` process group kill on `Drop` guaranteed on panic or timeout. |
| **Timing Flakiness in Dwell Test** | Test asserting dwell at exact second boundary failing on slow CI runners. | Bounded polling loop checking state transition every 250ms with a 12.0s ceiling. |
| **Production Socket Exposure** | Test socket opening in production systems. | Strictly gated by `WATCHAI_TEST_SOCKET` environment variable; zero sockets bound if unset. |
| **Contract Regression** | Breaking Phase 10 GNOME extension compatibility. | Zero changes to D-Bus signatures, properties, or interfaces; 100% backward-compatible. |
| **Task T022 Regression** | Accidentally implementing Claude Code hook telemetry. | T022 remains permanently discovery-gated and strictly unchecked `[ ]`. |

---

## 9. Documentation Updates

1. **`docs/e2e-testing.md`**: New guide for developers explaining how to run the E2E verification suite locally and in CI.
2. **`specs/001-agent-status-indicator/tasks.md`**: Synchronize tasks `T066`–`T070` to completed `[x]` upon implementation completion.
