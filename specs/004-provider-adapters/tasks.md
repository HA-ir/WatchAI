---
description: "Actionable implementation task breakdown for WatchAI Phase 8 — Multi-Provider Discovery & Capability Modeling"
---

# Tasks: Multi-Provider Discovery & Capability Modeling

**Input**: Design documents from `/specs/004-provider-adapters/` (`spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/provider-adapter-contract.md`, `quickstart.md`).  
**Prerequisites**: Approved specification, clarified failure decisions, constitution v1.0.0, Phase 7 merged (tasks T001–T095).  
**Branch**: `004-provider-adapters`  

## Format: `- [ ] [TaskID] [P?] [Story?] Description with file path`
- **[P]**: Can run in parallel (different files, no blocking dependencies).
- **[Story]**: Maps task directly to user stories (`[US1]`, `[US2]`, `[US3]`, `[US4]`) from `spec.md`.

---

## Phase 1: Setup & Domain Infrastructure (Foundational)

**Purpose**: Establish core domain capability types, event models, and generalized multi-pattern process scanning before concrete provider adapters are added.

- [X] T096 [P] Implement `TelemetryTier` enum (`ProcessDiscoveryOnly`, `PassiveLogTailing`, `OptInHookTelemetry`), `ProviderCapabilities` struct (`telemetry_tier`, `supports_tool_categories`, `supports_activity_events`), and extend `ProviderAdapter` trait with default `fn capabilities(&self) -> ProviderCapabilities` in `crates/watchai-adapters/src/traits.rs`.
- [X] T097 [P] Define `SessionLifecycleEvent` enum (`StateTransition`, `Heartbeat`, `SessionTerminated`) with helper methods (`is_critical()`, `session_id()`, `timestamp()`) in `crates/watchai-core/src/session.rs`.
- [X] T098 Implement `apply_lifecycle_event` on `AgentSession` in `crates/watchai-core/src/session.rs` (depends on T097), validating FSM transition legality via `can_transition_to()`, enforcing timestamp monotonicity against `state_entered_at`, and incrementing internal `sequence_number`.
- [X] T099 [P] Generalize `ProcessScanner::scan_proc_dir` in `crates/watchai-adapters/src/discovery.rs` to parse null-delimited `/proc/[pid]/cmdline` arguments, supporting argv[0]/argv[1] matching (direct binary and script runners) and explicit rejection of utilities (`grep`, `rg`, `sh`, `bash`, `zsh`, `git`, `cargo`, `ps`, `which`) while preserving positive start-time validation (`start_time > 0`) and per-process fault isolation.

**Checkpoint**: Foundational types compile and process scanner supports generalized, false-positive-safe multi-pattern matching.

---

## Phase 2: User Story 1 - Multi-Provider Process Discovery (Priority: P1) [US1] 🎯 MVP

**Goal**: Implement and register non-invasive `/proc` discovery adapters for OpenAI Codex CLI and OpenCode alongside Claude Code, deriving stable surrogate session IDs.

**Independent Test**: Run mock processes for `claude`, `codex`, and `opencode` across separate workspaces; assert that `AdapterRegistry::discover_all()` discovers all three with correct provider IDs (`claude-code`, `codex-cli`, `opencode`) and display names.

### Tests for User Story 1
- [X] T100 [P] [US1] Write unit tests in `crates/watchai-adapters/tests/provider_tests.rs` for process command-line matching (testing direct binaries `codex` / `opencode`, script runners `node .../codex` / `python .../opencode`, and asserting rejection of `grep codex`, `bash -c`, and child worker processes like `git`).
- [X] T104 [P] [US1] Write integration tests in `tests/multi_provider_test.rs` asserting concurrent discovery of mock Claude, Codex, and OpenCode processes, verifying stable surrogate session ID generation across restarts.

### Implementation for User Story 1
- [X] T101 [P] [US1] Implement `CodexCliAdapter` in `crates/watchai-adapters/src/codex_cli.rs` implementing `ProviderAdapter` (`provider_id = "codex-cli"`, `display_name = "OpenAI Codex"`, `capabilities()`, `check_environment()`, `discover_sessions()`).
- [X] T102 [P] [US1] Implement `OpenCodeAdapter` in `crates/watchai-adapters/src/opencode.rs` implementing `ProviderAdapter` (`provider_id = "opencode"`, `display_name = "OpenCode"`, `capabilities()`, `check_environment()`, `discover_sessions()`).
- [X] T103 [US1] Update `AdapterRegistry::default_registry()` in `crates/watchai-adapters/src/registry.rs` to register `ClaudeCodeAdapter`, `CodexCliAdapter`, and `OpenCodeAdapter` (depends on T101, T102).

