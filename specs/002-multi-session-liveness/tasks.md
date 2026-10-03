---
description: "Actionable implementation task breakdown for WatchAI Phase 6 — Multi-Session Aggregation & Liveness"
---

# Tasks: Multi-Session Aggregation & Liveness

**Input**: Design documents from `/specs/002-multi-session-liveness/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/`, `quickstart.md`, `checklists/requirements.md`).  
**Prerequisites**: Approved specification, constitution v1.0.0, Phase 5 merged (tasks through T036).  
**Branch**: `002-multi-session-liveness`  

## Format: `- [ ] [TaskID] [P?] [Story?] Description with file path`
- **[P]**: Can run in parallel (different files, no blocking dependencies).
- **[Story]**: Maps task directly to user stories (`[US1]`, `[US2]`, `[US3]`) from `spec.md`.

---

## Phase 1: Setup & Foundational Data Model (Group A)

**Purpose**: Extend core domain entities with internal liveness tracking metadata while preserving 100% backward compatibility with public D-Bus DTO contracts.

- [X] T037 Add internal `process_start_time: Option<u64>` (jiffies since boot) and `consecutive_proc_failures: u32` fields to `AgentSession` in `crates/watchai-core/src/session.rs`.
- [X] T038 [P] Update `AgentSession::new` and constructor helpers in `crates/watchai-core/src/session.rs` to initialize start time and failure counters while ensuring `SessionDto` in `crates/watchai-ipc/src/protocol.rs` remains unchanged.
- [X] T039 [P] Update unit tests in `crates/watchai-core/tests/sanitization_tests.rs` to verify that `process_start_time` and `consecutive_proc_failures` are strictly internal and cannot leak into serialized public DTO representations.

**Checkpoint**: Foundational data model supports internal process identity and error hysteresis without public contract changes.

---

## Phase 2: User Story 1 - Deterministic Multi-Session Priority Aggregation (Groups B & G) [US1]

**Goal**: Implement the pure mathematical aggregation engine computing desktop aggregate state ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \text{STARTING} > \text{CANCELLED} > \text{SUCCESS} > \text{UNKNOWN} > \text{IDLE}$), session counters, contextual tie-breaking, and the 10-second completion dwell window.

**Independent Test**: Provide an array of sessions with conflicting states to `compute_aggregate_state()` and assert that the computed state matches the priority formula, counters are exact, and terminal states settle to `IDLE` after 10 seconds.

### Tests for User Story 1
- [X] T040 [P] [US1] Write unit tests for zero-session (`IDLE`), single-session, and all priority hierarchy permutations in `crates/watchai-core/tests/aggregation_tests.rs`.
- [X] T041 [P] [US1] Write unit tests for `active_session_count`, `waiting_session_count`, and `error_session_count` calculations in `crates/watchai-core/tests/aggregation_tests.rs`.
- [X] T042 [P] [US1] Write unit tests for contextual tie-breaking (verifying most recent `state_entered_at`, then lexicographical `session_id`) in `crates/watchai-core/tests/aggregation_tests.rs`.
- [X] T043 [P] [US1] Write unit tests for the 10-second completion dwell window (asserting aggregate state resets to `IDLE` after 10s while session remains in terminal state) in `crates/watchai-core/tests/aggregation_tests.rs`.

### Implementation for User Story 1
- [X] T044 [US1] Implement `compute_aggregate_state` in `crates/watchai-core/src/aggregate.rs` evaluating the priority hierarchy $\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$ with dwell time filtering.
- [X] T045 [US1] Implement active, waiting, and error session counter calculation logic in `crates/watchai-core/src/aggregate.rs`.
- [X] T046 [US1] Implement contextual single-session deterministic tie-breaking function in `crates/watchai-core/src/aggregate.rs`.

**Checkpoint**: User Story 1 pure aggregation logic is 100% unit-tested and verified.

---

## Phase 3: User Story 2 - Resilient Process Liveness & PID Reuse Defense (Groups C & G) [US2]

**Goal**: Implement the background liveness checker verifying process survival via `/proc/[pid]/stat`, matching `starttime` ticks to prevent PID reuse, and enforcing a 2-consecutive-check failure hysteresis to detect crashes within 5 seconds without false alarms.

**Independent Test**: Simulate process termination and mismatched PID start times in unit tests, asserting that ungraceful crashes transition to `ERROR` within 2–4 seconds (bounded by 5s max) while transient single-read errors are safely tolerated.

### Tests for User Story 2
- [X] T047 [P] [US2] Write unit tests with an injectable `/proc` reader seam for process survival, PID recycling start-time mismatch, and 2-consecutive-check failure hysteresis in `crates/watchai-core/tests/liveness_tests.rs`.
- [X] T048 [P] [US2] Write unit tests asserting that a single transient `/proc` read error does NOT trigger a false `ERROR` transition in `crates/watchai-core/tests/liveness_tests.rs`.

### Implementation for User Story 2
- [X] T049 [US2] Implement `/proc/[pid]/stat` field 22 parser and `process_start_time` tick extraction in `crates/watchai-core/src/liveness.rs`.
- [X] T050 [US2] Implement PID recycling verification comparing live `/proc` start time against session `process_start_time`, immediately treating mismatched start times as process death in `crates/watchai-core/src/liveness.rs`.
- [X] T051 [US2] Implement 2-consecutive-failure hysteresis tracking in `crates/watchai-core/src/liveness.rs`, confirming termination within 2–4 seconds (bounded by 5s max) and transitioning dead processes to `LifecycleState::Error`.
- [X] T052 [US2] Implement non-blocking background liveness check loop (2-second interval) in `crates/watchai-core/src/liveness.rs`.

