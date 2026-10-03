---
description: "Actionable implementation task breakdown for WatchAI Phase 7 — Crash & Recovery"
---

# Tasks: Crash & Recovery

**Input**: Design documents from `/specs/003-crash-recovery/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`, `checklists/requirements.md`).  
**Prerequisites**: Approved specification, clarified failure decisions, constitution v1.0.0, Phase 6 merged (tasks T001–T069).  
**Branch**: `003-crash-recovery`  

## Format: `- [ ] [TaskID] [P?] [Story?] Description with file path`
- **[P]**: Can run in parallel (different files, no blocking dependencies).
- **[Story]**: Maps task directly to user stories (`[US1]`, `[US2]`, `[US3]`, `[US4]`) from `spec.md`.

---

## Phase 1: Setup & Foundational Infrastructure

**Purpose**: Harden process discovery with per-process fault isolation before recovery ordering is layered on top.

- [X] T070 [P] Implement per-process error isolation in `ProcessScanner::scan_processes` in `crates/watchai-adapters/src/discovery.rs` (wrapping `/proc/[pid]` directory reading, `cwd` inspection, and stat parsing in individual `Result` handlers so errors skip that PID with `debug!` logging without aborting directory scan).
- [X] T071 [P] Write unit tests in `crates/watchai-adapters/tests/discovery_tests.rs` verifying that unreadable stat files, non-numeric folders, missing cwd symlinks, and mid-sweep process disappearances are skipped safely while valid sibling processes are discovered.

**Checkpoint**: Process scanner isolates process faults cleanly.

---

## Phase 2: User Story 1 - Deterministic Daemon Crash Recovery & Process Rediscovery (Priority: P1) [US1]

**Goal**: Implement deterministic process reconstruction on daemon startup before D-Bus announcement, ordering sessions stably and avoiding false `ERROR` alerts.

**Independent Test**: Simulate daemon termination (`kill -9`), restart daemon, and verify surviving processes are rediscovered with identical deterministic IDs (`SHA256(PID + start_time + path)[0..16]`) in `IDLE + DISCOVERY_REQUIRED`.

### Tests for User Story 1
- [X] T072 [P] [US1] Write integration tests in `tests/crash_recovery_test.rs` for daemon restart with zero running agent processes (asserting clean start, empty registry, aggregate IDLE, and zero false signals).
- [X] T073 [P] [US1] Write integration tests in `tests/crash_recovery_test.rs` asserting surviving processes are rediscovered with identical deterministic IDs, initialized as `IDLE + DISCOVERY_REQUIRED`, and produce zero false `ERROR` emissions.
- [X] T074 [P] [US1] Write integration tests in `tests/crash_recovery_test.rs` verifying that multiple surviving processes discovered in arbitrary kernel directory order are deterministically sorted by `provider_id` ASC, then `project_path` ASC, then `process_id` ASC across both input permutations.
- [X] T075 [P] [US1] Write integration tests in `tests/crash_recovery_test.rs` verifying that processes that terminated while the daemon was offline are omitted from discovery and zero phantom sessions are resurrected.

### Implementation for User Story 1
- [X] T076 [US1] Implement deterministic sorting of discovered processes (`provider_id` ASC, then `project_path` ASC, then `process_id` ASC) during the recovery sweep in `crates/watchai-daemon/src/main.rs`.
- [X] T077 [US1] Ensure `watchai-daemon` startup recovery sweep and initial aggregate state computation strictly complete *before* `zbus` claims `org.freedesktop.WatchAI` in `crates/watchai-daemon/src/main.rs`.

**Checkpoint**: User Story 1 recovery discovery operates deterministically without phantom errors.

---

## Phase 3: User Story 2 - Resilient GNOME Shell Reconnection & Stale Client Recovery (Priority: P1) [US2]

**Goal**: Implement the GNOME Shell client connection state machine (`Disconnected`, `Connecting`, `Connected`, `Reconnecting`), 5.0-second handshake timeout, cached popover card presentation, and paused duration timers.

**Independent Test**: Disconnect daemon during open popover, assert top-bar dims to "Offline" and open cards freeze duration timers with `[CACHED]` status; restart daemon and assert automatic state synchronization without user intervention.

### Tests for User Story 2
- [X] T078 [P] [US2] Write GJS unit tests in `extension/tests/test_reconnect.js` for the client connection state machine (`Disconnected`, `Connecting`, `Connected`, `Reconnecting`).
- [X] T079 [P] [US2] Write GJS unit tests in `extension/tests/test_reconnect.js` asserting that D-Bus disconnection transitions cards to `CACHED / OFFLINE`, freezes live duration timers, and disconnects all D-Bus signal listeners.
- [X] T080 [P] [US2] Write GJS unit tests in `extension/tests/test_reconnect.js` verifying the 5-step asynchronous reconnection handshake, asserting that successful handshake replaces cached state, and asserting that an uncompleted handshake times out at exactly 5.0 seconds.
- [X] T081 [P] [US2] Write GJS unit tests in `extension/tests/test_reconnect.js` verifying lifecycle interleaving: extension starting before daemon, daemon starting before extension, and daemon disappearing/reappearing while popover is open.

