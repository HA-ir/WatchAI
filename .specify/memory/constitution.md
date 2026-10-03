<!--
Sync Impact Report:
- Version change: Uninitialized Template → 1.0.0
- Ratification Date: 2026-10-03
- Last Amended Date: 2026-10-03
- Core Principles Defined:
  - I. Provider-Agnostic Architecture & Domain Decoupling
  - II. Strict Layered Separation of Concerns
  - III. Explicit State-Machine Modeling & Resilient Lifecycle Tracking
  - IV. Local-First Operation & Zero-Leakage Privacy by Default
  - V. GNOME Shell Extension Stability & Compositor Isolation
  - VI. Determinism, Comprehensive Testing & Quality Gates
  - VII. Contract Stability, Observability & Living Documentation
- Added Sections:
  - Core Principles (7 primary engineering invariants)
  - System Architecture & Security Standards (IPC, fault tolerance, security, privacy)
  - Development Workflow & Quality Gates (TDD, reviewability, packaging, diagnostics)
  - Governance (compliance, amendment process, versioning rules)
- Removed Sections: None (initial formal ratification replacing template placeholders)
- Follow-up TODOs: None (all template placeholders replaced with concrete requirements)
-->

# WatchAI Constitution

## Core Principles

### I. Provider-Agnostic Architecture & Domain Decoupling
The core domain model, session management logic, and presentation layers MUST NOT be coupled to any single AI agent provider. All provider-specific integration logic—including Claude Code, OpenAI Codex CLI, OpenCode, and future agents—MUST reside exclusively within dedicated provider adapters implementing a standardized adapter interface. Core abstractions MUST remain agnostic to vendor-specific wire protocols, telemetry formats, and execution models.

### II. Strict Layered Separation of Concerns
The system architecture MUST enforce strict, one-way directional dependency boundaries across four distinct tiers:
1. **Agent Integration Layer (Adapters)**: Discovers, captures, and normalizes agent events into standardized domain events.
2. **Daemon / Core Engine**: Manages session state machines, aggregates active agent lifecycles, and enforces domain rules.
3. **IPC Layer**: Exposes a versioned, secure, asynchronous local IPC interface (e.g., D-Bus user session bus or restricted Unix domain socket).
4. **GNOME Shell Extension (UI)**: Acts strictly as a thin, reactive client rendering status indicators, metrics, and menus. The UI layer MUST NEVER perform direct agent detection, process inspection, or business logic.

### III. Explicit State-Machine Modeling & Resilient Lifecycle Tracking
Agent lifecycle and session status MUST be modeled as formal, deterministic finite-state machines (FSMs). Ad-hoc boolean flags (such as `is_busy`, `is_running`, `has_error`) MUST NOT represent session state. State transitions MUST be explicitly validated against permitted lifecycle edges. The engine MUST gracefully handle partial observability, network drops, ungraceful agent terminations, process crashes, and stale sessions using deterministic timeouts and heartbeat mechanisms.

### IV. Local-First Operation & Zero-Leakage Privacy by Default
WatchAI is a strictly local-first desktop application. It MUST NOT require any external cloud service, remote server, or third-party account to function. Privacy is non-negotiable: WatchAI MUST NOT collect, inspect, persist, or transmit agent prompts, source code snippets, git diffs, file contents, environment credentials, API keys, or raw command output. Only sanitized operational metadata (e.g., agent identifier, session status, start/end timestamps, elapsed duration, generic tool categories) MAY be processed.

### V. GNOME Shell Extension Stability & Compositor Isolation
Because GNOME Shell extensions execute directly within the GNOME Shell / Mutter compositor thread (GJS), extension instability endangers the entire desktop session. The GNOME Shell extension MUST remain lightweight, asynchronous, and strictly non-blocking. Synchronous I/O, heavy computation, long-polling, and direct filesystem scans inside the extension are strictly forbidden. Any failure or crash of the backend daemon or IPC connection MUST degrade gracefully in the UI without throwing unhandled exceptions or disrupting compositor responsiveness.

### VI. Determinism, Comprehensive Testing & Quality Gates
Every core module and state transition MUST exhibit deterministic behavior. Test-Driven Development (TDD) SHOULD be practiced where practical, especially for state transitions, adapter parsers, and IPC message encoders. Implementation changes MUST be backed by a comprehensive testing pyramid:
- **Unit Tests**: State machines, adapter event normalization, and utility modules.
- **Integration Tests**: IPC serialization/deserialization, daemon session lifecycle workflows, and mock adapter pipelines.
- **End-to-End / Headless Tests**: IPC contract compliance and GNOME Shell extension harness tests.

### VII. Contract Stability, Observability & Living Documentation
All external and inter-layer boundaries—specifically Provider Adapter contracts and IPC schemas—MUST be explicitly versioned and evolve with strict backward compatibility. Observability and diagnostic logging MUST be structured and actionable while guaranteeing zero emission of sensitive or private data. Architecture documentation, interface specifications, and configuration guides MUST be maintained alongside code changes as an inseparable deliverable.

