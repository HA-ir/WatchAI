---
description: "Actionable implementation task breakdown for WatchAI Phase 11 — End-to-End Test Suite & Mock Verification Harness"
---

# Tasks: End-to-End Test Suite & Mock Verification Harness

**Input**: Design documents from `/specs/006-e2e-mock-verification/` (`spec.md`, `plan.md`, `checklists/requirements.md`, `checklists/implementation.md`).
**Prerequisites**: Approved specification, clarified failure decisions, constitution v1.0.0, Phase 10 merged (commit `baefc83`).
**Branch**: `011-e2e-mock-verification`

## Format: `- [ ] [TaskID] [P?] [US?] [CanonicalID] Description with file path`
- **[P]**: Can run in parallel (different files, no blocking dependencies).
- **[US?]**: Maps task directly to user stories (`[US1]`, `[US2]`, `[US3]`, `[US4]`) from `spec.md`.
- **[CanonicalID]**: Traces directly to canonical tasks (`[T066]`, `[T067]`, `[T068]`, `[T069]`, `[T070]`) from `specs/001-agent-status-indicator/tasks.md`.
- **Traceability**: Every task cites corresponding `FR-*` requirements, `CHK-*` checklist items, and verification evidence.

---

## Phase 1: Domain Event Extension & Test-Scoped Ingestion Infrastructure [T066 Foundation]

**Purpose**: Extend core lifecycle events to support direct mock session registration, and implement the unprivileged Unix-domain test socket listener active strictly when `WATCHAI_TEST_SOCKET` is present in the environment.

- [x] T147 [P] [T066] Extend `SessionLifecycleEvent` enum in `crates/watchai-core/src/session.rs` with `SessionRegistered` variant containing `session_id: String`, `provider_id: String`, `provider_display_name: String`, `project_path: String`, `process_id: Option<u32>`, `initial_state: LifecycleState`, and `timestamp: DateTime<Utc>`.
  *Traceability*: FR-005; CHK-FEAS-002.
  *Verification*: `cargo check -p watchai-core`.

- [x] T148 [T066] Update `process_lifecycle_event()` in `crates/watchai-core/src/session.rs` to handle `SessionRegistered` prior to unknown-session checks: if session does not exist in registry, construct `AgentSession::new()` with `AdapterStatus::Active`, initialize timestamps, insert into registry, and return `EventProcessingOutcome::Created { session_id, initial_state }` (if session already exists, return `DroppedInvalidTransition`) (depends on T147).
  *Traceability*: FR-005; CHK-FEAS-002.
  *Verification*: Unit tests in T149.

- [x] T149 [P] [T066] Add unit tests in `crates/watchai-core/tests/event_tests.rs` asserting `SessionRegistered` event processing: session insertion into registry, duplicate session rejection, and correct initial state assignment (depends on T148).
  *Traceability*: CHK-FEAS-002.
  *Verification*: `cargo test --test event_tests`.

- [x] T150 [T066] Implement `run_test_socket_listener(socket_path, event_tx, shutdown_rx)` in `crates/watchai-daemon/src/test_socket.rs`: bind `tokio::net::UnixListener` at `socket_path`, enforce a strict 64 KiB line length limit using bounded buffering to prevent unbounded memory allocation, deserialize newline-delimited JSON lines into `SessionLifecycleEvent`, safely discard malformed JSON without aborting listener, forward valid events to `event_tx.send(event).await`, handle client EOF cleanly, and unlink the socket file upon shutdown.
  *Traceability*: FR-005; CHK-SOCK-001, CHK-SOCK-003, CHK-SOCK-004, CHK-SOCK-005, CHK-SOCK-006, CHK-SOCK-007, CHK-SOCK-008.
  *Verification*: Unit tests in T152.

