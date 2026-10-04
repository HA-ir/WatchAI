# Feature Specification: Phase 8 — Multi-Provider Discovery & Capability Modeling

**Feature Branch**: `004-provider-adapters`  
**Created**: 2026-10-04  
**Status**: Clarified  
**Input**: Multi-provider discovery, provider capability modeling, and lifecycle event ingestion for WatchAI  

---

## 1. Problem Statement & System Context

### 1.1 The User Problem
Modern software engineers frequently employ multiple distinct local AI coding agents across projects (such as Claude Code, OpenAI Codex CLI, and OpenCode). While WatchAI's core architecture was designed to be provider-agnostic (Constitution Principle I), the current implementation only scans for Claude Code (`claude`) processes. Users running OpenAI Codex CLI (`codex`) or OpenCode (`opencode`) find their agent sessions completely invisible in the top-bar indicator and popover menu. Furthermore, the UI presents all sessions identically, even though different providers possess fundamentally different observability tiers (e.g., pure process scanning vs. structured event streaming).

### 1.2 The System Problem
1. **Empty Adapter Stubs**: `crates/watchai-adapters/src/codex_cli.rs` and `crates/watchai-adapters/src/opencode.rs` exist solely as empty 1-line comments and are not registered in `AdapterRegistry`.
2. **Missing Capability Modeling**: The current `ProviderAdapter` trait lacks any mechanism to communicate what a provider can observe. The daemon cannot determine whether an adapter supports real-time activity events (`WORKING`, `WAITING`), tool categories, or only coarse `/proc` process presence (`IDLE`).
3. **Missing Asynchronous Event Ingestion Seam**: While the daemon polls `discover_sessions()` every 2 seconds, there is no standardized, provider-agnostic asynchronous ingestion channel (`mpsc::Sender<SessionLifecycleEvent>`) for adapters to stream verified lifecycle events into the core `SessionRegistry` without tight coupling.
4. **Environment Health Diagnostics**: The daemon lacks a unified interface to report whether configured provider executables are detected, missing from PATH, or misconfigured.

---

## 2. Clarifications

### Session 2026-10-04

- **Q1 (Provider Process Identification)**: What exact `/proc` discovery rules and patterns should be matched for Codex CLI and OpenCode to prevent false positives without assuming undocumented APIs?
  - **A1**: Use conservative, testable `/proc` matching based strictly on observable local process metadata:
    - **Codex CLI**: Matches when `/proc/[pid]/cmdline` argv[0] ends with `/codex` or `/codex-cli`, or argv[0] is `node`/`bun`/`python` and argv[1] contains `codex` or `codex-cli`. Generic command-line substrings (e.g. `grep codex`, build scripts, or child subprocesses like `git` / `rg`) are explicitly rejected.
    - **OpenCode**: Matches when argv[0] ends with `/opencode`, or argv[0] is `python`/`node` and argv[1] contains `opencode`.
    - **Project Path**: Resolved via `/proc/[pid]/cwd` symlink. If unreadable, isolated to that process and falls back to `/unknown/workspace`.
    - **Process Start Time**: Requires field 22 from `/proc/[pid]/stat`, $> 0$.
    - **Fault Isolation**: Any I/O error reading a process's entries skips that PID with debug logging without failing sibling discovery.

- **Q2 (Event Channel Capacity & Backpressure Policy)**: What is the bounded buffer size for the MPSC event channel, and how are saturation and dropped events handled?
  - **A2**: The event channel uses a bounded `tokio::sync::mpsc::channel(256)` (tested implementation parameter). To ensure critical state integrity:
    - **Critical Events (`StateTransition`, `SessionTerminated`)**: MUST NEVER be silently discarded. Producers await buffer reservation (`send().await`).
    - **Heartbeats (`Heartbeat`)**: Use non-blocking `try_send()`. If channel capacity exceeds 80% utilization (or on buffer full), heartbeats are dropped or coalesced with debug trace logging, preserving headroom for critical lifecycle transitions.
    - **Shutdown**: Channel close signals the consumer loop to process all buffered critical transitions before exiting cleanly.

