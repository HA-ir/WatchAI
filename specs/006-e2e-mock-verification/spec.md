# Feature Specification: Phase 11 — End-to-End Test Suite & Mock Verification Harness

**Feature Directory**: `specs/006-e2e-mock-verification`

**Created**: 2026-10-06

**Status**: Clarified (Ready for Planning)

**Input**: User description: "Phase 11 — End-to-End Test Suite & Mock Verification Harness"

---

## 1. Overview & Context

WatchAI has established a multi-provider daemon architecture, deterministic multi-session aggregation, process liveness monitoring, crash recovery, systemd user service integration, and desktop GSettings/notification integration (Phases 1–10).

Phase 11 delivers the **authoritative End-to-End Test Suite & Mock Verification Harness** (Tasks `T066`–`T070` from `specs/001-agent-status-indicator/tasks.md`). It establishes:
1. **`watchai-mock` CLI Binary (`T066`)**: A deterministic, scriptable command-line simulator capable of spawning synthetic agent processes, injecting validated lifecycle events, and simulating process crashes without requiring proprietary AI coding agent binaries (Claude Code, OpenAI Codex, OpenCode).
2. **Automated E2E Test Suite (`T067`–`T070`)**: Automated integration tests in `tests/e2e/` executing against the live daemon and D-Bus IPC service across all scenarios defined in `specs/001-agent-status-indicator/quickstart.md`:
   - `T067` (Scenario 1): Daemon startup, readiness synchronization (`READY=1`), and D-Bus registration.
   - `T068` (Scenario 2): Mock session lifecycle progression (`STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS` $\rightarrow$ 10s completion dwell $\rightarrow$ `IDLE`).
   - `T069` (Scenario 3): Multi-session priority conflict resolution ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$) and contextual tie-breaking.
   - `T070` (Scenario 4): Ungraceful process crash detection via `/proc` liveness failure hysteresis within 5 seconds.

---

## 2. User Scenarios & Testing *(mandatory)*

### User Story 1 - Daemon Startup & D-Bus Registration Check [Scenario 1] (Priority: P1) 🎯 MVP

As a WatchAI core developer or CI automation pipeline, I want to programmatically start the daemon in an isolated D-Bus environment and assert its readiness and interface registration so that I can verify that service bootstrap operates reliably on any Linux desktop without manual intervention.

**Why this priority**: Foundational prerequisite for all other end-to-end scenarios. If daemon startup or D-Bus registration fails or hangs, no downstream monitoring, extension client, or telemetry ingestion can function.

**Independent Test**: Can be tested independently by launching the daemon binary under an isolated ephemeral D-Bus session bus, asserting that the daemon claims `org.freedesktop.WatchAI` within 2.0 seconds, responds to `GetAggregateState()`, and cleanly terminates upon receiving `SIGTERM`.

**Acceptance Scenarios**:
1. **Given** an isolated D-Bus session bus, **When** `watchai-daemon` is launched, **Then** it claims bus name `org.freedesktop.WatchAI` at object path `/org/freedesktop/WatchAI`.
2. **Given** the daemon is running, **When** a D-Bus client invokes `GetAggregateState()`, **Then** the daemon returns state `'IDLE'` with zero active, waiting, and error session counts `('IDLE', 0, 0, 0, '<ISO_TIMESTAMP>')`.
3. **Given** the daemon is running, **When** a client invokes `GetSessions()`, **Then** it returns an empty array of sessions with zero errors.
4. **Given** the daemon is running, **When** `SIGTERM` is sent to the daemon process, **Then** the daemon cleanly releases `org.freedesktop.WatchAI` (emitting `NameOwnerChanged`), drains workers, and exits with code 0 within 5.0 seconds.

---

### User Story 2 - Full Mock Session Lifecycle & Dwell Progression [Scenario 2] (Priority: P2)