- [x] T151 [T066] Update `crates/watchai-daemon/src/main.rs` and `crates/watchai-daemon/src/lib.rs` to expose `pub mod test_socket;`, check `std::env::var("WATCHAI_TEST_SOCKET")`, spawn `test_socket_handle` when set (remaining a complete no-op with zero listeners when unset), handle `EventProcessingOutcome::Created` in `ingest_handle` by emitting `SessionAdded` and synchronizing aggregate state (while `EventProcessingOutcome::Applied` emits `SessionUpdated`), and drain `test_socket_handle` during graceful worker shutdown (depends on T150).
  *Traceability*: FR-005; CHK-SOCK-001, CHK-SOCK-002; CHK-FEAS-003.
  *Verification*: `cargo check -p watchai-daemon`.

- [x] T152 [P] [T066] Write unit tests in `crates/watchai-daemon/src/test_socket.rs` verifying test socket listener: connection acceptance, newline-delimited JSON ingestion, 64 KiB line limit enforcement (discarding oversized lines), malformed JSON recovery, and clean shutdown unlinking (depends on T150, T151).
  *Traceability*: CHK-SOCK-003, CHK-SOCK-004, CHK-SOCK-007.
  *Verification*: `cargo test -p watchai-daemon --lib test_socket`.

**Checkpoint**: Core domain and daemon support test-scoped, memory-bounded event injection via `WATCHAI_TEST_SOCKET` with 100% production purity when unset.

---

## Phase 2: `watchai-mock` CLI Binary Implementation [T066]

**Goal**: Implement the deterministic `watchai-mock` CLI binary supporting `session` lifecycle commands and `worker` simulation commands.

**Independent Test**: Execute `watchai-mock --help` and verify subcommands; run simulated `worker run` and verify process command-line matching in `/proc`; send events over a test socket and assert receipt.

- [x] T153 [P] [T066] Update `crates/watchai-mock/Cargo.toml` to declare dependencies on `serde.workspace = true`, `serde_json.workspace = true`, and `nix.workspace = true`.
  *Traceability*: CHK-FEAS-001.
  *Verification*: Cargo manifest inspection.

- [x] T154 [P] [T066] Implement command-line argument parser in `crates/watchai-mock/src/cli.rs` supporting subcommands:
  - `session start --id <id> --provider <provider> --project <path> [--state <state>] [--pid <pid>]`
  - `session transition --id <id> --state <state> [--tool <category>]`
  - `session heartbeat --id <id>`
  - `session terminate --id <id> [--exit-code <code>]`
  - `worker run [--provider <provider>] [--project <path>] [--hold-seconds <secs>]`
  - `worker kill --pid <pid> [--signal <sig>]`
  with `--socket <path>` support (defaulting to `WATCHAI_TEST_SOCKET` env var) and `--help` usage output.
  *Traceability*: FR-002, FR-004; CHK-REQ-001.
  *Verification*: Unit tests in T157.

- [x] T155 [T066] Implement `session` command execution in `crates/watchai-mock/src/main.rs`: connect to the Unix-domain socket specified by `--socket` or `WATCHAI_TEST_SOCKET`, serialize requested `SessionLifecycleEvent` as JSON with a trailing newline (`\n`), flush stream, and report success or connection failure diagnostics (depends on T153, T154).
  *Traceability*: FR-002, FR-005, FR-006; CHK-REQ-001, CHK-PRIV-001, CHK-PRIV-002.
  *Verification*: Integration tests in T157.

- [x] T156 [T066] Implement `worker run` and `worker kill` execution in `crates/watchai-mock/src/main.rs`:
  - `worker run`: Creates a temporary Python sleeper script named `<provider>.py` (e.g. `claude.py`) in the target project path and spawns `python3 <project>/<provider>.py` so that `argv[1]` satisfies `ProcessScanner` script-runner matching rules for real `/proc` discovery, holding for `--hold-seconds`.
  - `worker kill`: Resolves target PID and transmits requested signal (`SIGTERM` or `SIGKILL`, defaulting to `SIGKILL`) using `nix::sys::signal::kill` (depends on T154).
  *Traceability*: FR-002, FR-003; CHK-REQ-001, CHK-SCEN-012.
  *Verification*: Integration tests in T157.

