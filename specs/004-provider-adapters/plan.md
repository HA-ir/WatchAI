# Implementation Plan: Phase 8 — Multi-Provider Discovery & Capability Modeling

**Branch**: `004-provider-adapters` | **Date**: 2026-10-04 | **Spec**: [specs/004-provider-adapters/spec.md](spec.md)

---

## Summary

Phase 8 elevates WatchAI from a single-provider proof-of-concept (Claude Code only) to a true multi-provider desktop monitoring platform as mandated by Constitution Principle I. It implements non-invasive `/proc` discovery adapters for OpenAI Codex CLI (`codex-cli`) and OpenCode (`opencode`), registers them alongside `claude-code` in `AdapterRegistry`, extends the `ProviderAdapter` trait with an epistemically honest `ProviderCapabilities` model (`TelemetryTier::ProcessDiscoveryOnly`), and establishes a decoupled, prioritized asynchronous event ingestion seam (`tokio::sync::mpsc::channel(256)`). All operations preserve existing D-Bus IPC contracts and crash recovery invariants with zero external network dependencies and zero telemetry collection.

---

## Technical Context

**Language/Version**: Rust 1.75+ (2021 edition)  
**Primary Dependencies**: `tokio` 1.38 (async runtime, mpsc channels), `zbus` 4.4 (D-Bus IPC), `chrono` 0.4 (UTC timestamps), `tracing` 0.1 (structured logging), `sha2` 0.10 (deterministic session hashing), `async-trait` 0.1  
**Storage**: In-memory volatile state only (`SessionRegistry`). Zero session data on disk.  
**Testing**: `cargo test --workspace` (unit and integration tests with synthetic mock `/proc` fixtures), `gjs -m` (headless GNOME Shell extension tests).  
**Target Platform**: Linux Desktop (GNOME Shell 45, 46, 47 on Mutter, systemd user session).  
**Project Type**: Layered daemon + desktop integration (Adapters $\rightarrow$ Core $\rightarrow$ IPC $\rightarrow$ Extension).  
**Performance Goals**: Discovery sweep across all registered providers SHOULD complete within **50 milliseconds** on a baseline Linux desktop with $\le 1000$ active processes ($O(P)$ single-pass `/proc` traversal).  
**Constraints**: Zero elevated privileges (`systemd --user`), 100% local-first (zero cloud/network egress), zero telemetry collection (no prompts, code, tokens, diffs, or credentials). Bounded memory buffers (256-event channel).  
**Scale/Scope**: 3 supported providers (`claude-code`, `codex-cli`, `opencode`), supporting up to 50 concurrent agent processes without UI or IPC degradation.  

---

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Verification & Architectural Evidence |
|:---|:---:|:---|
| **I. Provider-Agnostic Architecture** | **PASS** | Implements `CodexCliAdapter` and `OpenCodeAdapter` within `crates/watchai-adapters`. Core domain models (`AgentSession`, `SessionRegistry`) remain completely agnostic to provider identities. |
| **II. Strict Layered Separation** | **PASS** | Adapters normalize `/proc` realities into `DiscoveredSession` and `SessionLifecycleEvent`. Daemon manages FSM and priority aggregation. D-Bus IPC remains unchanged. Extension UI remains a thin, reactive client. |
| **III. Explicit FSM Modeling** | **PASS** | All discovered sessions initialize strictly in `IDLE` (`DISCOVERY_REQUIRED`). Event consumer validates transitions against the 8-state transition matrix. Activity Knowledge Boundary strictly preserved: `/proc` presence alone never produces `WORKING` or `WAITING`. |
| **IV. Local-First & Zero-Leakage Privacy** | **PASS** | 100% local operation. Only non-sensitive metadata (`session_id`, `provider_id`, `project_name`, `process_id`, timestamps) processed. Prompts, code, diffs, arguments, and credentials strictly excluded. |
| **V. GNOME Extension Stability** | **PASS** | Existing D-Bus IPC interface preserved with zero schema changes. Popover automatically renders new providers using display names (`OpenAI Codex`, `OpenCode`) without GJS modifications. |
| **VI. Determinism & Quality Gates** | **PASS** | Deterministic surrogate session hashing (`SHA256(PID + start_time + path)[0..16]`). Deterministic sorting (`provider_id` ASC $\rightarrow$ `project_path` ASC $\rightarrow$ `process_id` ASC). Comprehensive test suite using synthetic mock fixtures. |
| **VII. Contract Stability & Observability** | **PASS** | IPC contract `(sssssssus)` preserved. Adapter capability metadata kept internal to the daemon. Structured logging with rate limiting. T022 preserved and untouched. |

