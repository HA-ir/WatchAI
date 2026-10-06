# Implementation Quality Checklist: Phase 11 — End-to-End Test Suite & Mock Verification Harness

**Purpose**: Implementation quality and verification gate checklist traceable to canonical tasks T066–T070, Quickstart Scenarios 1–4, and the Phase 11 implementation plan.
**Created**: 2026-10-06
**Feature**: [spec.md](../spec.md) | [plan.md](../plan.md) | [requirements.md](requirements.md)

**Review Ownership**: Reviewer-owned quality and verification gate artifact.
**Marker Semantics**:
- `[ ]` Open verification item (must be verified before Phase 11 implementation is considered complete).
- `[x]` Verified complete with automated test and/or inspection evidence.

---

## 1. Requirements Completeness & Traceability (T066–T070)

*Validates that all functional requirements map unambiguously to canonical tasks and quickstart scenarios.*

- [x] `CHK-REQ-001` Task `T066` is fully covered by `watchai-mock` CLI binary implementation supporting session lifecycle subcommands (`start`, `transition`, `heartbeat`, `terminate`) and worker simulation (`run`, `kill`). [FR-001, FR-002, FR-003, FR-004]
- [x] `CHK-REQ-002` Task `T067` is fully covered by `tests/e2e/scenario1_startup_test.rs` automating Quickstart Scenario 1. [FR-007, FR-010, FR-011]
- [x] `CHK-REQ-003` Task `T068` is fully covered by `tests/e2e/scenario2_lifecycle_test.rs` automating Quickstart Scenario 2. [FR-007, FR-012, FR-013, FR-014]
- [x] `CHK-REQ-004` Task `T069` is fully covered by `tests/e2e/scenario3_aggregation_test.rs` automating Quickstart Scenario 3. [FR-007, FR-015, FR-016, FR-017]
- [x] `CHK-REQ-005` Task `T070` is fully covered by `tests/e2e/scenario4_crash_test.rs` automating Quickstart Scenario 4. [FR-007, FR-018, FR-019]
- [x] `CHK-REQ-006` Zero hidden assumptions: Session IDs, FSM states (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`), D-Bus methods, and signals are fully defined. [Architecture Boundary]

---

## 2. Mock Event Protocol & Security (`WATCHAI_TEST_SOCKET`)

*Validates socket isolation, bounded memory, parser resilience, and production purity.*

- [x] `CHK-SOCK-001` Socket listener in `crates/watchai-daemon/src/test_socket.rs` activates strictly when `WATCHAI_TEST_SOCKET` is present in the daemon process environment. [FR-005]
- [x] `CHK-SOCK-002` When `WATCHAI_TEST_SOCKET` is unset (normal production and systemd user runs), the daemon binds zero test sockets and listens to zero extra endpoints (100% production purity). [FR-005]
- [x] `CHK-SOCK-003` Socket input buffering enforces a strict maximum line length limit (64 KiB) to prevent unbounded memory allocation or DoS from malformed newline-free streams. [Security Invariant]
- [x] `CHK-SOCK-004` Malformed or invalid JSON input lines are safely logged and discarded without crashing the daemon or dropping the socket listener. [Robustness]
- [x] `CHK-SOCK-005` Client disconnects or EOF are handled cleanly without aborting the listener task. [Robustness]
- [x] `CHK-SOCK-006` Multiple sequential and concurrent mock connections are handled gracefully without deadlocks. [Concurrency]
- [x] `CHK-SOCK-007` Test socket file is created with restricted user permissions (`0700` parent directory) and cleanly unlinked during daemon shutdown. [Security Invariant]
- [x] `CHK-SOCK-008` Validated `SessionLifecycleEvent` messages are forwarded directly into the daemon's existing internal `tokio::sync::mpsc::channel(256)` (`event_tx`). [Production Reuse]

---

## 3. End-to-End Test Authenticity

*Validates that tests exercise real production logic without fake or stub substitutes.*

- [x] `CHK-AUTH-001` E2E tests execute the real compiled production daemon binary (`env!("CARGO_BIN_EXE_watchai-daemon")`) rather than an in-process mock daemon. [Authenticity]
- [x] `CHK-AUTH-002` Tests communicate with the daemon exclusively over the real D-Bus session bus using `zbus::Connection`. [Authenticity]
- [x] `CHK-AUTH-003` Core FSM transition engine (`watchai-core::session`), session registry, priority aggregation math (`watchai-core::aggregate`), and completion dwell are the real production implementations. [Authenticity]
- [x] `CHK-AUTH-004` Zero production backdoors, test-only D-Bus methods, or fake signal emitters exist in production crates. [Production Purity]

---

## 4. D-Bus Isolation

*Validates ephemeral bus creation, multi-test isolation, and zero host contamination.*

- [x] `CHK-BUS-001` Every test scenario spawns its own ephemeral `dbus-daemon --session --print-address --nofork` instance. [FR-009]
- [x] `CHK-BUS-002` `DBUS_SESSION_BUS_ADDRESS` is set strictly for the test child process tree, guaranteeing zero interaction with the host user session bus. [FR-009]
- [x] `CHK-BUS-003` Parallel test scenarios execute under distinct ephemeral buses without bus name collisions. [Isolation]
- [x] `CHK-BUS-004` Service ownership of `org.freedesktop.WatchAI` is explicitly verified on the ephemeral bus. [Verification]
- [x] `CHK-BUS-005` Ephemeral `dbus-daemon` is cleanly terminated upon test completion or failure. [Lifecycle]

---

## 5. Process Lifecycle & RAII Cleanup (`ChildGuard`)

*Validates leak-free process management across normal completion, panics, and timeouts.*

- [x] `CHK-PROC-001` RAII `ChildGuard` wraps every spawned child process (`watchai-daemon`, `dbus-daemon`, mock workers). [FR-022]
- [x] `CHK-PROC-002` Child processes are spawned in dedicated Linux process groups (`setpgid(0, 0)`). [Process Group Isolation]
- [x] `CHK-PROC-003` `ChildGuard::drop()` sends `SIGTERM` followed by `SIGKILL` to the process group, guaranteeing that child and grandchild processes are terminated on test completion, timeout, assertion failure, or panic. [FR-022]
- [x] `CHK-PROC-004` Already-dead or exited child processes are handled gracefully without panicking during drop. [Robustness]
- [x] `CHK-PROC-005` `ChildGuard::drop()` reaps child processes via `wait()` to prevent zombie accumulation in CI environments. [Robustness]
- [x] `CHK-PROC-006` `E2eTestFixture` captures child daemon stdout and stderr in memory buffers and dumps them upon assertion failure or panic for immediate debugging. [FR-021]

---

## 6. Scenario Coverage & Acceptance Scenarios

*Validates that Scenarios 1–4 prove the Phase 11 acceptance criteria.*

- [x] `CHK-SCEN-001` **Scenario 1 (T067)**: Asserts daemon claims `org.freedesktop.WatchAI` within 2.0s of launch. [FR-010, Scenario 1.1]
- [x] `CHK-SCEN-002` **Scenario 1 (T067)**: Asserts `GetAggregateState()` returns `('IDLE', 0, 0, 0, '<ISO_TIMESTAMP>')`. [FR-010, Scenario 1.2]
- [x] `CHK-SCEN-003` **Scenario 1 (T067)**: Asserts `GetSessions()` returns an empty vector with zero errors. [Scenario 1.3]
- [x] `CHK-SCEN-004` **Scenario 1 (T067)**: Asserts sending `SIGTERM` causes the daemon to release its bus name and exit cleanly with code 0 within 5.0s. [FR-011, Scenario 1.4]
- [x] `CHK-SCEN-005` **Scenario 2 (T068)**: Asserts `watchai-mock session start` registers session and emits `SessionAdded`. [Scenario 2.1]
- [x] `CHK-SCEN-006` **Scenario 2 (T068)**: Asserts step-by-step lifecycle progression (`STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS`). [FR-012, Scenarios 2.2, 2.3, 2.4]
- [x] `CHK-SCEN-007` **Scenario 2 (T068)**: Asserts D-Bus signal stream delivers `SessionUpdated` and `AggregateStateChanged` with correct payloads and monotonic timestamps. [FR-014]
- [x] `CHK-SCEN-008` **Scenario 2 (T068)**: Asserts that after reaching `SUCCESS`, the daemon enters completion dwell and resets aggregate state smoothly to `IDLE` (`active=0`) after the real 10-second dwell window elapses. [FR-013, Scenario 2.5]
- [x] `CHK-SCEN-009` **Scenario 3 (T069)**: Asserts multi-session priority hierarchy ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$). [FR-015, Scenarios 3.1, 3.2]
- [x] `CHK-SCEN-010` **Scenario 3 (T069)**: Asserts active, waiting, and error session counters accurately reflect the multi-session state. [FR-016]
- [x] `CHK-SCEN-011` **Scenario 3 (T069)**: Asserts contextual tie-breaking recency selects the session with the most recent `state_entered_at` timestamp. [FR-017, Scenario 3.3]
- [x] `CHK-SCEN-012` **Scenario 4 (T070)**: Asserts `watchai-mock worker run` spawns a process matching provider signatures, discovered by the daemon via `/proc`. [FR-003]
- [x] `CHK-SCEN-013` **Scenario 4 (T070)**: Asserts killing the worker process (`SIGKILL` / `kill -9`) is detected by `/proc` liveness failure hysteresis within 2.0s–4.0s (bounded by 5.0s max). [FR-018, Scenario 4.1]
- [x] `CHK-SCEN-014` **Scenario 4 (T070)**: Asserts confirmed process death transitions the session into `ERROR` and updates aggregate state to `ERROR` without daemon deadlock. [FR-019, Scenario 4.2]

---

## 7. Timing, Dwell & Flakiness Mitigations

*Validates bounded timeouts, timing tolerance, and real dwell preservation.*

- [x] `CHK-TIME-001` Scenario 2 evaluates the authentic, unmodified production 10-second completion dwell window (`COMPLETION_DWELL_SECONDS = 10`) without artificial test overrides. [Architecture Invariant]
- [x] `CHK-TIME-002` Dwell testing uses bounded polling (250ms interval, 12.0s ceiling) asserting dwell expiration at elapsed $\ge 9.8\text{s}$ and $\le 12.0\text{s}$. [Flakiness Mitigation]
- [x] `CHK-TIME-003` Daemon startup uses bounded polling (50ms interval, 2.0s ceiling). [Flakiness Mitigation]
- [x] `CHK-TIME-004` Crash detection hysteresis uses bounded polling (100ms interval, 5.0s ceiling). [Flakiness Mitigation]
- [x] `CHK-TIME-005` Zero brittle fixed `tokio::time::sleep()` calls in assertion loops. [Flakiness Mitigation]

---

## 8. CI & Headless Environment Feasibility

*Validates execution in headless CI environments without graphical servers.*

- [x] `CHK-CI-001` All tests execute headlessly with zero Mutter, GNOME Shell, X11, or Wayland display dependencies. [FR-020]
- [x] `CHK-CI-002` Test suite runs via standard Cargo commands: `cargo test --test '*'`. [CI Ergonomics]
- [x] `CHK-CI-003` System dependencies (`/usr/bin/dbus-daemon`, `/proc`) are documented and validated before test execution. [Prerequisites]

---

## 9. Security & Zero-Leakage Privacy Invariants

*Validates metadata-only boundaries and unprivileged execution.*

- [x] `CHK-PRIV-001` Zero prompt text, tool arguments, source code diffs, tokens, or credentials enter the mock protocol or test assertions. [FR-006]
- [x] `CHK-PRIV-002` `watchai-mock` payloads are strictly restricted to metadata (`session_id`, `provider_id`, `project_path`, `state`, `process_id`). [Metadata Boundary]
- [x] `CHK-PRIV-003` Test socket is a local Unix-domain socket with zero network exposure. [Security Invariant]

---

## 10. Contract Stability & Governance Invariants

*Validates non-regression of IPC contracts, extension compatibility, and task governance.*

- [x] `CHK-REG-001` D-Bus wire protocol `(sssssssus)` and interface `org.freedesktop.WatchAI` remain 100% unchanged. [Contract Stability]
- [x] `CHK-REG-002` Phase 10 GNOME extension settings, desktop notifications, and accessibility behavior remain un-regressed. [Extension Stability]
- [x] `CHK-REG-003` Task `T022` in `specs/001-agent-status-indicator/tasks.md:74` remains strictly discovery-gated and unchecked `[ ]`. [Constitution Check]
- [x] `CHK-REG-004` Existing unit, integration, and GJS test suites (68 Rust tests, 6 GJS test suites) continue to pass. [Regression Baseline]

---

## 11. Implementation Feasibility & Workspace Structure

*Validates Rust workspace configuration, dependencies, and file layout.*

- [x] `CHK-FEAS-001` `crates/watchai-mock/Cargo.toml` adds `serde` and `serde_json` from workspace dependencies. [Dependencies]
- [x] `CHK-FEAS-002` `crates/watchai-core/src/session.rs` adds `SessionRegistered` variant to `SessionLifecycleEvent`. [Core Extension]
- [x] `CHK-FEAS-003` `crates/watchai-daemon/src/test_socket.rs` is cleanly exposed in `lib.rs` and spawned conditionally in `main.rs`. [Daemon Extension]
- [x] `CHK-FEAS-004` `tests/Cargo.toml` correctly registers the 4 E2E test targets. [Cargo Integration]
- [x] `CHK-FEAS-005` `docs/e2e-testing.md` is provided for developer guidance. [Documentation]

---

## Verification Dependencies & Evidence Map

```
Core Event Extension (SessionRegistered) ──► Daemon Test Socket (WATCHAI_TEST_SOCKET)
                                                      │
                                                      ├────────────────────────┐
                                                      ▼                        ▼
                                             watchai-mock (T066)       tests/e2e/harness.rs
                                                      │                        │
                                                      └───────────┬────────────┘
                                                                  ▼
                                                      Scenario 1: Startup (T067)
                                                      Scenario 2: Lifecycle (T068)
                                                      Scenario 3: Aggregation (T069)
                                                      Scenario 4: Crash (T070)
                                                                  │
                                                                  ▼
                                                      All 4 E2E Suites Pass
                                                      + 68 Rust Tests Pass
                                                      + 6 GJS Suites Pass
```
