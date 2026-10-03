# Implementation Plan: Baseline Agent Monitoring & GNOME Shell Indicator

**Branch**: `001-agent-status-indicator` | **Date**: 2026-10-03 | **Spec**: [specs/001-agent-status-indicator/spec.md](spec.md)

**Input**: Feature specification from `/specs/001-agent-status-indicator/spec.md` and WatchAI Constitution (`.specify/memory/constitution.md`).

---

## Summary

WatchAI provides a Linux desktop system to monitor local AI coding agents (Claude Code, OpenAI Codex CLI, OpenCode) through a lightweight GNOME Shell top-bar indicator and an interactive session popover. 

The implementation strictly separates concerns into four architectural tiers:
1. **Provider Adapters**: Translates provider-specific events/processes into normalized domain lifecycle events with tiered discovery.
2. **Daemon / Core Engine**: Manages deterministic finite-state machines (FSMs), resolves multi-session priority aggregation, performs `/proc` liveness tracking, and maintains purely ephemeral in-memory state.
3. **Local IPC Layer**: Exposes standard D-Bus session bus service (`org.freedesktop.WatchAI`) with asynchronous signals for low-latency (<100ms) event streaming without blocking.
4. **GNOME Shell Extension (ESM)**: Acts strictly as a reactive presentation client rendering indicator icons, accessible labels, and session cards in the Mutter compositor thread.

---

## Technical Context

**Language/Version**: 
- **Daemon & Adapters**: Rust 1.80+ (2021 edition) for type-safe state machines, zero-cost concurrency, and single-binary packaging.
- **GNOME Shell Extension**: Modern JavaScript (ESM, GNOME 45+) running in GJS (GNOME JavaScript Engine).

**Primary Dependencies**:
- **Daemon**: `tokio` (async runtime), `zbus` 4.x (native async D-Bus), `serde` / `serde_json` (serialization), `chrono` (UTC timestamps), `tracing` / `tracing-subscriber` (structured diagnostics).
- **Extension**: `gi://Gio` (`Gio.DBusProxy`), `gi://St` (Shell Toolkit UI widgets), `gi://Clutter`, `gi://GObject`.

**Storage**:
- **Session State**: 100% volatile in-memory storage (zero session data or workspace telemetry written to disk).
- **User Preferences**: Standard desktop configuration via `GSettings` (schema: `org.gnome.shell.extensions.watchai`).

**Testing Frameworks**:
- `cargo test` (unit, domain FSM, and adapter contract tests).
- Mock adapter test suite with deterministic event injection.
- D-Bus integration tests using headless D-Bus daemon (`dbus-run-session`).
- Headless GNOME Shell extension tests using `gjs` test runners.

**Target Platform**: Modern Linux distributions (Ubuntu 24.04+, Fedora 40+, Debian 12+) running GNOME Shell 45+ on Wayland and X11.

**Project Type**: Multi-component Linux desktop application (background daemon service + GNOME Shell extension).

**Performance Goals**:
- State transition propagation from daemon to top bar in `<100ms`.
- Daemon memory usage `<15MB` RSS.
- Zero dropped frames or UI latency in GNOME Shell compositor thread.

**Constraints**:
- **Zero Data Leakage**: Absolute prohibition against collecting, persisting, or transmitting prompts, source code, tokens, or credentials.
- **Local-Only**: 100% offline, zero cloud or external backend requirement.
- **Unprivileged Execution**: Runs entirely in user space under `systemd --user`.

**Scale/Scope**: Support up to 50 concurrent agent sessions across multiple workspaces without performance degradation.

---

## Constitution Check

*GATE: All items MUST pass before implementation begins.*

| Principle | Requirement | Plan Compliance Status | Verification Strategy |
| :--- | :--- | :---: | :--- |
| **I. Provider-Agnostic Core** | Core domain & UI must not depend on vendor-specific concepts. | **PASS** | Domain model defines generic `LifecycleState` and `AgentSession`. Adapters are isolated in `crates/adapters/`. |
| **II. Strict Layered Separation** | Unidirectional dependencies: Adapters $\rightarrow$ Core $\rightarrow$ IPC $\rightarrow$ Extension. | **PASS** | Monorepo layout enforces crate boundaries. GNOME extension contains zero provider logic. |
| **III. Explicit State Machines** | Formal FSM modeling; no ad-hoc boolean flags. | **PASS** | Exhaustive Rust enum state machine with transition validator table; invalid transitions rejected. |
| **IV. Local-First & Zero-Leakage** | Zero remote calls; zero prompt/code/token persistence. | **PASS** | In-memory only; schema whitelist strictly enforces non-sensitive metadata only. |
| **V. GNOME Shell Stability** | Extension must be non-blocking and compositor-safe. | **PASS** | Extension uses purely asynchronous `Gio.DBusProxy` calls and signal subscriptions. |
| **VI. Determinism & Testing** | Comprehensive test pyramid with mock adapters. | **PASS** | Dedicated mock CLI and unit/integration/E2E test phases with headless verification. |
| **VII. Contract Stability** | Versioned IPC and living documentation. | **PASS** | D-Bus XML interface specification (`org.freedesktop.WatchAI.xml`) and living doc requirements. |