---

## Project Structure

### Documentation (Phase 8 Feature)

```text
specs/004-provider-adapters/
├── spec.md              # Clarified feature specification
├── plan.md              # This implementation plan
├── research.md          # Technical decisions and architectural audit
├── data-model.md        # Domain entities, capabilities, and event types
├── quickstart.md        # Runnable validation scenarios
└── contracts/
    └── provider-adapter-contract.md # Adapter trait and IPC invariance rules
```

### Source Code Architecture

```text
crates/
├── watchai-adapters/
│   ├── src/
│   │   ├── lib.rs              # Export modules: claude_code, codex_cli, opencode, discovery, traits, registry
│   │   ├── traits.rs           # Modify: Add ProviderCapabilities, TelemetryTier, capabilities(&self)
│   │   ├── discovery.rs        # Modify: Generalized multi-pattern matching with false-positive rejection
│   │   ├── claude_code.rs      # Modify: Implement capabilities(), preserve T022 gate
│   │   ├── codex_cli.rs        # Implement: Real CodexCliAdapter with /proc inspection
│   │   ├── opencode.rs         # Implement: Real OpenCodeAdapter with /proc inspection
│   │   └── registry.rs         # Modify: Pre-register ClaudeCodeAdapter, CodexCliAdapter, OpenCodeAdapter
│   └── tests/
│       ├── discovery_tests.rs  # Existing: Fault isolation tests
│       └── provider_tests.rs   # New: Unit tests for Codex & OpenCode discovery & false-positive rejection
│
├── watchai-core/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── state.rs            # Preserve: 8-state FSM
│   │   ├── session.rs          # Modify: Add SessionLifecycleEvent and event application helper
│   │   ├── aggregate.rs        # Preserve: Priority aggregation & dwell
│   │   └── liveness.rs         # Preserve: Liveness check & retention pruning
│   └── tests/
│       └── event_tests.rs      # New: Unit tests for SessionLifecycleEvent validation and ordering
│
├── watchai-daemon/
│   ├── src/
│   │   ├── main.rs             # Modify: Spawn MPSC ingestion consumer task; pre-claim multi-provider recovery
│   │   └── sync.rs             # Preserve: D-Bus signal throttling
│   └── tests/
│
└── tests/
    ├── crash_recovery_test.rs  # Existing Phase 7 tests
    └── multi_provider_test.rs  # New: Integration tests for concurrent multi-provider discovery & ingestion
```

---

## Detailed Implementation Steps

### 1. Adapter Capabilities & Data Types (`crates/watchai-adapters/src/traits.rs`)
- Define `TelemetryTier`: `ProcessDiscoveryOnly`, `PassiveLogTailing`, `OptInHookTelemetry`.
- Define `ProviderCapabilities`: `telemetry_tier`, `supports_tool_categories`, `supports_activity_events`.
- Add `fn capabilities(&self) -> ProviderCapabilities` to `ProviderAdapter`.
- Update `ClaudeCodeAdapter` to implement `capabilities()` returning `ProviderCapabilities::process_discovery_only()`.

### 2. Conservative Multi-Pattern Process Discovery (`crates/watchai-adapters/src/discovery.rs`)
- Extend `ProcessScanner` with specialized matching rules:
  - Match direct binary names: `claude`, `codex`, `codex-cli`, `opencode`.
  - Match script runners: `node`, `bun`, `python`, `python3` where `argv[1]` contains the agent binary.
  - Reject search commands, shells, and child utilities: `grep`, `rg`, `sh`, `bash`, `git`, `cargo`, `ps`.
  - Enforce field 22 positive start time validation (`start_time > 0`).
  - Resolve project directory via `read_link(/proc/[pid]/cwd)` with `/unknown/workspace` fallback.
  - Isolate per-process errors so unreadable processes skip cleanly without interrupting discovery.

### 3. Concrete Provider Adapters (`codex_cli.rs` & `opencode.rs`)
- Implement `CodexCliAdapter`:
  - `provider_id()`: `"codex-cli"`.
  - `display_name()`: `"OpenAI Codex"`.
  - `capabilities()`: `ProviderCapabilities::process_discovery_only()`.
  - `check_environment()`: Checks PATH for `codex` / `codex-cli`.
  - `discover_sessions()`: Scans `/proc` using conservative codex rules.