### Implementation for User Story 2
- [X] T082 [US2] Implement D-Bus `NameOwnerChanged` signal monitoring for `org.freedesktop.WatchAI` with client connection state transitions in `extension/dbus_client.js`.
- [X] T083 [US2] Implement the atomic 5-step synchronization handshake (`Reacquire Proxy` $\rightarrow$ `GetAggregateState` $\rightarrow$ `GetSessions` $\rightarrow$ `Attach Signals` $\rightarrow$ `Mark Online`) with 5.0-second timeout in `extension/dbus_client.js`.
- [X] T084 [US2] Implement cached presentation mode (`setOfflineMode`) in `extension/popover.js`, preserving rendered cards as `CACHED / OFFLINE`, pausing duration timer updates, and replacing with authoritative state upon reconnect.
- [X] T085 [US2] Implement dimmed "Offline" styling and updated AT-SPI accessible description ("WatchAI daemon offline") in `extension/indicator.js`.
- [X] T086 [US2] Implement strict GLib timer and D-Bus proxy signal disconnection in `extension/dbus_client.js` and `extension/extension.js` during disable/disconnect to prevent memory leaks and duplicate subscriptions in Mutter.

**Checkpoint**: User Story 2 client reconnection and cached state presentation operate without compositor stalls.

---

## Phase 4: User Story 3 - Crash Loop Defense & Failure Isolation (Priority: P2) [US3]

**Goal**: Implement jittered exponential backoff (1.0s initial, doubling each failure up to 30.0s ceiling with $\pm 20\%$ jitter) for client reconnection and rate-limited logging in the daemon to prevent D-Bus storms and log saturation during crash loops.

**Independent Test**: Simulate rapid repeated daemon crashes, asserting that client retry intervals double exponentially up to 30s with $\pm 20\%$ jitter, reset to 1.0s on success, and emit zero desktop notification popups.

### Tests for User Story 3
- [X] T087 [P] [US3] Write unit tests in `extension/tests/test_reconnect.js` verifying exponential backoff calculations (1.0s, 2.0s, 4.0s ... 30.0s max) with $\pm 20\%$ jitter bounds and asserting successful handshake resets backoff to 1.0s.

### Implementation for User Story 3
- [X] T088 [US3] Implement jittered exponential backoff retry scheduler in `extension/dbus_client.js`, enforcing 1.0s initial delay, 2.0x multiplier, 30.0s ceiling, $\pm 20\%$ jitter, and reset on successful synchronization without desktop notifications.
- [X] T089 [US3] Implement rate-limited error logging in `crates/watchai-daemon/src/main.rs` to prevent log saturation during persistent unreadable `/proc` entries.
- [X] T090 [US3] Add defensive error catching around incoming D-Bus message unpacking in `extension/dbus_client.js` to ensure malformed payloads never crash GNOME Shell.

**Checkpoint**: User Story 3 crash loop defenses prevent bus flooding and desktop notifications.

---

## Phase 5: User Story 4 - In-Flight Telemetry Continuity & Activity Boundary (Priority: P3) [US4]

**Goal**: Preserve the Activity Knowledge Boundary after restart, ensuring recovered sessions transition to `WORKING` only upon receipt of verified fresh telemetry without requiring process restarts.

**Independent Test**: Inject telemetry event into an `IDLE` recovered session, asserting it transitions to `WORKING`, while verifying process presence alone never promotes `IDLE` to `WORKING`.

### Tests for User Story 4
- [X] T091 [P] [US4] Write integration tests in `tests/crash_recovery_test.rs` asserting that a surviving process recovered in `IDLE` (`DISCOVERY_REQUIRED`) transitions to `WORKING` only upon receipt of fresh verified telemetry, while `/proc` presence alone never promotes `IDLE` to `WORKING`.

### Implementation for User Story 4
- [X] T092 [US4] Verify telemetry event routing in `crates/watchai-daemon/src/main.rs` and `crates/watchai-core/src/session.rs` so recovered sessions seamlessly receive post-restart telemetry and transition to `WORKING`.

**Checkpoint**: User Story 4 in-flight telemetry recovery respects the Activity Knowledge Boundary.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Update architectural documentation, IPC contract guides, and execute quickstart validation scenarios.

- [X] T093 [P] Update architecture documentation with crash recovery discovery flow, GJS reconnection state machine, and backoff formulas in `docs/architecture.md`.
- [X] T094 [P] Update IPC contract documentation with `NameOwnerChanged` protocol and 5-step handshake sequence in `docs/contracts/dbus-ipc-contract.md`.
- [X] T095 Execute all 5 validation scenarios in `specs/003-crash-recovery/quickstart.md` and record outcomes.

---

## Dependencies & Execution Order

```text
Phase 1: Foundational Infrastructure (T070–T071)
  └─► Phase 2: User Story 1 Daemon Recovery (T072–T077) [US1]
  └─► Phase 3: User Story 2 Extension Reconnect (T078–T086) [US2]
        └─► Phase 4: User Story 3 Crash Loop Defense (T087–T090) [US3]
        └─► Phase 5: User Story 4 Telemetry Continuity (T091–T092) [US4]
              └─► Phase 6: Polish & Documentation (T093–T095)
```

### Parallel Execution Opportunities
- **Phase 1**: `T070` and `T071` can be authored in parallel.
- **Phase 2 (US1)**: Test tasks `T072`, `T073`, `T074`, and `T075` can be written concurrently before implementation tasks `T076`–`T077`.
- **Phase 3 (US2)**: Test tasks `T078`, `T079`, `T080`, and `T081` can be developed concurrently before GJS tasks `T082`–`T086`.
- **Phase 4 (US3)**: Test task `T087` can run in parallel with daemon logging task `T089`.
- **Phase 6**: Documentation updates `T093` and `T094` can run in parallel.
