---
description: "Actionable implementation task breakdown for WatchAI Baseline Agent Monitoring & GNOME Shell Indicator"
---

# Tasks: Baseline Agent Monitoring & GNOME Shell Indicator

**Input**: Design documents from `/specs/001-agent-status-indicator/` (`spec.md`, `plan.md`, `data-model.md`, `contracts/`, `quickstart.md`, `checklists/system-quality.md`).  
**Prerequisites**: Approved specification, constitution v1.0.0, and implementation plan.  
**Branch**: `001-agent-status-indicator`  

## Format: `- [ ] [TaskID] [P?] [Story?] Description with file path`
- **[P]**: Can run in parallel (different files, no blocking dependencies).
- **[Story]**: Maps task directly to user stories (`[US1]`, `[US2]`, `[US3]`, `[US4]`) from `spec.md`.

---

## Phase 1: Setup & Workspace Foundation

**Purpose**: Initialize the multi-crate Rust workspace, GJS extension project structure, and shared developer tooling.

- [x] T001 Initialize Cargo workspace root configuration with workspace members (`crates/watchai-core`, `crates/watchai-adapters`, `crates/watchai-ipc`, `crates/watchai-daemon`, `crates/watchai-mock`) in `Cargo.toml`.
- [x] T002 [P] Configure global repository `.gitignore` for Rust build artifacts, GJS metadata, and local socket directories in `.gitignore`.
- [x] T003 [P] Configure Rust code formatting and linting rules in `rustfmt.toml` and `clippy.toml`.
- [x] T004 [P] Initialize GNOME Shell extension package scaffold with manifest and UUID (`watchai@gnome.org`) targeting GNOME 45, 46, and 47 in `extension/metadata.json`.
- [x] T005 [P] Create initial documentation directory structure and architecture overview stub in `docs/architecture.md`.

---

## Phase 2: Foundational Core Domain & State Machine

**Purpose**: Implement pure domain models, deterministic finite-state machines, and strict sanitization rules that block all user stories.

**⚠️ CRITICAL**: No user story implementation can begin until this foundational phase passes all unit tests.

- [x] T006 Implement `LifecycleState` enum (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`) with display traits and serialization in `crates/watchai-core/src/state.rs`.
- [x] T007 Implement the deterministic FSM state transition matrix and validation logic (strictly rejecting invalid state edges) in `crates/watchai-core/src/state.rs`.
- [x] T008 [P] Implement `AgentSession` entity with sanitized metadata fields (whitelisting `session_id`, `provider_id`, `project_name`, `current_state`, `process_id`, `active_tool_category`, and prohibiting prompts, code, tokens) in `crates/watchai-core/src/session.rs`.
- [x] T009 [P] Implement thread-safe volatile in-memory session registry with monotonic event sequence validation in `crates/watchai-core/src/session.rs`.
- [x] T010 [P] Implement unit tests for all valid and invalid FSM transition edges, sequence number deduplication, and out-of-order event drops in `crates/watchai-core/tests/state_machine_tests.rs`.
- [x] T011 [P] Implement unit tests validating data sanitization invariants (ensuring prompt, code diff, and credential fields cannot be serialized) in `crates/watchai-core/tests/sanitization_tests.rs`.

**Checkpoint**: Core domain logic and FSM engine are fully unit-tested and verified.

---

## Phase 3: Foundational Local IPC Contract & D-Bus Service

**Purpose**: Implement the `org.freedesktop.WatchAI` D-Bus interface, DTOs, and asynchronous signal broadcasting.

- [x] T012 Define D-Bus Data Transfer Objects (DTOs) and struct signatures matching the XML contract in `crates/watchai-ipc/src/protocol.rs`.
- [x] T013 Implement the `zbus 4.x` D-Bus service skeleton exporting `org.freedesktop.WatchAI` interface methods (`GetAggregateState`, `GetSessions`, `GetSession`) in `crates/watchai-ipc/src/dbus_service.rs`.
- [x] T014 Implement asynchronous D-Bus signal dispatchers (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`) in `crates/watchai-ipc/src/dbus_service.rs`.
- [x] T015 [P] Implement headless D-Bus contract tests running under `dbus-run-session` to verify method responses and signal payloads in `crates/watchai-ipc/tests/dbus_contract_tests.rs`.
- [x] T016 [P] Update IPC interface documentation with introspection schemas and example `gdbus`/`busctl` calls in `docs/contracts/dbus-ipc-contract.md`.

**Checkpoint**: Local IPC contract is active and verified headlessly.

---

## Phase 4: User Story 1 - Real-Time Top-Bar Agent Status Monitoring (Priority: P1) 🎯 MVP