---

## Project Structure

```text
WatchAI/
├── Cargo.toml                       # Workspace root configuration
├── crates/
│   ├── watchai-core/                # Layer 2 & 3: Domain model, FSM engine, aggregation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── state.rs             # LifecycleState enum and FSM transition matrix
│   │       ├── session.rs           # AgentSession entity and volatile session registry
│   │       ├── aggregate.rs         # Deterministic priority aggregation algorithm
│   │       └── liveness.rs          # /proc process monitoring and adaptive silence timers
│   │
│   ├── watchai-adapters/            # Layer 1: Provider adapters & discovery engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs            # ProviderAdapter trait definition
│   │       ├── registry.rs          # AdapterRegistry catalog decoupling daemon from concrete adapters
│   │       ├── discovery.rs         # Tiered discovery coordinator (/proc scanner)
│   │       ├── claude_code.rs       # Claude Code adapter implementation
│   │       ├── codex_cli.rs         # OpenAI Codex CLI adapter implementation
│   │       └── opencode.rs          # OpenCode adapter implementation
│   │
│   ├── watchai-ipc/                 # Layer 5: D-Bus service and contract definitions
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── dbus_service.rs      # zbus 4.x implementation of org.freedesktop.WatchAI
│   │       └── protocol.rs          # IPC payload DTOs and serialization
│   │
│   └── watchai-daemon/              # Layer 4, 6, 8, 9: Daemon binary entrypoint
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs              # Daemon bootstrap, systemd notification, signal trap
│           ├── config.rs            # GSettings / XDG configuration loader
│           └── logging.rs           # Tracing subscriber setup with sanitized log filters
│
├── extension/                       # Layer 7: GNOME Shell Extension (GNOME 45+ ESM)
│   ├── metadata.json                # Extension metadata (UUID: watchai@gnome.org)
│   ├── extension.js                 # Extension lifecycle (enable/disable entrypoints)
│   ├── indicator.js                 # PanelMenu.Button implementation (top-bar icon)
│   ├── popover.js                   # Session menu section & card widgets
│   ├── dbus_client.js               # Gio.DBusProxy client wrapper (async signal listener)
│   ├── stylesheet.css               # St styling (accents for WORKING, WAITING, ERROR, SUCCESS)
│   └── schemas/                     # GSettings schema definition
│       └── org.gnome.shell.extensions.watchai.gschema.xml
│
├── systemd/                         # Layer 6: systemd --user service unit
│   └── watchai.service              # systemd user unit (Type=dbus, BusName=org.freedesktop.WatchAI)
│
├── tests/                           # Layer 10: Integration and E2E test suites
│   ├── domain_fsm_tests.rs          # Comprehensive FSM transition edge tests
│   ├── aggregation_tests.rs         # Priority resolution and multi-session tie-breaking tests
│   ├── adapter_mock_tests.rs        # Mock adapter event stream & sanitization tests
│   ├── dbus_ipc_tests.rs            # Headless D-Bus method and signal verification
│   └── fixtures/                    # Mock event sequences and process stubs
│
└── docs/                            # Living documentation
    ├── architecture.md              # Layered architecture overview
    ├── contracts/                   # IPC and adapter specifications
    └── discovery/                   # Provider-specific telemetry discovery notes
```

---

## Detailed Conceptual Layers & Responsibilities