- [x] T157 [P] [T066] Write integration tests in `crates/watchai-mock/tests/mock_cli_tests.rs` asserting CLI parsing, socket transmission, JSON serialization format, and `worker run` `/proc` signature compliance (depends on T154, T155, T156).
  *Traceability*: CHK-REQ-001, CHK-PRIV-002.
  *Verification*: `cargo test -p watchai-mock`.

**Checkpoint**: `watchai-mock` CLI is fully functional, capable of injecting lifecycle events over Unix sockets and spawning discovery-compatible mock workers.

---

## Phase 3: Shared E2E Test Harness & Scenario 1 — Startup & D-Bus Registration [US1] [T067] 🎯 MVP

**Goal**: Establish the isolated E2E test fixture with ephemeral `dbus-daemon`, RAII child cleanup, and automate Quickstart Scenario 1.

**Independent Test**: Execute `cargo test --test scenario1_startup_test` headlessly without display servers, asserting daemon claims `org.freedesktop.WatchAI` within 2.0s and terminates cleanly on `SIGTERM`.

- [x] T158 [P] [T067] Update `tests/Cargo.toml` to add `nix.workspace = true` and `serde_json.workspace = true` dependencies, and register integration test targets:
  - `[[test]] name = "scenario1_startup_test" path = "e2e/scenario1_startup_test.rs"`
  - `[[test]] name = "scenario2_lifecycle_test" path = "e2e/scenario2_lifecycle_test.rs"`
  - `[[test]] name = "scenario3_aggregation_test" path = "e2e/scenario3_aggregation_test.rs"`
  - `[[test]] name = "scenario4_crash_test" path = "e2e/scenario4_crash_test.rs"`.
  *Traceability*: FR-007; CHK-FEAS-004.
  *Verification*: Cargo manifest inspection.

- [x] T159 [T067] Implement `tests/e2e/harness.rs`:
  - Implement `ChildGuard`: RAII guard wrapping `std::process::Child` and process group ID (`setpgid(0, 0)`), sending `SIGTERM` followed by bounded `SIGKILL` on `Drop`, and reaping child via `wait()`.
  - Implement `E2eTestFixture`: Spawns ephemeral `dbus-daemon --session --print-address --nofork`, parses `DBUS_SESSION_BUS_ADDRESS`, creates temporary test directory with socket path, spawns `watchai-daemon` child process with `WATCHAI_TEST_SOCKET`, captures daemon stdout/stderr into memory buffers for failure diagnostics, establishes a `zbus::Connection`, and exposes helper methods (`wait_for_dbus_ready`, `get_aggregate_state`, `get_sessions`, `send_mock_event`, `run_mock_cli`) (depends on T158).
  *Traceability*: FR-008, FR-009, FR-020, FR-021, FR-022; CHK-BUS-001, CHK-BUS-002, CHK-BUS-003, CHK-BUS-004, CHK-BUS-005, CHK-PROC-001, CHK-PROC-002, CHK-PROC-003, CHK-PROC-004, CHK-PROC-005, CHK-PROC-006.
  *Verification*: Compiles in T160.

- [x] T160 [US1] [T067] Implement Quickstart Scenario 1 in `tests/e2e/scenario1_startup_test.rs`:
  - Assert `watchai-daemon` claims well-known D-Bus bus name `org.freedesktop.WatchAI` within 2.0s.
  - Assert `GetAggregateState` returns `('IDLE', 0, 0, 0, '<ISO_TIMESTAMP>')`.
  - Assert `GetSessions` returns empty array with zero errors.
  - Assert sending `SIGTERM` causes daemon to release bus name (broadcasting `NameOwnerChanged`), drain workers, and exit cleanly with code 0 within 5.0s (depends on T159).
  *Traceability*: FR-010, FR-011; Acceptance Scenarios 1.1, 1.2, 1.3, 1.4; CHK-SCEN-001, CHK-SCEN-002, CHK-SCEN-003, CHK-SCEN-004.
  *Verification*: `cargo test --test scenario1_startup_test`.