- **Q3 (D-Bus Capability Exposure)**: Should provider capability metadata be exposed across the D-Bus IPC contract in Phase 8?
  - **A3**: No. Capability metadata (`ProviderCapabilities`) remains strictly **internal to the daemon** in Phase 8. No GNOME Shell extension UI component currently requires D-Bus capability introspection. The existing 9-field session DTO `(sssssssus)` and all D-Bus methods/signals remain 100% backward compatible without modifications.

- **Q4 (Environment Discovery Caching)**: Should `check_environment()` be evaluated on startup only or polled periodically?
  - **A4**: **Startup-only evaluation**. `check_environment()` runs once during daemon initialization. Detecting agents installed after daemon startup is explicitly deferred to daemon restart or manual service restart. Background polling for executable presence is rejected as unnecessary complexity.

- **Q5 (Performance Benchmark Methodology)**: How should the discovery sweep performance target (NFR-004) be specified and verified without test flakiness?
  - **A5**: Converted from an absolute functional assertion to an explicit **Performance Target**:
    - Complete discovery sweep across all registered providers SHOULD complete within **50 milliseconds** on a baseline Linux desktop with $\le 1000$ active processes and warm `/proc` cache.
    - Automated unit and integration test suites will verify algorithmic scaling ($O(P)$ single-pass directory inspection) using synthetic mock fixtures (`MockProcReader`), avoiding brittle wall-clock assertions on virtualized or loaded CI environments.

- **Q6 (FSM Event Validation & Activity Boundary)**: How are incoming asynchronous events validated against the 8-state FSM?
  - **A6**: All events must strictly satisfy:
    - **State Transitions**: Validated against `LifecycleState::can_transition_to(&new_state)`. Invalid transitions are dropped with structured warnings.
    - **Monotonic Sequence Numbers**: Events must satisfy `event.sequence_number > session.sequence_number`. Out-of-order or duplicate events are logged and dropped.
    - **Unknown Sessions**: If an event references an unknown `session_id`, it is rejected unless it is a valid `StateTransition(Starting)` carrying process metadata.
    - **Activity Knowledge Boundary**: Process survival in `/proc` strictly initializes in `IDLE` (`DISCOVERY_REQUIRED`) and NEVER promotes to `WORKING` or `WAITING`. Active states strictly require verified `SessionLifecycleEvent::StateTransition` records.

- **Q7 (Capability Model Simplicity)**: What is the exact capability structure for Phase 8?
  - **A7**: A clean, minimal domain type:
    ```rust
    pub enum TelemetryTier {
        ProcessDiscoveryOnly,
        PassiveLogTailing,
        OptInHookTelemetry,
        LocalRpcStream,
    }
    pub struct ProviderCapabilities {
        pub telemetry_tier: TelemetryTier,
        pub supported_states: Vec<LifecycleState>,
        pub supports_tool_categories: bool,
    }
    ```
    For Phase 8, `ClaudeCodeAdapter`, `CodexCliAdapter`, and `OpenCodeAdapter` all report `TelemetryTier::ProcessDiscoveryOnly`, `supported_states = vec![Idle, Unknown, Error]`, and `supports_tool_categories = false`.

---

## 3. User Scenarios & Testing *(mandatory)*

### User Story 1 - Multi-Provider Process Discovery (Priority: P1) [US1]

As a developer running OpenAI Codex CLI or OpenCode on my Linux desktop, I want WatchAI to automatically discover and display my running sessions alongside Claude Code sessions, so that all my active local AI agents are monitored seamlessly in the top bar and popover menu.

**Why this priority**: Core Constitution Principle I mandates provider agnosticism. Without real discovery for the intended providers (Codex CLI and OpenCode), WatchAI is functionally a single-provider application.

**Independent Test**: Can be tested by running mock or real processes named `claude`, `codex`, and `opencode` across distinct directories, verifying that all three are discovered, populated in `SessionRegistry`, exposed over D-Bus `GetSessions()`, and rendered with their respective display names in the popover menu.

**Acceptance Scenarios**:

1. **Given** an active `codex` process in `/proc` with a readable start time and project `cwd`, **When** the daemon executes discovery, **Then** a session is created with `provider_id = "codex-cli"` and `provider_display_name = "OpenAI Codex"`.
2. **Given** an active `opencode` process in `/proc` with a readable start time and project `cwd`, **When** the daemon executes discovery, **Then** a session is created with `provider_id = "opencode"` and `provider_display_name = "OpenCode"`.
3. **Given** multiple concurrent processes from different providers, **When** startup recovery or periodic discovery runs, **Then** processes are deterministically sorted by `provider_id` ASC, `project_path` ASC, and `process_id` ASC.

---

### User Story 2 - Provider Capability Modeling (Priority: P2) [US2]

As a developer inspecting session details, I want WatchAI to know the capability boundaries of each provider adapter (e.g. process discovery only vs. live telemetry stream), so that the daemon and UI never infer active execution states (`WORKING`, `WAITING`) for providers that only support process presence.

**Why this priority**: Preserves the Activity Knowledge Boundary (Constitution Principle III). Epistemic truthfulness requires that the system explicitly knows whether an adapter can report fine-grained tool execution or only process existence.

**Independent Test**: Query `adapter.capabilities()` on each registered adapter; assert that discovery-only adapters declare `TelemetryTier::ProcessDiscoveryOnly`, preventing uninstrumented sessions from transitioning away from `IDLE` without live event evidence.

**Acceptance Scenarios**:

1. **Given** a provider adapter with `TelemetryTier::ProcessDiscoveryOnly`, **When** a process is detected in `/proc`, **Then** the session initializes with `current_state = IDLE` and `adapter_status = DiscoveryRequired`.
2. **Given** a provider adapter that does not declare capability for `ToolCategoryReporting`, **When** sessions are discovered, **Then** `active_tool_category` remains `None` (empty string over D-Bus) without fabricating placeholder categories.

---

### User Story 3 - Provider Environment Health & Diagnostics (Priority: P3) [US3]

As a user troubleshooting my AI developer tools, I want the daemon to probe whether `claude`, `codex`, and `opencode` binaries exist in `$PATH` or common local directories on startup, so that structured logs report adapter readiness accurately.

**Why this priority**: Avoids silent discovery failures. If a user runs an agent installed via an unexported PATH or alternative package manager, diagnostic status clarifies why the agent is not discovered.

**Independent Test**: Assert that `check_environment()` returns `AdapterStatus::Active` when the provider binary exists in PATH, and `AdapterStatus::DiscoveryRequired` or `AdapterStatus::ConfigRequired` when absent, with structured debug logging.

**Acceptance Scenarios**:

1. **Given** the `codex` binary is installed in `/usr/local/bin/codex`, **When** `CodexCliAdapter::check_environment()` is invoked on startup, **Then** it returns `AdapterStatus::Active`.
2. **Given** the `opencode` binary is absent from `$PATH`, **When** `OpenCodeAdapter::check_environment()` is invoked on startup, **Then** it returns `AdapterStatus::DiscoveryRequired` and logs diagnostic guidance at `debug` level.

---

### User Story 4 - Standardized Provider Lifecycle Event Ingestion Seam (Priority: P4) [US4]

As an adapter developer, I want a decoupled, asynchronous event channel between adapters and the daemon engine, so that adapters can ingest verified lifecycle events (`SessionLifecycleEvent`) into the core state machine without tight coupling to internal registry locks or D-Bus signal dispatchers.

**Why this priority**: Establishes the architectural foundation for future event-driven telemetry (such as hooks or local sockets) without requiring ad-hoc daemon modifications.

**Independent Test**: Inject a synthetic `SessionLifecycleEvent` into the ingestion channel; verify that `SessionRegistry` updates the target session, evaluates transition validity against the FSM, updates aggregate state, and emits D-Bus signals.

**Acceptance Scenarios**:

1. **Given** an active session in `IDLE`, **When** an adapter pushes a valid `SessionLifecycleEvent::StateTransition(Working)` into the ingestion channel, **Then** the session transitions to `WORKING`, `last_seen_at` is updated, and D-Bus signals are broadcast.
2. **Given** a malformed or out-of-order event, **When** it arrives via the ingestion channel, **Then** the core FSM rejects it safely without crashing the ingestion worker.

---

## 4. Requirements *(mandatory)*

### Functional Requirements