**Checkpoint**: User Story 1 delivers MVP multi-provider process discovery across Claude, Codex, and OpenCode.

---

## Phase 3: User Story 2 - Provider Capability Modeling (Priority: P2) [US2]

**Goal**: Formalize capability reporting across all provider adapters, enforcing the Activity Knowledge Boundary so discovery-only providers strictly initialize in `IDLE + DISCOVERY_REQUIRED`.

**Independent Test**: Query `adapter.capabilities()` on each registered adapter; assert `telemetry_tier = ProcessDiscoveryOnly` and verify `/proc` discovery alone never promotes sessions to `WORKING` or `WAITING`.

### Tests for User Story 2
- [X] T109 [P] [US2] Write unit tests in `crates/watchai-adapters/tests/provider_tests.rs` verifying capability descriptors across all three registered adapters (`telemetry_tier = ProcessDiscoveryOnly`, `supports_tool_categories = false`, `supports_activity_events = false`).
- [X] T110 [P] [US2] Write integration tests in `tests/multi_provider_test.rs` asserting the Activity Knowledge Boundary across providers: discovered sessions strictly initialize in `IDLE` (`DISCOVERY_REQUIRED`) and process presence alone never promotes sessions to `WORKING` or `WAITING`.

### Implementation for User Story 2
- [X] T106 [P] [US2] Explicitly implement `capabilities()` on `ClaudeCodeAdapter` in `crates/watchai-adapters/src/claude_code.rs` returning `ProviderCapabilities::process_discovery_only()`.
- [X] T107 [P] [US2] Verify `capabilities()` on `CodexCliAdapter` in `crates/watchai-adapters/src/codex_cli.rs` returns `ProviderCapabilities::process_discovery_only()`.
- [X] T108 [P] [US2] Verify `capabilities()` on `OpenCodeAdapter` in `crates/watchai-adapters/src/opencode.rs` returns `ProviderCapabilities::process_discovery_only()`.

**Checkpoint**: User Story 2 establishes explicit capability boundaries for all registered adapters.

---

## Phase 4: User Story 3 - Provider Environment Health & Diagnostics (Priority: P3) [US3]

**Goal**: Implement startup environment checks probing `$PATH` for provider binaries and report adapter status cleanly in daemon logs without background polling.

**Independent Test**: Assert that `check_environment()` returns `AdapterStatus::Active` when the provider binary exists in PATH, and `AdapterStatus::DiscoveryRequired` when absent, with structured debug logging.

### Tests for User Story 3
- [X] T114 [P] [US3] Write unit tests in `crates/watchai-adapters/tests/provider_tests.rs` verifying `check_environment()` returns `Active` when binary is in PATH and `DiscoveryRequired` when absent.

### Implementation for User Story 3
- [X] T113 [US3] Add startup environment probing loop in `crates/watchai-daemon/src/main.rs` evaluating `check_environment()` once across all registered adapters during daemon startup without background polling.

**Checkpoint**: User Story 3 provides clear startup diagnostics for installed vs missing agent CLIs.

---

## Phase 5: User Story 4 - Standardized Provider Lifecycle Event Ingestion Seam (Priority: P4) [US4]

**Goal**: Establish a bounded, prioritized asynchronous MPSC event channel between adapters and the daemon engine, ensuring critical state transitions are guaranteed delivery while heartbeats coalesce/drop under saturation.

**Independent Test**: Flood the ingestion channel with 300 heartbeats while injecting a `StateTransition(Working)` event; assert that heartbeats are coalesced/dropped while the transition is delivered and session updates in `SessionRegistry`.

### Tests for User Story 4
- [X] T115 [P] [US4] Write unit tests in `crates/watchai-core/tests/event_tests.rs` for `SessionLifecycleEvent` validation, testing legal FSM transitions, rejection of stale timestamps, and rejection of events for unknown sessions.
- [X] T118 [P] [US4] Write integration tests in `tests/multi_provider_test.rs` verifying that flooding the ingestion channel with heartbeats drops/coalesces heartbeats while delivering 100% of critical `StateTransition(Working)` events without deadlock or indefinite hangs.
- [X] T119 [P] [US4] Write integration tests in `tests/multi_provider_test.rs` asserting cooperative shutdown: dropping channel sender drains pending critical transitions before worker task exits.