As a developer verifying agent tracking, I want to drive a mock session through complete lifecycle progression (`STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS` $\rightarrow$ `IDLE`) using `watchai-mock` and assert D-Bus signal emissions so that I can guarantee that state machine transitions, priority updates, and the 10-second completion dwell window operate correctly end-to-end.

**Why this priority**: Proves the complete unidirectional event flow from mock event emission, through daemon FSM processing and aggregation, to client-facing D-Bus signal dispatch.

**Independent Test**: Can be tested independently by using `watchai-mock` to step a session through all states, listening to the D-Bus signal stream with `zbus`, and asserting that `SessionAdded`, `SessionUpdated`, and `AggregateStateChanged` signals match the expected states and dwell timers.

**Acceptance Scenarios**:
1. **Given** the daemon is running with an active test event socket (`WATCHAI_TEST_SOCKET`), **When** `watchai-mock session start` registers a session, **Then** the daemon registers the session in `STARTING` and emits `SessionAdded` and `AggregateStateChanged('STARTING', 1, 0, 0, ...)`.
2. **Given** a registered mock session in `STARTING`, **When** `watchai-mock session transition --state WORKING` is executed, **Then** the daemon transitions the session to `WORKING`, updates aggregate state to `WORKING` with `active_session_count = 1`, and emits `SessionUpdated` and `AggregateStateChanged`.
3. **Given** the session is in `WORKING`, **When** `watchai-mock session transition --state WAITING --tool Bash` is executed, **Then** aggregate state updates to `WAITING` with `waiting_session_count = 1` and `SessionDto.active_tool_category = 'Bash'`.
4. **Given** the session is in `WAITING`, **When** `watchai-mock session transition --state SUCCESS` is executed, **Then** aggregate state updates to `SUCCESS` and enters the 10-second completion dwell window.
5. **Given** the session is in `SUCCESS` completion dwell, **When** the full production 10-second completion dwell elapses ($T \approx 10\text{s}$), **Then** the daemon's dwell evaluator resets aggregate state smoothly to `IDLE` with `active_session_count = 0` and emits `AggregateStateChanged('IDLE', 0, 0, 0, '...')`.

---

### User Story 3 - Multi-Session Priority Aggregation & Conflict Resolution [Scenario 3] (Priority: P3)

As a developer managing multiple background coding agents across repositories, I want WatchAI to resolve conflicting lifecycle states across multiple concurrent sessions according to the mathematical priority hierarchy ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$) so that the top-bar indicator always alerts me to the most urgent blocking state.

**Why this priority**: Validates multi-session concurrency, conflict resolution, active counter accuracy, and contextual tie-breaking across diverse providers and workspaces.

**Independent Test**: Can be tested independently by launching two or more concurrent mock sessions with conflicting states (`WORKING` vs `WAITING` vs `ERROR`), asserting that the higher-priority state strictly governs aggregate state, and verifying contextual tie-breaking recency.

**Acceptance Scenarios**:
1. **Given** Session 1 is in `WORKING` and Session 2 is in `WAITING`, **When** `GetAggregateState()` is queried, **Then** aggregate state is `WAITING` with `active_session_count = 2`, `waiting_session_count = 1`, `error_session_count = 0`.
2. **Given** Session 1 is in `WORKING` and Session 2 is in `WAITING`, **When** Session 1 transitions to `ERROR`, **Then** aggregate state immediately transitions to `ERROR` (`ERROR` overrides `WAITING`), with `error_session_count = 1`.
3. **Given** two concurrent sessions are both in `ERROR`, **When** contextual single-session inspection is performed, **Then** the session with the most recent `state_entered_at` timestamp is selected, broken lexicographically by `session_id`.
4. **Given** multiple active sessions, **When** all sessions reach terminal states (`SUCCESS` / `CANCELLED`), **Then** each session completes its 10-second completion dwell before aggregate state smoothly resets to `IDLE`.

---

### User Story 4 - Process Crash & Liveness Failure Detection [Scenario 4] (Priority: P4)