| Layer | Component | Core Responsibilities | Explicit Exclusions |
| :--- | :--- | :--- | :--- |
| **1. Adapters** | `watchai-adapters` | Discover agent processes; validate environment; translate raw events into sanitized `SessionLifecycleEvent`. | Never modifies core state; never accesses D-Bus directly. |
| **2. Domain Model** | `watchai-core` | Defines `AgentSession`, `LifecycleState`, and whitelisted non-sensitive schemas. | No persistence code; no GUI awareness. |
| **3. FSM Engine** | `watchai-core` | Enforces valid state transitions; manages 10s `SUCCESS` dwell timers; computes deterministic aggregate state. | No direct I/O or network operations. |
| **4. Central Daemon** | `watchai-daemon` | Orchestrates adapters, liveness monitors, and IPC service; traps OS signals (`SIGTERM`, `SIGINT`). | No vendor-specific parsing (delegated to adapters). |
| **5. Local IPC** | `watchai-ipc` | Exposes `org.freedesktop.WatchAI` over user D-Bus; broadcasts `SessionUpdated` and `AggregateStateChanged`. | Enforces user UID isolation; no remote sockets. |
| **6. systemd User Service** | `systemd/watchai.service`| Manages daemon startup on graphical session boot (`Type=dbus`); handles auto-restart on failure. | Unprivileged user slice only; no root privileges. |
| **7. GNOME Extension** | `extension/` | Renders top-bar icon and menu popover; displays accessible labels; auto-reconnects on daemon restart. | Strictly non-blocking; zero agent process inspection. |
| **8. Settings** | `GSettings` / `config.rs` | Stores user preferences (dwell time, notification toggles, top-bar icon style). | Does NOT store session histories or workspace paths. |
| **9. Logging** | `watchai-daemon` | Structured local logging (`tracing`); captures operational diagnostics with prompt/code sanitization. | Zero leakage of prompt text, tokens, or credentials. |
| **10. Test Suite** | `tests/` | Unit, integration, mock-adapter, and headless IPC testing for automated regression prevention. | Zero reliance on live external AI agent endpoints. |

---

## Phased Implementation Plan

### Phase 1: Core Domain Model & Finite-State Machine Engine
- **Goal**: Implement the pure domain logic, state machine validation, and multi-session priority aggregation with 100% test coverage.
- **Dependencies**: None.
- **Deliverables**:
  - `watchai-core/src/state.rs`: `LifecycleState` enum, state transition graph, and transition validator.
  - `watchai-core/src/session.rs`: `AgentSession` entity, volatile in-memory registry, monotonic sequence tracker.
  - `watchai-core/src/aggregate.rs`: Priority aggregation algorithm ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$).
  - `tests/domain_fsm_tests.rs`: Comprehensive unit tests covering all valid transitions, rejected edges, and dwell timeouts.
- **Risks & Mitigations**:
  - *Risk*: Race conditions during concurrent state transitions across multiple threads.
  - *Mitigation*: Protect the volatile session registry using a high-performance read-write lock (`tokio::sync::RwLock`).

---