### Implementation for User Story 4
- [X] T116 [US4] Implement bounded asynchronous MPSC event channel (`capacity = 256`) and spawn event consumer task in `crates/watchai-daemon/src/main.rs`, routing events to `SessionRegistry`, updating session state, and broadcasting D-Bus `SessionUpdated` and throttled `AggregateStateChanged`.
- [X] T117 [US4] Implement prioritized event handling and backpressure in `crates/watchai-daemon/src/main.rs` (depends on T116): critical transitions (`StateTransition`, `SessionTerminated`) use `.send().await` with cooperative cancellation on shutdown, while `Heartbeat` uses `try_send()` and drops/coalesces when channel capacity exceeds 80%.

**Checkpoint**: User Story 4 establishes the decoupled event ingestion seam with prioritized backpressure.

---

## Phase 6: Multi-Provider Crash Recovery & IPC/UI Compatibility (Priority: P2)

**Goal**: Verify that multi-provider discovery integrates seamlessly with Phase 7 startup crash recovery, canonical sorting, existing D-Bus contracts, and GNOME Shell popover rendering.

- [X] T120 [P] Write integration tests in `tests/multi_provider_test.rs` verifying startup recovery sweep across all registered providers, asserting canonical sorting (`provider_id` ASC $\rightarrow$ `project_path` ASC $\rightarrow$ `process_id` ASC) and pre-claim aggregate computation before D-Bus announcement.
- [X] T121 [P] Write integration tests in `tests/multi_provider_test.rs` asserting that D-Bus `GetSessions()` returns sessions across all three providers formatted as `(sssssssus)` with zero IPC contract changes.
- [X] T122 [P] Write GJS unit tests in `extension/tests/test_popover.js` verifying that the existing session popover menu renders session cards from `codex-cli` and `opencode` with display names `"OpenAI Codex"` and `"OpenCode"` and correct priority ordering.

**Checkpoint**: Multi-provider sessions recover deterministically after crash and render cleanly in GNOME Shell.

---

## Phase 7: Polish, Performance Benchmarking & Documentation

**Purpose**: Verify algorithmic performance targets, update architecture and contract documentation, and run quickstart validation scenarios.

- [X] T123 [P] Implement synthetic benchmark in `crates/watchai-adapters/tests/provider_tests.rs` verifying $O(P)$ single-pass discovery scaling across 1000 simulated `/proc` entries against the 50ms performance target.
- [X] T124 [P] Update architectural documentation in `docs/architecture.md` describing multi-provider architecture, `TelemetryTier`, `ProviderCapabilities`, and the asynchronous event ingestion seam.
- [X] T125 [P] Update contract documentation in `docs/contracts/provider-adapter-contract.md` with final trait signatures and backpressure invariants.
- [X] T126 Run and record all 5 validation scenarios in `specs/004-provider-adapters/quickstart.md`.

---

## Dependencies & Execution Order

```text
Phase 1: Domain Infrastructure (T096–T099)
  │
  ├─► Phase 2: User Story 1 - Multi-Provider Process Discovery (T100–T104) [US1] 🎯 MVP
  │     │
  │     ├─► Phase 3: User Story 2 - Provider Capability Modeling (T106–T110) [US2]
  │     │
  │     └─► Phase 4: User Story 3 - Environment Health Diagnostics (T113–T114) [US3]
  │
  └─► Phase 5: User Story 4 - Asynchronous Event Ingestion Seam (T115–T119) [US4]
        │
        └─► Phase 6: Multi-Provider Crash Recovery & IPC/UI Compatibility (T120–T122)
              │
              └─► Phase 7: Polish, Performance & Documentation (T123–T126)
```