As a user running AI coding agents in terminal tabs, I want WatchAI to detect when an agent process is killed or crashes abruptly (`kill -9`) within 5 seconds without user intervention so that I am never left with stale or frozen `WORKING` status indicators.

**Why this priority**: Critical reliability requirement (Constitution Principle III). Guarantees that unmonitored crashes or terminal exits are detected via `/proc` liveness polling and 2-check failure hysteresis without false positives.

**Independent Test**: Can be tested independently by spawning a mock worker process assigned to a `WORKING` session, killing the process with `SIGKILL`, and asserting that the daemon confirms process death and transitions the session to `ERROR` within 2.0 to 4.0 seconds (bounded strictly by 5.0 seconds).

**Acceptance Scenarios**:
1. **Given** a mock worker process with PID $P$ registered in state `WORKING`, **When** process $P$ is abruptly killed with `SIGKILL` (`kill -9`), **Then** the daemon detects process disappearance within 2 consecutive checks (2.0–4.0 seconds).
2. **Given** process $P$ has died, **When** process death is confirmed, **Then** the session transitions to `ERROR` with `adapter_status = Active`, and the daemon emits `SessionUpdated` and `AggregateStateChanged('ERROR', ...)`.
3. **Given** process $P$ dies while another session is actively `WORKING`, **When** process $P$ crashes into `ERROR`, **Then** the aggregate state transitions to `ERROR` (`ERROR` > `WORKING`).
4. **Given** a transient `/proc` read error (single check failure), **When** the next check succeeds, **Then** the failure counter resets to 0 and NO premature `ERROR` transition is triggered.

---

### Edge Cases

- **D-Bus Bus Name Collision during Test Runs**: Two tests running in parallel attempting to claim `org.freedesktop.WatchAI` on the host session bus.
  - *Resolution*: E2E test harness executes each test under a dedicated ephemeral D-Bus session bus (`dbus-daemon --session --print-address` wrapped in `ChildGuard`) to guarantee 100% bus isolation.
- **Leaked Mock Processes or Orphaned Daemons**: A test panics mid-run before reaching shutdown assertions.
  - *Resolution*: RAII child process guard (`ChildGuard`) automatically sends `SIGTERM` and `SIGKILL` to daemon and mock processes upon `Drop`.
- **Clock Skew & Timestamp Drift in Timing Tests**: System clock adjustments occurring during dwell or hysteresis tests.
  - *Resolution*: Monotonic timing assertions evaluate bounded duration intervals ($\le 5.0\text{s}$ for crash detection, $10\text{s} \pm 2\text{s}$ for completion dwell) rather than wall-clock equality.
- **Portability on Headless Linux (CI Environments without Graphical Displays)**:
  - *Resolution*: Entire E2E test suite executes headlessly using standard Rust `zbus` clients and ephemeral `dbus-daemon` instances with zero Mutter, X11, or Wayland dependencies.

---

## 3. Requirements *(mandatory)*

### Functional Requirements

#### `watchai-mock` CLI Binary (T066)
- **FR-001**: System MUST provide an executable binary `watchai-mock` in `crates/watchai-mock`.
- **FR-002**: `watchai-mock` MUST support command-line argument parsing using standard flags and subcommands:
  - `session start --id <id> --provider <provider> --project <path> [--state <state>] [--pid <pid>]`
  - `session transition --id <id> --state <state> [--tool <category>]`
  - `session heartbeat --id <id>`
  - `session terminate --id <id> [--exit-code <code>]`
  - `worker run [--provider <provider>] [--project <path>] [--hold-seconds <secs>]`
  - `worker kill --pid <pid> [--signal <sig>]`