**Checkpoint**: User Story 2 process liveness and PID reuse defense are fully unit-tested and verified.

---

## Phase 4: User Story 3 - Stale Session Recovery & Terminal Retention Pruning (Groups D, E & G) [US3]

**Goal**: Implement adaptive silence timeouts (transitioning unresponsive unmonitored sessions to `UNKNOWN`), telemetry-gated recovery, 60-second terminal session retention pruning, and startup discovery sweeps with deterministic session IDs.

**Independent Test**: Assert that unmonitored sessions transition to `UNKNOWN` after 300s of silence, recover to `WORKING` only upon valid telemetry, and terminal sessions are pruned from memory after exactly 60 seconds.

### Tests for User Story 3
- [X] T053 [P] [US3] Write unit tests for adaptive silence timeouts (300s for `WORKING`, 60s for `STARTING`) transitioning unmonitored sessions to `UNKNOWN` in `crates/watchai-core/tests/retention_tests.rs`.
- [X] T054 [P] [US3] Write unit tests for stale recovery (verifying telemetry transitions `UNKNOWN` $\rightarrow$ `WORKING`, while process presence alone keeps session in `IDLE`/`UNKNOWN` without claiming active work) in `crates/watchai-core/tests/retention_tests.rs`.
- [X] T055 [P] [US3] Write unit tests for 60-second terminal retention pruning (asserting session remains in `SUCCESS`/`ERROR`/`CANCELLED` for 60s before being removed) in `crates/watchai-core/tests/retention_tests.rs`.

### Implementation for User Story 3
- [X] T056 [US3] Implement adaptive silence timeout evaluation in `crates/watchai-core/src/liveness.rs` transitioning unresponsive unmonitored sessions to `UNKNOWN`.
- [X] T057 [US3] Implement telemetry-gated recovery logic in `crates/watchai-core/src/session.rs` allowing explicit telemetry events to transition sessions from `UNKNOWN` to `WORKING`.
- [X] T058 [US3] Implement 60-second terminal retention pruning worker in `crates/watchai-core/src/liveness.rs` removing expired sessions from `SessionRegistry` and returning pruned session IDs.
- [X] T059 [US3] Update `ProcessScanner::scan_processes` in `crates/watchai-adapters/src/discovery.rs` to capture `process_start_time` and assign `initial_state = LifecycleState::Idle` with `AdapterStatus::DiscoveryRequired`.
- [X] T060 [US3] Implement deterministic surrogate key calculation `SHA256(PID + process_start_time + project_path)[0..16]` on discovery sweep in `crates/watchai-adapters/src/discovery.rs`.

**Checkpoint**: User Story 3 retention pruning, stale recovery, and startup discovery sweeps are verified.

---

## Phase 5: Daemon Integration & Signal Throttling (Group F)

**Purpose**: Wire aggregation, liveness, and retention workers into the central daemon background loop, enforcing signal suppression for duplicate events.

- [X] T061 Implement immediate startup `/proc` discovery sweep in daemon initialization before event loops start in `crates/watchai-daemon/src/main.rs`.
- [X] T062 Wire `LivenessChecker` background task (2-second interval) and retention pruner into daemon runtime in `crates/watchai-daemon/src/main.rs`.
- [X] T063 Wire `compute_aggregate_state` into the daemon event loop to update `WatchAiDbusService` on any session addition, transition, or removal in `crates/watchai-daemon/src/main.rs`.
- [X] T064 Implement signal throttling in `crates/watchai-daemon/src/main.rs`, suppressing duplicate `AggregateStateChanged` emissions when state and counts remain unchanged.
- [X] T065 Connect retention pruner to emit `SessionRemoved` D-Bus signal when 60-second retention expires in `crates/watchai-daemon/src/main.rs`.
- [X] T066 Implement graceful shutdown logic ensuring background liveness and discovery tasks terminate cleanly on `SIGINT`/`SIGTERM` in `crates/watchai-daemon/src/main.rs`.

---

## Phase 6: Documentation & Validation Polish (Group H)

**Purpose**: Update architectural documentation, IPC contract guides, and execute all quickstart verification scenarios.

- [X] T067 [P] Update architecture documentation with mathematical aggregation formula, liveness checking model, and activity knowledge boundary in `docs/architecture.md`.
- [X] T068 [P] Update IPC documentation with signal emission throttling invariants and retention pruning sequences in `docs/contracts/dbus-ipc-contract.md`.
- [X] T069 Execute all runnable validation scenarios from `specs/002-multi-session-liveness/quickstart.md` (priority resolution, dwell vs retention, crash detection, PID reuse, daemon restart) and record outcomes in `specs/002-multi-session-liveness/quickstart.md`.

---

## Dependencies & Execution Order

```text
Phase 1: Foundational Data Model (T037–T039)
  └─► Phase 2: User Story 1 Aggregation (T040–T046) [US1]
  └─► Phase 3: User Story 2 Liveness & PID Reuse (T047–T052) [US2]
        └─► Phase 4: User Story 3 Retention & Recovery (T053–T060) [US3]
              └─► Phase 5: Daemon Integration & Throttling (T061–T066)
                    └─► Phase 6: Documentation & Quickstart (T067–T069)
```

### Parallel Execution Opportunities
- **Phase 1**: T038 and T039 can be authored in parallel with T037.
- **Phase 2 (US1)**: Test tasks T040, T041, T042, T043 can be authored concurrently before implementation tasks T044–T046.
- **Phase 3 (US2)**: Test tasks T047 and T048 can be authored concurrently.
- **Phase 4 (US3)**: Test tasks T053, T054, T055 can be authored concurrently.
- **Phase 6**: T067 and T068 documentation updates can run in parallel.