- Implement `OpenCodeAdapter`:
  - `provider_id()`: `"opencode"`.
  - `display_name()`: `"OpenCode"`.
  - `capabilities()`: `ProviderCapabilities::process_discovery_only()`.
  - `check_environment()`: Checks PATH for `opencode`.
  - `discover_sessions()`: Scans `/proc` using conservative opencode rules.
- Update `AdapterRegistry::default_registry()` to register all 3 adapters.

### 4. Decoupled Ingestion Pipeline & Channel (`crates/watchai-core`, `crates/watchai-daemon`)
- In `watchai-core/src/session.rs`:
  - Define `SessionLifecycleEvent`: `StateTransition`, `Heartbeat`, `SessionTerminated`.
  - Add helper `apply_event` validating transition legality and arrival timestamps.
- In `watchai-daemon/src/main.rs`:
  - Initialize bounded channel: `tokio::sync::mpsc::channel::<SessionLifecycleEvent>(256)`.
  - Spawn dedicated event consumer task routing valid events to `SessionRegistry`, emitting D-Bus `SessionUpdated` and throttled `AggregateStateChanged` signals.
  - Implement heartbeat drop/coalescing when buffer utilization exceeds 80% to guarantee buffer headroom for critical state transitions.
  - Cooperative shutdown: channel closure drains remaining critical transitions before termination.

### 5. Multi-Provider Startup Recovery (`crates/watchai-daemon/src/main.rs`)
- Startup recovery sweep invokes `adapter_registry.discover_all().await`.
- Discovered surviving sessions across all providers are sorted canonically (`provider_id` ASC $\rightarrow$ `project_path` ASC $\rightarrow$ `process_id` ASC).
- Populates `SessionRegistry` with `initial_state = IDLE`, `adapter_status = DiscoveryRequired`.
- Computes initial aggregate state before claiming `org.freedesktop.WatchAI` bus name.

---

## Testing Strategy

### 1. Unit Tests
- `crates/watchai-adapters/tests/provider_tests.rs`:
  - Process command line parser tests (valid direct binaries, script runners, rejected grep/shell commands, rejected child subprocesses).
  - Environment check tests with mocked PATH.
  - Adapter capability descriptor verification.
- `crates/watchai-core/tests/event_tests.rs`:
  - FSM validation of incoming `SessionLifecycleEvent::StateTransition`.
  - Rejection of stale/out-of-order timestamps.
  - Unknown session handling.

### 2. Integration Tests
- `tests/multi_provider_test.rs`:
  - Concurrent discovery of mock Claude, Codex, and OpenCode processes.
  - Deterministic sorting across arbitrary kernel directory order permutations.
  - Activity Knowledge Boundary assertion: process discovery alone strictly yields `IDLE + DISCOVERY_REQUIRED`.
  - Ingestion channel test: injecting `StateTransition(Working)` transitions session to `WORKING` and updates D-Bus state.
  - Prioritized backpressure test: flooding channel with 300 heartbeats coalesces heartbeats while delivering 100% of critical state transitions.
  - Multi-provider crash recovery test: surviving processes across all 3 providers reconstructed identically.

### 3. Regression Tests
- Re-run all existing test suites:
  - `cargo test --workspace` (all 38 existing tests must continue to pass).
  - `gjs -m extension/tests/test_reconnect.js`.
  - `gjs -m extension/tests/test_popover.js` (verifying multi-provider session card sorting and display).
  - `gjs -m extension/tests/test_indicator.js`.

---

## Explicit Scope Exclusions (Phase 9+)

Phase 8 MUST NOT implement:
- **Task T022**: Claude Code opt-in hook event telemetry receiver remains strictly reserved, untouched, and unchecked `[ ]`.
- Inventing unverified or speculative hook formats, proprietary APIs, or CLI wrapper flags for Claude Code, Codex, or OpenCode.
- systemd user service unit files, systemd generators, or installation scripts (deferred to Phase 9+).
- GSettings configuration schemas or GNOME preferences dialogs (deferred to Phase 10+).
- Packaging, Meson build configuration, RPM/DEB packaging, Flatpak manifests (deferred to Phase 11+).
- UI redesign of the GNOME Shell popover or top-bar indicator.
- D-Bus interface changes or new D-Bus properties.
