# Quickstart Validation Guide: Multi-Provider Discovery & Capability Modeling

**Feature**: Phase 8 — Multi-Provider Discovery & Capability Modeling (`004-provider-adapters`)  
**Date**: 2026-10-04  
**Status**: Draft  

---

## 1. Prerequisites
- Linux development environment with Rust toolchain (`cargo test`).
- GNOME Shell test environment (or headless GJS runner `gjs -m`).
- D-Bus inspection tools (`busctl`, `gdbus`).

---

## 2. Validation Scenarios

### Scenario 1: Multi-Provider Concurrent Process Discovery
Prove that running processes for Claude Code, OpenAI Codex CLI, and OpenCode are simultaneously discovered, populated in `SessionRegistry`, and exposed over D-Bus with correct display names.

1. **Setup**:
   - Spawn three mock processes in distinct project workspaces:
     - PID 6001: `/usr/bin/claude` in `/home/user/project-claude`
     - PID 6002: `/usr/bin/codex` in `/home/user/project-codex`
     - PID 6003: `/usr/bin/opencode` in `/home/user/project-opencode`
2. **Action**:
   - Start or trigger discovery sweep: `adapter_registry.discover_all().await`.
3. **Assert Outcome**:
   - Returns 3 discovered sessions:
     - `provider_id = "claude-code"`, `display_name = "Claude Code"`
     - `provider_id = "codex-cli"`, `display_name = "OpenAI Codex"`
     - `provider_id = "opencode"`, `display_name = "OpenCode"`
   - Each session derives a stable surrogate session ID based on PID, start time, and project path.
   - All three initialize strictly in `IDLE` (`DISCOVERY_REQUIRED`).
   - `GetSessions()` over D-Bus returns all 3 sessions formatted as `(sssssssus)`.

---

### Scenario 2: False-Positive & Child Process Rejection
Prove that shell searches, grep commands, and child worker processes are rejected and do not create false agent sessions.

1. **Setup**:
   - Spawn processes with misleading command lines:
     - PID 7001: `grep -rn codex /home/user`
     - PID 7002: `bash -c "echo opencode"`
     - PID 7003: `git status` (spawned inside a project monitored by codex)
2. **Action**:
   - Execute discovery sweep across all adapters.
3. **Assert Outcome**:
   - Zero sessions are created for PIDs 7001, 7002, 7003.
   - Rejection is logged cleanly at `trace`/`debug` level.

---

### Scenario 3: Provider Capability & Environment Health Inspection
Verify that each provider adapter reports `TelemetryTier::ProcessDiscoveryOnly` and evaluates binary existence on startup without background polling.

1. **Setup**:
   - Ensure `claude` binary exists in PATH; simulate missing `codex` and `opencode` binaries.
2. **Action**:
   - Daemon runs startup environment checks on all registered adapters.
3. **Assert Outcome**:
   - `ClaudeCodeAdapter::check_environment()` returns `AdapterStatus::Active`.
   - `CodexCliAdapter::check_environment()` returns `AdapterStatus::DiscoveryRequired`.
   - `OpenCodeAdapter::check_environment()` returns `AdapterStatus::DiscoveryRequired`.
   - `adapter.capabilities()` for all three returns `TelemetryTier::ProcessDiscoveryOnly`, `supports_tool_categories = false`, and `supports_activity_events = false`.
   - Daemon logs environment status once on startup; zero recurring background polling is scheduled.

---

### Scenario 4: Prioritized Event Ingestion & Backpressure Under Load
Verify that when the event channel is flooded with heartbeats, low-value heartbeats are coalesced or dropped, while critical state transitions are guaranteed delivery.

1. **Setup**:
   - Initialize daemon with bounded ingestion channel (`capacity = 256`).
   - Register an active session `s1` in `IDLE`.
2. **Action**:
   - Flood channel with 300 `SessionLifecycleEvent::Heartbeat` events.
   - Send 1 `SessionLifecycleEvent::StateTransition(Working)` for session `s1`.
3. **Assert Outcome**:
   - Queue drops/coalesces excess heartbeats when utilization exceeds 80%.
   - `StateTransition(Working)` is successfully received and processed by the consumer.
   - Session `s1` transitions to `WORKING`.
   - D-Bus `SessionUpdated` and `AggregateStateChanged` signals are emitted.
   - Zero daemon deadlocks or memory leaks occur.

---

### Scenario 5: Multi-Provider Crash Recovery & Deterministic Sorting
Verify that daemon crash recovery discovers surviving processes across all three providers and sorts them in canonical order before D-Bus announcement.

1. **Setup**:
   - PIDs running for Claude, Codex, and OpenCode in reverse order:
     - PID 8003: OpenCode (`/workspace/a`)
     - PID 8002: Codex (`/workspace/b`)
     - PID 8001: Claude (`/workspace/c`)
2. **Action**:
   - Simulate daemon ungraceful termination (`kill -9`) and restart.
3. **Assert Outcome**:
   - Startup discovery sweep detects all 3 surviving processes before D-Bus claim.
   - Discovered sessions are deterministically sorted:
     1. `claude-code` (`/workspace/c`, PID 8001)
     2. `codex-cli` (`/workspace/b`, PID 8002)
     3. `opencode` (`/workspace/a`, PID 8003)
   - Initial aggregate state is computed prior to claiming `org.freedesktop.WatchAI`.
   - Zero false `ERROR` alerts emitted.