**Checkpoint**: Scenario 1 passes MVP, proving isolated daemon startup, D-Bus service claim, and graceful shutdown.

---

## Phase 4: Scenario 2 — Mock Session Lifecycle Progression & Real 10s Dwell [US2] [T068]

**Goal**: Automate Quickstart Scenario 2, stepping a session through full lifecycle progression and verifying D-Bus signal order and the authentic 10-second completion dwell.

**Independent Test**: Execute `cargo test --test scenario2_lifecycle_test`, asserting signal stream delivers `SessionAdded`, `SessionUpdated`, and `AggregateStateChanged` in order, and aggregate smoothly resets to `IDLE` after 10s completion dwell.

- [x] T161 [US2] [T068] Implement Quickstart Scenario 2 in `tests/e2e/scenario2_lifecycle_test.rs`:
  - Start daemon with `E2eTestFixture`.
  - Register mock session via `watchai-mock session start --id s1 --provider claude-code --project /tmp/proj1 --state Starting`; assert D-Bus emits `SessionAdded` and `AggregateStateChanged('STARTING', 1, 0, 0, ...)`.
  - Transition session: `watchai-mock session transition --id s1 --state Working`; assert `SessionUpdated` and `AggregateStateChanged('WORKING', 1, 0, 0, ...)`.
  - Transition session: `watchai-mock session transition --id s1 --state Waiting --tool ShellExecution`; assert `SessionUpdated` (`active_tool_category = 'SHELL_EXECUTION'`) and `AggregateStateChanged('WAITING', 1, 1, 0, ...)`.
  - Transition session: `watchai-mock session transition --id s1 --state Success`; assert `AggregateStateChanged('SUCCESS', 1, 0, 0, ...)` entering 10-second dwell window.
  - Evaluate real 10-second completion dwell: poll `GetAggregateState()` every 250ms with a bounded 12.0s timeout; assert aggregate state transitions smoothly to `IDLE` with `active_session_count = 0` at elapsed time $\ge 9.8\text{s}$ and $\le 12.0\text{s}$, asserting emission of `AggregateStateChanged('IDLE', 0, 0, 0, ...)` (depends on T155, T159).
  *Traceability*: FR-012, FR-013, FR-014; Acceptance Scenarios 2.1, 2.2, 2.3, 2.4, 2.5; CHK-SCEN-005, CHK-SCEN-006, CHK-SCEN-007, CHK-SCEN-008; CHK-TIME-001, CHK-TIME-002.
  *Verification*: `cargo test --test scenario2_lifecycle_test`.

**Checkpoint**: Scenario 2 proves complete end-to-end lifecycle progression and authentic 10-second completion dwell.

---

## Phase 5: Scenario 3 — Multi-Session Priority Aggregation & Conflict Resolution [US3] [T069]

**Goal**: Automate Quickstart Scenario 3, verifying multi-session concurrency, mathematical priority hierarchy ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$), active counters, and contextual tie-breaking.

**Independent Test**: Execute `cargo test --test scenario3_aggregation_test`, asserting `WAITING` overrides `WORKING`, `ERROR` overrides `WAITING`, and counters accurately track multi-session state.

- [x] T162 [US3] [T069] Implement Quickstart Scenario 3 in `tests/e2e/scenario3_aggregation_test.rs`:
  - Start daemon with `E2eTestFixture`.
  - Register Session 1 (`WORKING`) and Session 2 (`WAITING`); assert `GetAggregateState()` returns `WAITING` (`active=2, waiting=1, error=0`), proving `WAITING` overrides `WORKING`.
  - Transition Session 1 to `ERROR`; assert aggregate state immediately updates to `ERROR` (`ERROR` overrides `WAITING`), with `error_session_count = 1`.
  - Register Session 3 in `ERROR` with a newer timestamp; assert contextual single-session inspection selects Session 3 (verifying recency tie-breaking).
  - Terminate sessions to `SUCCESS`; assert each session completes its 10-second dwell before aggregate state smoothly resets to `IDLE` (depends on T155, T159).
  *Traceability*: FR-015, FR-016, FR-017; Acceptance Scenarios 3.1, 3.2, 3.3, 3.4; CHK-SCEN-009, CHK-SCEN-010, CHK-SCEN-011.
  *Verification*: `cargo test --test scenario3_aggregation_test`.