#### Multi-Provider Discovery & Scanning
- **FR-001**: The system MUST implement `CodexCliAdapter` in `crates/watchai-adapters/src/codex_cli.rs`, discovering `codex` or `codex-cli` processes via non-invasive `/proc` scanning based strictly on observable argv[0]/argv[1] executable identities.
- **FR-002**: The system MUST implement `OpenCodeAdapter` in `crates/watchai-adapters/src/opencode.rs`, discovering `opencode` processes via non-invasive `/proc` scanning based strictly on observable argv[0]/argv[1] executable identities.
- **FR-003**: `AdapterRegistry::default_registry()` MUST register `ClaudeCodeAdapter`, `CodexCliAdapter`, and `OpenCodeAdapter` as built-in default providers.
- **FR-004**: Each provider adapter MUST derive deterministic surrogate session IDs using the canonical identity formula:
  $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
- **FR-005**: All provider adapters MUST enforce per-process fault isolation during `/proc` directory traversal, ensuring unreadable stat files, missing cwd links, or rapid process terminations do not abort discovery for sibling processes or other providers.
- **FR-006**: Discovered sessions across all providers MUST initialize strictly in:
  - `current_state = LifecycleState::Idle`
  - `adapter_status = AdapterStatus::DiscoveryRequired`
  - `consecutive_proc_failures = 0`
  - `process_start_time = Some(start_time)` (where `start_time > 0`)

#### Provider Capability Modeling
- **FR-007**: The `ProviderAdapter` trait MUST define a `capabilities(&self) -> ProviderCapabilities` method returning structured capability metadata (`telemetry_tier`, `supported_states`, `supports_tool_categories`).
- **FR-008**: For Phase 8, `ClaudeCodeAdapter`, `CodexCliAdapter`, and `OpenCodeAdapter` MUST declare `TelemetryTier::ProcessDiscoveryOnly` and `supports_tool_categories = false` until verified telemetry mechanisms are implemented.
- **FR-009**: The core daemon MUST NOT promote any session to `WORKING` or `WAITING` based on process survival alone if its adapter declares `ProcessDiscoveryOnly`.

#### Decoupled Event Ingestion Pipeline
- **FR-010**: The core domain MUST define a standardized `SessionLifecycleEvent` enum:
  - `StateTransition { session_id, new_state, sequence_number, tool_category, timestamp }`
  - `SessionTerminated { session_id, exit_code, timestamp }`
  - `Heartbeat { session_id, timestamp }`
- **FR-011**: The daemon MUST provide a bounded asynchronous MPSC channel (`tokio::sync::mpsc::channel(256)`) receiving `SessionLifecycleEvent` records and routing them to `SessionRegistry`.
- **FR-012**: The event consumer MUST validate all incoming events against the 8-state FSM transition matrix, dropping invalid or out-of-order events (`sequence_number <= current`) with structured warning logs. Critical transitions (`StateTransition`, `SessionTerminated`) MUST NOT be dropped due to buffer saturation from low-value heartbeats.