- **FR-003**: `worker run` MUST spawn or execute a lightweight background process with process arguments and command line formatted to match real provider process signatures (`claude`, `codex`, `opencode`) so that the daemon's `ProcessScanner` discovers it via real `/proc` inspection.
- **FR-004**: `session transition` MUST validate the requested target state against the canonical 8-state FSM (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`).
- **FR-005**: Mock event injection into the daemon MUST operate via a dedicated local Unix-domain stream socket configured via the environment variable `WATCHAI_TEST_SOCKET`:
  - When `WATCHAI_TEST_SOCKET` is set, `watchai-daemon` MUST bind an unprivileged Unix-domain stream socket at the specified path and forward deserialized `SessionLifecycleEvent` messages directly into its existing asynchronous `event_tx` ingestion channel.
  - When `WATCHAI_TEST_SOCKET` is unset (normal production and systemd execution), the daemon MUST NOT bind or listen to any test socket, ensuring 100% production purity.
  - `SessionLifecycleEvent` MUST support `SessionRegistered { session_id, provider_id, provider_display_name, project_path, process_id, initial_state, timestamp }` allowing `watchai-mock session start` to register mock sessions deterministically.
- **FR-006**: `watchai-mock` MUST enforce Zero-Leakage Privacy: mock session payloads MUST NEVER contain prompt text, tool arguments, source code diffs, tokens, or credentials.

#### E2E Test Suite & Test Automation Harness (T067–T070)
- **FR-007**: System MUST provide automated E2E test suites in `tests/e2e/`:
  - `tests/e2e/scenario1_startup_test.rs` (Automating Quickstart Scenario 1 - T067)
  - `tests/e2e/scenario2_lifecycle_test.rs` (Automating Quickstart Scenario 2 - T068)
  - `tests/e2e/scenario3_aggregation_test.rs` (Automating Quickstart Scenario 3 - T069)
  - `tests/e2e/scenario4_crash_detection_test.rs` (Automating Quickstart Scenario 4 - T070)
- **FR-008**: The E2E test harness MUST manage the daemon process lifecycle programmatically, starting `watchai-daemon` as an unprivileged child process and terminating it via `SIGTERM` / `SIGINT`.
- **FR-009**: The test harness MUST isolate each test execution under an independent D-Bus session bus (spawning an ephemeral `dbus-daemon --session --print-address` instance with address captured) to prevent collisions with host desktop sessions or concurrent test runners.
- **FR-010**: `scenario1_startup_test.rs` MUST assert that `watchai-daemon` claims `org.freedesktop.WatchAI` within 2.0 seconds of launch and returns `'IDLE'` state with zero sessions.
- **FR-011**: `scenario1_startup_test.rs` MUST assert that `watchai-daemon` releases its D-Bus bus name upon receiving `SIGTERM` and exits cleanly within 5.0 seconds.
- **FR-012**: `scenario2_lifecycle_test.rs` MUST assert complete lifecycle progression:
  `STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS` $\rightarrow$ 10s completion dwell $\rightarrow$ `IDLE`.
- **FR-013**: In `scenario2_lifecycle_test.rs`, the completion dwell test MUST evaluate the real, production 10-second completion dwell window (`COMPLETION_DWELL_SECONDS = 10`) without artificial test overrides, polling `GetAggregateState()` with a bounded timeout (12.0 seconds) until aggregate state resets smoothly to `IDLE` at $T \approx 10\text{s}$.
- **FR-014**: `scenario2_lifecycle_test.rs` MUST verify that D-Bus signals `SessionAdded`, `SessionUpdated`, and `AggregateStateChanged` are emitted with exact matching payloads and timestamp ordering.
- **FR-015**: `scenario3_aggregation_test.rs` MUST assert the total priority hierarchy:
  $$\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$$
- **FR-016**: `scenario3_aggregation_test.rs` MUST assert that active, waiting, and error session counters accurately reflect the multi-session state.
- **FR-017**: `scenario3_aggregation_test.rs` MUST assert contextual tie-breaking recency when multiple sessions share equal maximum priority.
- **FR-018**: `scenario4_crash_detection_test.rs` MUST assert that an abruptly killed worker process (`SIGKILL`) is detected within 2 consecutive `/proc` liveness cycles (2.0–4.0 seconds, strictly bounded by 5.0 seconds).
- **FR-019**: `scenario4_crash_detection_test.rs` MUST assert that process death transitions the session into `ERROR` and updates aggregate state to `ERROR` without daemon panics or deadlocks.
- **FR-020**: All E2E test suites MUST run completely headless with zero graphical display, Mutter, or X11 dependencies.
- **FR-021**: The test harness MUST capture and print daemon stdout and stderr diagnostics upon test failure to facilitate immediate CI debugging.
- **FR-022**: The test harness MUST implement RAII process guards (`ChildGuard`) guaranteeing that child daemon and worker processes are terminated on test completion, timeout, assertion failure, or panic.

---

## 4. Key Entities & Architecture

### Mock Event Injection Protocol
- **`WATCHAI_TEST_SOCKET`**: Environment variable specifying the absolute path of an isolated Unix-domain stream socket.
- **Wire Format**: Newline-delimited JSON (`\n`) serialization of `SessionLifecycleEvent`.
- **Event Types**:
  - `SessionRegistered`: Creates or updates a session in the registry and emits `SessionAdded`.
  - `StateTransition`: Requests transition to target `LifecycleState`, emits `SessionUpdated` and `AggregateStateChanged`.
  - `Heartbeat`: Confirms liveness, updates `last_seen_at`.
  - `SessionTerminated`: Maps exit code to terminal state, triggers dwell/retention.

### E2E Test Fixture (`E2eTestFixture`)
- **`dbus_daemon`**: `ChildGuard` managing ephemeral `dbus-daemon`.
- **`daemon_child`**: `ChildGuard` managing `watchai-daemon`.
- **`session_bus_address`**: Ephemeral D-Bus session bus address (`unix:path=...`).
- **`zbus_connection`**: `zbus::Connection` to the ephemeral session bus.
- **`test_socket_path`**: Absolute path to `WATCHAI_TEST_SOCKET`.
- **`temp_dir`**: `tempfile::TempDir` auto-cleaned on drop.

---

## 5. Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: 100% of E2E test scenarios (`scenario1`–`scenario4`) pass reliably in headless automated execution via `cargo test --test '*'`.
- **SC-002**: Daemon startup and D-Bus registration in Scenario 1 completes within **2.0 seconds**.
- **SC-003**: Ungraceful crash detection in Scenario 4 confirms process death and transitions session to `ERROR` within **5.0 seconds** max.
- **SC-004**: 100% of mock payloads, CLI commands, and test assertions comply with Zero-Leakage Privacy (zero prompts, diffs, tool arguments, or credentials).
- **SC-005**: 100% of child processes (daemon, ephemeral dbus, mock workers) are cleanly reaped with zero zombie or orphaned processes remaining after test execution.
- **SC-006**: Existing unit, integration, and GJS test suites (68 Rust tests, 6 GJS suites) continue to pass with zero regressions.

---

## 6. Assumptions & Technical Constraints

- **Headless Execution**: All tests run in headless environments using standard Rust `zbus` clients and ephemeral `dbus-daemon` instances.
- **D-Bus Contract Immutability**: The public D-Bus wire contract `(sssssssus)` and interface `org.freedesktop.WatchAI` remain completely unchanged.
- **No Real AI Binaries Required**: Testing operates exclusively via synthetic processes and event simulation; no real Claude Code or Codex binaries or subscriptions are needed.
- **Linux `/proc` Requirement**: Liveness and process scanning tests require a standard Linux `/proc` filesystem.

---

## 7. Explicit Out-of-Scope Items

- **Task T022**: Claude Code opt-in hook event telemetry receiver remains strictly discovery-gated and unchecked `[ ]`.
- Testing physical GNOME Shell top-bar pixels or Mutter compositor windows (handled via unit GJS tests).
- Distribution packaging (RPM/DEB/Flatpak) (reserved for Phase 12).
- Downloading or executing real proprietary AI agent binaries.
- Cloud telemetry or network-based test reporters.