## System Architecture & Security Standards

### 1. Architectural Topology & Boundaries
- **Process Isolation**: The background daemon runs as a dedicated user-level daemon (e.g., systemd user service `watchai.service`). The GNOME Shell extension connects to this daemon exclusively via the local user IPC bus.
- **Zero Elevated Privileges**: WatchAI runs entirely in user-space (`systemd --user`). It MUST NOT require or request `root` or `sudo` privileges for any runtime monitoring or indicator functionality.
- **IPC Contract & Bus Governance**: The IPC interface MUST use standard desktop IPC mechanisms (such as D-Bus on the session bus at `org.freedesktop.WatchAI` or restricted Unix domain sockets under `$XDG_RUNTIME_DIR/watchai/`). Access MUST be constrained to the active desktop session user.

### 2. Privacy & Data Sanitization Specification
- **Prohibited Data Classes**: The following data types MUST NEVER be collected, cached, logged, or exposed across IPC:
  - User prompts, instructions, context windows, and model outputs.
  - Source code, file paths containing sensitive project identifiers, file contents, and patch diffs.
  - Environment variables, authentication tokens, API keys, and credential stores.
  - Raw stdout/stderr terminal streams and shell execution histories.
- **Permitted Metadata**:
  - Provider identifier (e.g., `claude-code`, `codex-cli`, `opencode`).
  - Session identifier (UUIDv4 or cryptographic local hash).
  - Normalized lifecycle state (e.g., `Idle`, `Planning`, `Executing`, `AwaitingInput`, `Terminated`).
  - Timing statistics (session start, state entry timestamp, duration).
  - High-level tool activity category (e.g., `FileIO`, `Execution`, `Searching`) without arguments or targets.

### 3. Fault Tolerance & Stale Session Handling
- **Heartbeat & Liveness Verification**: Adapters and daemon workers MUST implement active liveness detection (e.g., periodic health checks or PID/socket monitoring).
- **Graceful Degradation**: If an agent process exits unexpectedly or terminates without a disconnect signal, the core state machine MUST transition the session to `Orphaned` or `Terminated` after a configured timeout, rather than remaining permanently in an active state.
- **Reconnection Resilience**: If the daemon restarts, the GNOME extension MUST automatically attempt exponential backoff reconnection without user intervention and without crashing the shell.

## Development Workflow & Quality Gates

### 1. Code Quality & Modularity
- **Small, Reviewable Changes**: Pull requests and commits MUST be focused on single logical changes, adhering to clear module boundaries and explicit dependency graphs.
- **Unix Philosophy**: Each component (CLI, daemon, adapter, extension) MUST do one thing well with clear input/output contracts.
- **Idiomatic Linux Desktop Integration**: Components MUST respect Linux standards, including XDG Base Directory specifications (`$XDG_CONFIG_HOME`, `$XDG_DATA_HOME`, `$XDG_RUNTIME_DIR`), systemd user lifecycle conventions, and GSettings configuration schemas.

### 2. Testing & Verification Gates
- No PR or merge into main branches SHALL be approved without passing unit and integration test suites.
- State-machine test suites MUST include negative path testing, race-condition assertions, and timeout verification.
- Mock adapters for each supported provider MUST be provided in the test suite to enable fully offline, deterministic test runs without requiring real AI agent execution.

### 3. Documentation & Schema Governance
- Any change to IPC protocols, adapter interfaces, or configuration schemas MUST be accompanied by an update to the corresponding specification document (`docs/spec/`, `docs/ipc/`, etc.).
- Public-facing APIs, configuration options, and command-line interfaces MUST be documented with complete examples.

## Governance

### 1. Constitutional Authority & Compliance
- This Constitution represents the supreme architectural and engineering policy for the WatchAI project.
- All functional specifications, implementation plans, tasks, pull requests, and architectural decisions MUST strictly conform to the principles and constraints established herein.
- Any pull request or specification that violates these principles (e.g., introducing cloud dependencies, leaking prompt data, or bypassing layered boundaries) MUST be rejected or amended prior to merging.

### 2. Amendment Procedure
- Amendments to this Constitution require:
  1. A documented proposal detailing the motivation, architectural trade-offs, and migration strategy.
  2. Review and consensus among project maintainers.
  3. Explicit update to the Version, Ratification, and Last Amended metadata.
  4. Generation of a temporary Sync Impact Report documenting all changes.

### 3. Semantic Versioning
The Constitution version follows Semantic Versioning (`MAJOR.MINOR.PATCH`):
- **MAJOR**: Incompatible governance shifts, removal or fundamental weakening of core principles (e.g., altering the local-only or zero-leakage privacy invariants).
- **MINOR**: Addition of new core principles, architectural layers, or significant expansions of existing standards.
- **PATCH**: Wording improvements, clarifications, typo corrections, and non-semantic formatting refinements.

**Version**: 1.0.0 | **Ratified**: 2026-10-03 | **Last Amended**: 2026-10-03