#### D-Bus & Client Compatibility
- **FR-013**: The public D-Bus contract `org.freedesktop.WatchAI` MUST remain 100% backward compatible:
  - `GetSessions()` MUST return sessions from all registered providers using the existing `(sssssssus)` signature.
  - `GetAggregateState()` MUST compute priority aggregation across all providers identically.
  - All signals (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`) MUST preserve existing payloads.
  - Provider capability metadata MUST remain internal to the daemon in Phase 8 without exposing new D-Bus properties.
- **FR-014**: The GNOME Shell popover menu MUST render cards from `codex-cli` and `opencode` with their proper display names (`OpenAI Codex`, `OpenCode`) without requiring extension modifications.

---

### Non-Functional Requirements

- **NFR-001 (Zero Cloud / Local-First)**: Provider discovery, environment checks, and event ingestion MUST operate 100% locally. Zero external network requests, telemetry pings, or cloud API calls are permitted.
- **NFR-002 (Privacy Invariant)**: Ingestion events and provider discovery MUST strictly handle whitelisted operational metadata (`session_id`, `provider_id`, `project_name`, `process_id`, `current_state`, timestamps). Prompts, completions, code diffs, command arguments, and environment credentials MUST NEVER be collected, serialized, or logged.
- **NFR-003 (Crash Recovery Compatibility)**: Pre-claim startup recovery sweep in `main.rs` MUST execute discovery across all registered adapters before acquiring the D-Bus bus name, sorting all surviving sessions via `deterministic_cmp`.
- **NFR-004 (Performance Target)**: Discovery sweep across all registered providers SHOULD complete within **50 milliseconds** on a baseline Linux desktop with $\le 1000$ active processes and warm `/proc` cache. Test suites verify single-pass $O(P)$ algorithmic complexity via synthetic mock fixtures rather than flaky wall-clock assertions.
- **NFR-005 (Resource Bounding)**: The asynchronous event ingestion channel MUST use a bounded buffer (default 256 events) with prioritized handling to prevent memory growth under event contention.

---

## 5. Key Entities & Domain Types

### `ProviderCapabilities`
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryTier {
    ProcessDiscoveryOnly,
    PassiveLogTailing,
    OptInHookTelemetry,
    LocalRpcStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub telemetry_tier: TelemetryTier,
    pub supported_states: Vec<LifecycleState>,
    pub supports_tool_categories: bool,
}
```

### `SessionLifecycleEvent`
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionLifecycleEvent {
    StateTransition {
        session_id: String,
        new_state: LifecycleState,
        sequence_number: u64,
        tool_category: Option<ToolCategory>,
        timestamp: DateTime<Utc>,
    },
    Heartbeat {
        session_id: String,
        timestamp: DateTime<Utc>,
    },
    SessionTerminated {
        session_id: String,
        exit_code: Option<i32>,
        timestamp: DateTime<Utc>,
    },
}
```

---

## 6. Scope Boundaries

### Explicitly In Scope for Phase 8
- Implementation of `CodexCliAdapter` in `crates/watchai-adapters/src/codex_cli.rs` (`/proc` process scanning for `codex` / `codex-cli`).
- Implementation of `OpenCodeAdapter` in `crates/watchai-adapters/src/opencode.rs` (`/proc` process scanning for `opencode`).
- Extension of `ProviderAdapter` trait with `capabilities(&self) -> ProviderCapabilities` and startup environment checks.
- Pre-registration of `CodexCliAdapter` and `OpenCodeAdapter` in `AdapterRegistry::default_registry()`.
- Definition of `SessionLifecycleEvent` and bounded asynchronous event ingestion channel in `crates/watchai-daemon`.
- Multi-provider discovery integration tests and unit tests.
- Updates to architecture documentation.

### Explicitly Out of Scope for Phase 8
- **Task T022**: Claude Code opt-in hook event telemetry receiver remains strictly reserved, untouched, and unchecked `[ ]`.
- Inventing unverified or speculative hook formats, proprietary APIs, or CLI wrapper flags for Claude Code, Codex, or OpenCode.
- Systemd user service unit files, systemd generators, or installation scripts (deferred to Phase 9+).
- GSettings configuration schemas or GNOME preferences dialogs (deferred to Phase 10+).
- Packaging, Meson build configuration, RPM/DEB packaging, Flatpak manifests (deferred to Phase 11+).
- UI redesign of the GNOME Shell popover or top-bar indicator.
- D-Bus interface changes or new D-Bus properties.

---

## 7. Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: When processes for `claude`, `codex`, and `opencode` run concurrently, 100% of surviving processes are discovered and reported in `GetSessions()` within **500 milliseconds** of daemon startup.
- **SC-002**: Multi-provider process discovery sorting produces identical, deterministic session lists across 100% of arbitrary kernel directory read permutations.
- **SC-003**: 100% of discovered sessions from all three providers initialize in `IDLE` (`DISCOVERY_REQUIRED`), verifying that zero false `WORKING` or `WAITING` states are inferred.
- **SC-004**: Ingesting a valid `SessionLifecycleEvent` transitions a session to `WORKING` within **10 milliseconds** and emits `SessionUpdated` over D-Bus.
- **SC-005**: All existing Rust and GJS test suites pass with 0 regressions, maintaining 100% backward compatibility with Phase 1–7 contracts.
- **SC-006**: Saturated event queues coalesce or drop heartbeats while 100% of critical lifecycle state transitions and termination events are reliably delivered.