### Phase 2: Local D-Bus IPC Interface & Contract Verification
- **Goal**: Implement the `org.freedesktop.WatchAI` D-Bus service, methods, and reactive signals.
- **Dependencies**: Phase 1 (`watchai-core`).
- **Deliverables**:
  - `watchai-ipc/src/dbus_service.rs`: `zbus` 4.x service exporting methods (`GetAggregateState`, `GetSessions`, `GetSession`) and signals (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`).
  - `watchai-ipc/src/protocol.rs`: Data Transfer Objects (DTOs) adhering strictly to the data sanitization whitelist.
  - `tests/dbus_ipc_tests.rs`: Automated headless tests using `dbus-run-session` to verify method responses and signal emission.
- **Risks & Mitigations**:
  - *Risk*: D-Bus serialization overhead or blocking in async handlers.
  - *Mitigation*: Keep all D-Bus property reads strictly in-memory with zero disk or process I/O.

---

### Phase 3: Provider Adapter Interface & Tiered Discovery
- **Goal**: Implement the pluggable adapter framework with process-level discovery and telemetry normalization.
- **Dependencies**: Phase 1 (`watchai-core`), Phase 2 (`watchai-ipc`).
- **Deliverables**:
  - `watchai-adapters/src/traits.rs`: `ProviderAdapter` trait and `AdapterStatus` definition.
  - `watchai-adapters/src/registry.rs`: `AdapterRegistry` providing a dynamic collection of registered adapters.
  - `watchai-adapters/src/discovery.rs`: Non-invasive `/proc` scanner detecting active agent binaries (`claude`, `codex`, `opencode`) and extracting PIDs/working directories.
  - `watchai-adapters/src/claude_code.rs`: Claude Code adapter with hook support and process fallback.
  - `watchai-adapters/src/codex_cli.rs`: OpenAI Codex CLI adapter with `DISCOVERY_REQUIRED` fallback.
  - `watchai-adapters/src/opencode.rs`: OpenCode adapter with `DISCOVERY_REQUIRED` fallback.
  - `tests/adapter_mock_tests.rs`: Mock adapter testing event stream sanitization and out-of-order event drops.
- **Risks & Mitigations**:
  - *Risk*: Unverified or changing CLI provider internal telemetry formats.
  - *Mitigation*: Strictly enforce the Tiered Discovery Model: unverified providers fall back to process presence and display a `DISCOVERY_REQUIRED` badge in the UI.

---

### Phase 4: Central Daemon, systemd --user Service & Liveness Watcher
- **Goal**: Integrate the core engine, adapters, and IPC service into a production-grade background daemon.
- **Dependencies**: Phase 1, Phase 2, Phase 3.
- **Deliverables**:
  - `watchai-daemon/src/main.rs`: Daemon orchestrator initializing adapters, liveness timers, and D-Bus server.
  - `watchai-core/src/liveness.rs`: Background task checking `/proc/[pid]` status every 2 seconds, triggering 5-second crash transitions.
  - `systemd/watchai.service`: `Type=dbus` user unit file with `BusName=org.freedesktop.WatchAI` and auto-restart policy.
  - `watchai-daemon/src/logging.rs`: Structured `tracing` setup with sanitized log filters preventing prompt/code leakage.
- **Risks & Mitigations**:
  - *Risk*: Daemon crash leaving orphaned phantom sessions in client UIs.
  - *Mitigation*: Clients detect D-Bus `NameOwnerChanged` and immediately transition to `OFFLINE` until daemon re-registration.

---

### Phase 5: GNOME Shell Extension (GNOME 45+ ESM)
- **Goal**: Build the top-bar indicator, popover menu, and asynchronous D-Bus client for GNOME Shell.
- **Dependencies**: Phase 2 (`watchai-ipc` interface contract).
- **Deliverables**:
  - `extension/metadata.json`: GNOME 45, 46, 47 compatibility declaration.
  - `extension/dbus_client.js`: Asynchronous `Gio.DBusProxy` wrapper handling connection, signals, and auto-reconnection.
  - `extension/indicator.js`: `PanelMenu.Button` top-bar indicator displaying symbolic icons, color styles, and AT-SPI accessibility labels.
  - `extension/popover.js`: Interactive session list popover showing session cards, provider badges, elapsed durations, and empty states.
  - `extension/stylesheet.css`: High-contrast, theme-adaptive styles for `WORKING`, `WAITING`, `SUCCESS`, and `ERROR`.
  - `extension/schemas/`: GSettings schema for user preferences.
- **Risks & Mitigations**:
  - *Risk*: Extension freezing the GNOME Shell / Mutter compositor thread during IPC delays.
  - *Mitigation*: Purely asynchronous GJS calls; zero synchronous D-Bus methods; zero filesystem reads inside extension code.

---

### Phase 6: End-to-End Integration, Validation & Packaging
- **Goal**: Perform comprehensive system integration, verify all quickstart validation scenarios, and build standard Linux desktop packages.
- **Dependencies**: Phases 1 through 5.
- **Deliverables**:
  - End-to-end verification of all scenarios in `quickstart.md` (startup, mock transitions, multi-session conflict, crash recovery, UI verification).
  - Test CLI binary (`watchai-mock`) for developer simulation and QA testing.
  - Documentation updates (`README.md`, `docs/architecture.md`, `docs/contracts/`).
  - Distribution build scripts (Meson / Cargo packaging for systemd units and GNOME extensions).
- **Verification Strategy**:
  - Execute automated integration tests in CI.
  - Manually verify GNOME Shell top-bar behavior on clean Ubuntu and Fedora VM environments.

---

## Complexity Tracking

*No constitutional violations identified. No complexity bypasses or exceptions required.*

| Aspect | Baseline Choice | Why Sufficient |
| :--- | :--- | :--- |
| **Storage Architecture** | Volatile in-memory | Satisfies zero-leakage privacy without disk synchronization, migration logic, or SQLite complexity. |
| **IPC Protocol** | D-Bus Session Bus | Native to GNOME Shell (GJS), eliminating custom socket framing, thread locks, or client polling. |
| **Packaging Architecture** | `systemd --user` | Native to Linux desktops; provides logging (`journald`), supervision, and lifecycle ordering out of the box. |