**Checkpoint**: Scenario 3 proves multi-session conflict resolution, mathematical priority aggregation, and tie-breaking.

---

## Phase 6: Scenario 4 — Process Crash & Liveness Hysteresis Detection [US4] [T070]

**Goal**: Automate Quickstart Scenario 4, verifying real `/proc` discovery of a mock worker, abrupt `SIGKILL` process termination, and 2-check failure hysteresis detection within 5.0 seconds max.

**Independent Test**: Execute `cargo test --test scenario4_crash_test`, asserting killed mock worker is detected within 2.0s–4.0s (bounded by 5.0s max) and transitions to `ERROR`.

- [x] T163 [US4] [T070] Implement Quickstart Scenario 4 in `tests/e2e/scenario4_crash_test.rs`:
  - Start daemon with `E2eTestFixture`.
  - Spawn mock worker: `watchai-mock worker run --provider claude-code --project /tmp/crash-test`; capture worker PID, verify daemon discovers process via `/proc` within 2.0s, and query `GetSessions()` over D-Bus to resolve derived session ID.
  - Transition session to `WORKING` via `watchai-mock session transition --id <session_id> --state Working`.
  - Terminate worker abruptly with `SIGKILL` (`kill -9 $WORKER_PID`).
  - Poll D-Bus with a 5.0s bounded timeout; assert daemon's background liveness loop detects missing PID over 2 consecutive checks (2.0s–4.0s) and transitions session to `ERROR`.
  - Assert D-Bus emits `SessionUpdated` and `AggregateStateChanged('ERROR', 1, 0, 1, ...)`.
  - Assert daemon remains fully responsive to D-Bus method calls (`GetAggregateState`, `GetSessions`) after process crash (depends on T156, T159).
  *Traceability*: FR-018, FR-019; Acceptance Scenarios 4.1, 4.2, 4.3, 4.4; CHK-SCEN-012, CHK-SCEN-013, CHK-SCEN-014; CHK-TIME-004.
  *Verification*: `cargo test --test scenario4_crash_test`.

**Checkpoint**: Scenario 4 proves ungraceful crash detection via real `/proc` liveness monitoring and hysteresis.

---

## Phase 7: Developer Documentation, Verification & Governance Polish

**Purpose**: Document E2E test execution, run formatting and clippy gates, verify full test suite non-regression, and synchronize canonical task lists.

- [x] T164 [P] Create developer documentation in `docs/e2e-testing.md` explaining:
  - Architecture of the mock verification harness and `WATCHAI_TEST_SOCKET` protocol.
  - Command-line syntax and usage for `watchai-mock` (`session` and `worker` commands).
  - Instructions for running the E2E integration test suite locally (`cargo test --test 'scenario*'`).
  - Troubleshooting tips and reading failure diagnostics from captured daemon stderr.
  *Traceability*: CHK-FEAS-005.
  *Verification*: Documentation review.

- [x] T165 [P] Run Rust code formatting and clippy static analysis across the entire workspace:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  *Traceability*: Code Quality Invariant.
  *Verification*: Exit code 0 with zero warnings.

- [x] T166 Run all four automated E2E integration test suites:
  - `cargo test --test scenario1_startup_test` (T160)
  - `cargo test --test scenario2_lifecycle_test` (T161)
  - `cargo test --test scenario3_aggregation_test` (T162)
  - `cargo test --test scenario4_crash_test` (T163).
  *Traceability*: CHK-CI-002; SC-001, SC-002, SC-003.
  *Verification*: All 4 integration test binaries exit with code 0.