### Parallel Execution Opportunities
- **Phase 1**: `T096`, `T097`, and `T099` touch different files and can be authored concurrently; `T098` executes sequentially after `T097`.
- **Phase 2 (US1)**: Test tasks `T100` and `T104` can run in parallel before adapter implementations `T101` and `T102`.
- **Phase 3 (US2)**: `T106`, `T107`, and `T108` implement/verify `capabilities()` across distinct adapter files in parallel.
- **Phase 5 (US4)**: Unit test task `T115` runs concurrently with daemon channel implementation `T116`.
- **Phase 6**: `T120`, `T121`, and `T122` test recovery, D-Bus, and GJS independently.
- **Phase 7**: Documentation tasks `T124` and `T125` run in parallel with benchmark `T123`.

---

## Traceability Matrix

| Requirement / Criterion | Covered Task IDs | Notes |
|:---|:---|:---|
| **FR-001** (Codex CLI discovery) | `T100`, `T101`, `T104` | Tested & implemented |
| **FR-002** (OpenCode discovery) | `T100`, `T102`, `T104` | Tested & implemented |
| **FR-003** (Default registry registration) | `T103`, `T104` | Registered in `default_registry()` |
| **FR-004** (Surrogate session identity) | `T099`, `T104` | SHA256 deterministic hash |
| **FR-005** (Per-process fault isolation) | `T099`, `T100` | Isolated `Result` handling in `/proc` scan |
| **FR-006** (Initial IDLE state & start time) | `T099`, `T104`, `T110` | Enforces `IDLE + DISCOVERY_REQUIRED` |
| **FR-007** (Provider capability trait) | `T096`, `T106`, `T107`, `T108` | Trait definition & default implementation |
| **FR-008** (ProcessDiscoveryOnly baseline) | `T096`, `T106`, `T107`, `T108`, `T109` | Implemented across all 3 adapters |
| **FR-009** (Activity boundary enforcement) | `T110` | Discovered sessions never promote to WORKING |
| **FR-010** (SessionLifecycleEvent definition) | `T097`, `T098`, `T115` | Critical vs heartbeat variants |
| **FR-011** (Bounded MPSC channel) | `T116`, `T117`, `T118` | Bounded capacity 256 |
| **FR-012** (FSM validation & prioritized delivery) | `T098`, `T115`, `T117`, `T118` | Transitions guaranteed; heartbeats dropped |
| **FR-013** (D-Bus IPC compatibility) | `T121` | Existing `(sssssssus)` contract preserved |
| **FR-014** (GNOME popover rendering) | `T122` | Dynamic display names rendered |
| **NFR-001** (Zero cloud / local-first) | `T101`, `T102`, `T113` | 100% local operation |
| **NFR-002** (Zero-leakage privacy) | `T097`, `T099`, `T116` | Whitelisted metadata only |
| **NFR-003** (Crash recovery compatibility) | `T120` | Pre-claim sweep & deterministic sorting |
| **NFR-004** (Performance target <50ms) | `T099`, `T123` | $O(P)$ scaling benchmark |
| **NFR-005** (Resource bounding) | `T116`, `T117`, `T118` | Prioritized backpressure |
| **SC-001** (Concurrent discovery <500ms) | `T104`, `T120` | Multi-provider discovery test |
| **SC-002** (Deterministic sorting) | `T120` | Sorts `provider_id` $\rightarrow$ `path` $\rightarrow$ `pid` |
| **SC-003** (Activity knowledge boundary) | `T110` | IDLE + DISCOVERY_REQUIRED verified |
| **SC-004** (Event transition latency) | `T116`, `T118` | Rapid FSM state update & D-Bus signal |
| **SC-005** (Zero regression) | `T120`, `T121`, `T122` | All Phase 1–7 test suites pass |
| **SC-006** (Prioritized delivery under saturation) | `T118` | Heartbeats drop while transitions survive |

---

## Phase Boundaries & Governance Checks

### Explicit Exclusions (Phase 9+)
- **Task T022**: Claude Code opt-in hook event telemetry receiver remains strictly reserved, untouched, and unchecked `[ ]`.
- Inventing unverified or speculative hook formats, proprietary APIs, or CLI wrapper flags for Claude Code, Codex, or OpenCode.
- systemd user service unit files, systemd generators, or installation scripts (deferred to Phase 9+).
- GSettings configuration schemas or GNOME preferences dialogs (deferred to Phase 10+).
- Packaging, Meson build configuration, RPM/DEB packaging, Flatpak manifests (deferred to Phase 11+).
- UI redesign of the GNOME Shell popover or top-bar indicator.
- D-Bus interface changes or new D-Bus properties.