**Goal**: Deliver an end-to-end working MVP where a local agent session (Claude Code or mock) reflects state transitions (`IDLE` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS` / `ERROR`) in real time on the GNOME Shell top bar.

**Independent Test**: Start the daemon and GNOME extension, trigger mock/real state transitions, and observe immediate top-bar icon color and AT-SPI text label changes within 100ms without shell stutter.

### Tests for User Story 1
- [x] T017 [P] [US1] Write end-to-end integration test for US1 lifecycle progression (`IDLE` $\rightarrow$ `STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS` $\rightarrow$ `IDLE`) in `tests/us1_single_session_flow_test.rs`.
- [x] T018 [P] [US1] Write GJS unit test for extension D-Bus proxy initialization and `AggregateStateChanged` signal processing in `extension/tests/test_indicator.js`.

### Implementation for User Story 1
- [x] T019 [US1] Implement `ProviderAdapter` trait and `AdapterStatus` enum in `crates/watchai-adapters/src/traits.rs`.
- [x] T020 [US1] Implement baseline process scanner inspecting `/proc/[pid]/cmdline` and `/proc/[pid]/cwd` with deterministic session key derivation from `(PID, process_start_time, cwd)` in `crates/watchai-adapters/src/discovery.rs`.
- [x] T021 [US1] Implement Claude Code baseline process detection and workspace resolution in `crates/watchai-adapters/src/claude_code.rs`.
- [x] T022 [US1] Implement Claude Code opt-in hook event telemetry receiver in `crates/watchai-adapters/src/claude_code.rs`.
- [x] T023 [US1] Implement minimal central daemon orchestrator initializing adapters, session registry, and D-Bus server in `crates/watchai-daemon/src/main.rs`.
- [x] T024 [US1] Implement asynchronous D-Bus client proxy wrapper (`Gio.DBusProxy`) in `extension/dbus_client.js`.
- [x] T025 [US1] Implement GNOME Shell top-bar indicator button (`PanelMenu.Button`) rendering symbolic icons for all 8 states in `extension/indicator.js`.
- [x] T026 [US1] Implement high-contrast CSS styling and color accents for `WORKING` (amber), `WAITING` (red alert), `SUCCESS` (green), and `ERROR` (crimson) in `extension/stylesheet.css`.
- [x] T027 [US1] Implement AT-SPI accessible name and description bindings for the top-bar indicator in `extension/indicator.js`.
- [x] T028 [US1] Implement extension lifecycle hooks (`enable()` / `disable()`) with safe connection setup and signal cleanup in `extension/extension.js`.

**Checkpoint**: User Story 1 is fully functional as a standalone MVP!

---

## Phase 5: User Story 2 - Detailed Session Inspection via Popover Menu (Priority: P2)

**Goal**: Provide an interactive popover menu when clicking the top-bar indicator, displaying individual session cards, provider badges, project folders, elapsed durations, and terminal/process IDs.

**Independent Test**: Run two agent sessions, open the popover menu, and verify accurate display of provider names, project folder basenames, live duration counters, and terminal PIDs.

### Tests for User Story 2
- [x] T029 [P] [US2] Write unit test for `GetSessions` response serialization with active and completed sessions in `crates/watchai-ipc/tests/session_list_tests.rs`.
- [x] T030 [P] [US2] Write GJS test verifying popover card insertion, state badge updates, and empty state rendering in `extension/tests/test_popover.js`.

### Implementation for User Story 2
- [x] T031 [US2] Implement session collection query and struct packing in daemon D-Bus service in `crates/watchai-ipc/src/dbus_service.rs`.
- [x] T032 [US2] Implement session card UI widget (`PopupMenu.PopupMenuSection`) rendering provider badge, project name, duration, and state in `extension/popover.js`.
- [x] T033 [US2] Implement active duration timer updating session card elapsed time every second in `extension/popover.js`.
- [x] T034 [US2] Implement visual highlighting and prominent sorting for sessions in `WAITING` or `ERROR` in `extension/popover.js`.
- [x] T035 [US2] Implement informative empty state UI when zero sessions are active in `extension/popover.js`.
- [x] T036 [US2] Connect `SessionAdded`, `SessionUpdated`, and `SessionRemoved` D-Bus signals to dynamically mutate popover cards without closing the menu in `extension/popover.js`.

**Checkpoint**: User Story 2 is complete; sessions can be inspected individually in the GNOME panel.

---

## Phase 6: User Story 3 - Multi-Session State Aggregation & Conflict Resolution (Priority: P3)

**Goal**: Implement the deterministic state priority resolution engine ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \text{STARTING} > \text{CANCELLED} > \text{SUCCESS} > \text{UNKNOWN} > \text{IDLE}$) and 10-second completion dwell timers.

**Independent Test**: Simultaneously run conflicting sessions (e.g. Session 1 `WORKING` and Session 2 `WAITING`) and confirm the top-bar indicator deterministically displays `WAITING`.

### Tests for User Story 3
- [x] T037 [P] [US3] Write property-based unit tests for all permutations of multi-session state aggregation in `crates/watchai-core/tests/aggregation_tests.rs`.
- [x] T038 [P] [US3] Write integration test verifying 10-second dwell time for `SUCCESS` and `CANCELLED` states before settling to `IDLE` in `crates/watchai-core/tests/dwell_time_tests.rs`.

### Implementation for User Story 3
- [x] T039 [US3] Implement priority aggregation algorithm ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$) in `crates/watchai-core/src/aggregate.rs`.
- [x] T040 [US3] Implement session disambiguation logic supporting multiple concurrent sessions from the same provider across different or identical projects in `crates/watchai-core/src/session.rs`.
- [x] T041 [US3] Implement asynchronous dwell timer task (default: 10s) transitioning completed sessions to `IDLE` in `crates/watchai-core/src/aggregate.rs`.
- [x] T042 [US3] Implement retention pruning task (default: 60s) removing terminal sessions from memory and emitting `SessionRemoved` in `crates/watchai-core/src/session.rs`.
- [x] T043 [US3] Update GNOME Shell top-bar indicator to display active session count badge when multiple sessions run in `extension/indicator.js`.

**Checkpoint**: User Story 3 complete; multi-session conflict resolution operates deterministically.

---

## Phase 7: User Story 4 - Resilient Lifecycle Tracking & Crash Recovery (Priority: P4)

**Goal**: Implement process liveness tracking via `/proc`, 5-second crash detection, and seamless auto-reconnection across daemon and GNOME Shell restarts.

**Independent Test**: Kill an active agent process (`kill -9`) and verify status transitions to `ERROR` within 5 seconds; restart the daemon and verify GNOME extension auto-reconnects with exponential backoff.

### Tests for User Story 4
- [x] T044 [P] [US4] Write integration test simulating abrupt agent process exit (`kill -9`) and asserting 5-second crash transition in `tests/process_crash_recovery_test.rs`.
- [x] T045 [P] [US4] Write integration test for daemon restart and GNOME Shell proxy re-acquisition in `extension/tests/test_reconnect.js`.

### Implementation for User Story 4
- [x] T046 [US4] Implement periodic `/proc/[pid]` process existence checker (every 2 seconds) in `crates/watchai-core/src/liveness.rs`.
- [x] T047 [US4] Implement adaptive silence timeout monitor (5 min for `WORKING`, 1 min for `STARTING` when PID is unavailable) in `crates/watchai-core/src/liveness.rs`.
- [x] T048 [US4] Implement crash detection handler transitioning abruptly killed processes to `ERROR` within 5 seconds in `crates/watchai-core/src/liveness.rs`.
- [x] T049 [US4] Implement D-Bus `NameOwnerChanged` signal watcher in GNOME extension to transition UI to offline state when daemon terminates in `extension/dbus_client.js`.
- [x] T050 [US4] Implement exponential backoff reconnection loop in GNOME extension when daemon restarts in `extension/dbus_client.js`.
- [x] T051 [US4] Ensure strict cleanup of all GJS timers, signal handlers, and D-Bus proxy references in `disable()` in `extension/extension.js`.

**Checkpoint**: User Story 4 complete; system is hardened against process crashes and service restarts.

---

## Phase 8: Additional Provider Adapters & Discovery Spikes

**Purpose**: Implement OpenAI Codex CLI and OpenCode adapters following the Tiered Discovery Model with explicit discovery validation spikes and centralized `AdapterRegistry`.

- [x] T052 [P] Conduct discovery spike to audit OpenAI Codex CLI process signatures, CLI flags, and potential telemetry channels, documenting findings in `docs/discovery/codex-cli.md`.
- [x] T053 Implement OpenAI Codex CLI adapter (`codex-cli`) with `/proc` process detection and fallback `DISCOVERY_REQUIRED` badge in `crates/watchai-adapters/src/codex_cli.rs`.
- [x] T054 [P] Conduct discovery spike to audit OpenCode binary execution and event hooks, documenting findings in `docs/discovery/opencode.md`.
- [x] T055 Implement OpenCode adapter (`opencode`) with `/proc` process detection and fallback `DISCOVERY_REQUIRED` badge in `crates/watchai-adapters/src/opencode.rs`.
- [x] T056 [P] Write unit tests for Codex CLI and OpenCode adapter event normalization in `crates/watchai-adapters/tests/adapter_normalization_tests.rs`.
- [x] T057 Implement `AdapterRegistry` in `crates/watchai-adapters/src/registry.rs` and wire into daemon discovery engine in `crates/watchai-daemon/src/main.rs`.

---

## Phase 9: systemd --user Service & Desktop Lifecycle Integration

**Purpose**: Package the daemon as a first-class Linux user-space service integrated with desktop session lifecycle.

- [x] T058 Create systemd user service unit file (`Type=notify`, `BusName=org.freedesktop.WatchAI`, `Restart=on-failure`) in `systemd/watchai.service`.
- [x] T059 Implement systemd readiness and status notification (`sd_notify("READY=1")`) in daemon bootstrap in `crates/watchai-daemon/src/main.rs`.
- [x] T060 Implement graceful OS signal handling (`SIGTERM`, `SIGINT`) emitting D-Bus disconnect notices and releasing bus name in `crates/watchai-daemon/src/main.rs`.
- [x] T061 [P] Implement structured logging via `tracing` with prompt and token redaction filters in `crates/watchai-daemon/src/logging.rs`.

---

## Phase 10: Configuration, Notifications & Accessibility Polish

**Purpose**: Implement user preferences via GSettings, optional desktop notifications, and AT-SPI accessibility enhancements.

- [x] T062 Create GSettings schema definition (`org.gnome.shell.extensions.watchai.gschema.xml`) covering dwell duration, notification toggles, and icon mode in `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml`.
- [x] T063 Compile and bind GSettings preferences inside the GNOME extension in `extension/extension.js`.
- [x] T064 Implement optional desktop notifications with rate-limiting when an agent transitions into `WAITING` or `ERROR` in `extension/indicator.js`.
- [x] T065 Verify and refine AT-SPI accessible descriptions across all top-bar and popover widgets in `extension/indicator.js` and `extension/popover.js`.

---

## Phase 11: End-to-End Test Suite & Mock Verification Harness

**Purpose**: Build a dedicated simulation harness (`watchai-mock`) and automate end-to-end testing against all scenarios in `quickstart.md`.

- [x] T066 Implement `watchai-mock` CLI binary to simulate provider lifecycle events without requiring real AI agents in `crates/watchai-mock/src/main.rs`.
- [x] T067 [P] Automate Quickstart Scenario 1 (daemon startup & D-Bus registration check) in `tests/e2e/scenario1_startup_test.rs`.
- [x] T068 [P] Automate Quickstart Scenario 2 (mock session lifecycle progression) in `tests/e2e/scenario2_lifecycle_test.rs`.
- [x] T069 [P] Automate Quickstart Scenario 3 (multi-session priority conflict resolution) in `tests/e2e/scenario3_aggregation_test.rs`.
- [x] T070 [P] Automate Quickstart Scenario 4 (ungraceful process crash detection) in `tests/e2e/scenario4_crash_test.rs`.

---

## Phase 12: Documentation, Packaging & Release Readiness

**Purpose**: Finalize developer documentation, distribution build scripts with install/uninstall targets, and complete the requirements quality checklist review.

- [x] T071 Create Meson build configuration with install and clean uninstall targets (`ninja uninstall`) for compiling GSettings, installing systemd unit, and bundling GNOME extension in `meson.build`.
- [x] T072 [P] Write comprehensive user installation, configuration, and uninstallation guide in `README.md`.
- [x] T073 [P] Finalize provider adapter integration guide for third-party developers in `docs/adapter-development.md`.
- [x] T074 Perform final audit of `specs/001-agent-status-indicator/checklists/system-quality.md` confirming all 59 quality items are evaluated and marked.

---

## Dependencies & Execution Order

```text
Phase 1 (Setup) 
  └─► Phase 2 (Foundational Domain & FSM) 
        └─► Phase 3 (Foundational D-Bus IPC) 
              └─► Phase 4 (US1: Real-Time Top-Bar Indicator MVP) 🎯
                    ├─► Phase 5 (US2: Popover Inspection)
                    ├─► Phase 6 (US3: Multi-Session Priority Aggregation)
                    └─► Phase 7 (US4: Crash & Restart Resilience)
                          └─► Phase 8 (Additional Adapters & Registry)
                          └─► Phase 9 (systemd Integration)
                          └─► Phase 10 (Configuration & Notifications)
                                └─► Phase 11 (E2E Verification & Mock Harness)
                                      └─► Phase 12 (Packaging & Release)
```

### Parallel Opportunities

- **Phase 1**: T002, T003, T004, T005 can all be authored in parallel.
- **Phase 2**: T008, T009, T010, T011 can be developed concurrently once T006/T007 define the core enum.
- **Phase 3**: T015 and T016 can proceed in parallel with T013/T014.
- **User Stories (Phases 5, 6, 7)**: Once Phase 4 (MVP) is established, popover widget development (US2), aggregation math (US3), and liveness watchers (US4) can be developed concurrently across separate files.
- **Phase 8**: T052 (Codex spike) and T054 (OpenCode spike) are completely independent research tasks.
- **Phase 11**: All E2E automated test scenarios (T067–T070) can be authored in parallel.