- [x] T167 [P] Execute full regression verification across all existing tests:
  - All 68 Rust unit and integration tests (`cargo test --workspace`).
  - All 6 GNOME Shell extension GJS test suites (`gjs -m extension/tests/...`).
  - GSettings schema validation (`glib-compile-schemas --strict --dry-run extension/schemas/`).
  *Traceability*: CHK-REG-004; SC-006.
  *Verification*: 100% pass across all existing test suites.

- [x] T168 Verify governance and architectural invariants:
  - Confirm task `T022` in `specs/001-agent-status-indicator/tasks.md:74` remains strictly discovery-gated and unchecked `[ ]`.
  - Confirm D-Bus wire protocol `(sssssssus)` and interface `org.freedesktop.WatchAI` are 100% unchanged.
  - Confirm daemon core lifecycle and completion dwell semantics remain 100% unchanged.
  - Confirm `watchai-daemon` creates zero test sockets when `WATCHAI_TEST_SOCKET` is unset.
  *Traceability*: CHK-REG-001, CHK-REG-002, CHK-REG-003; CHK-SOCK-002.
  *Verification*: Git diff inspection and verification logs.

- [x] T169 Synchronize canonical tasks `T066`, `T067`, `T068`, `T069`, and `T070` in `specs/001-agent-status-indicator/tasks.md` to completed `[x]` ONLY after all verification gates (T165, T166, T167, T168) pass cleanly.
  *Traceability*: CHK-REQ-001 through CHK-REQ-005.
  *Verification*: Git diff inspection of `specs/001-agent-status-indicator/tasks.md`.

---

## Phase 11 Task Dependency & Execution Flow

```
T147 (SessionRegistered variant) ──► T148 (process_lifecycle_event outcome) ──► T149 (event_tests)
                                                  │
                                                  ▼
                                      T150 (test_socket.rs listener) ─────────► T152 (test_socket tests)
                                                  │
                                                  ▼
                                      T151 (main.rs socket spawn & drain)
                                                  │
         ┌────────────────────────────────────────┴────────────────────────────────────────┐
         ▼                                                                                 ▼
T153 (watchai-mock Cargo.toml)                                                   T158 (tests/Cargo.toml)
         │                                                                                 │
         ▼                                                                                 ▼
T154 (cli.rs parser)                                                             T159 (harness.rs fixture)
         │                                                                                 │
         ├────────────────────────────────────────┐                                        │
         ▼                                        ▼                                        │
T155 (session subcommands)              T156 (worker subcommands)                          │
         │                                        │                                        │
         └───────────────────┬────────────────────┘                                        │
                             ▼                                                             │
                    T157 (mock_cli_tests)                                                  │
                             │                                                             │
                             └─────────────────────────────┬───────────────────────────────┘
                                                           ▼
                                              T160 (Scenario 1: Startup) [T067]
                                                           │
                                                           ├───────────────────────────────┐
                                                           ▼                               ▼
                                              T161 (Scenario 2: Lifecycle) [T068]   T162 (Scenario 3: Aggregation) [T069]
                                                           │                               │
                                                           ├───────────────────────────────┘
                                                           ▼
                                              T163 (Scenario 4: Crash) [T070]
                                                           │
                                                           ├───────────────────────────────┐
                                                           ▼                               ▼
                                              T164 (docs/e2e-testing.md)           T165 (fmt & clippy)
                                                           │                               │
                                                           ├───────────────────────────────┘
                                                           ▼
                                              T166 (Run 4 E2E Test Suites)
                                                           │
                                                           ▼
                                              T167 (Run Full Regression Baseline)
                                                           │
                                                           ▼
                                              T168 (Governance Invariant Checks)
                                                           │
                                                           ▼
                                              T169 (Synchronize Canonical Tasks)
```
