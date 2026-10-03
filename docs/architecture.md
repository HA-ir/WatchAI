# WatchAI Architecture Overview

WatchAI is a Linux desktop system designed to monitor local AI coding agents (Claude Code, OpenAI Codex CLI, OpenCode, and others) and present their real-time lifecycle states in the GNOME Shell top bar.

## Four-Tier Layered Architecture

1. **Provider Adapters (`crates/watchai-adapters`)**:
   - Detects active local agent sessions via non-invasive `/proc` scanning and opt-in event hooks.
   - Normalizes vendor-specific telemetry into generic `SessionLifecycleEvent` records.
   - Strictly enforces data sanitization: no prompts, code, tokens, or credentials enter the core domain.
   - Decoupled from the daemon via `AdapterRegistry`.

2. **Core Domain & State Machine (`crates/watchai-core`)**:
   - Implements the 8-state deterministic finite-state machine (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`).
   - Resolves multi-session priority aggregation ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$).
   - Performs hybrid process liveness tracking (`/proc` PID checks + adaptive silence timeouts).
   - Manages in-memory volatile session storage (zero session data on disk).

3. **Local IPC Layer (`crates/watchai-ipc`)**:
   - Exposes the `org.freedesktop.WatchAI` interface on the D-Bus user session bus.
   - Provides methods for aggregate and individual session inspection.
   - Broadcasts asynchronous signals (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`).

4. **GNOME Shell Extension (`extension/`)**:
   - ESM JavaScript extension for GNOME Shell (45, 46, 47).
   - Top-bar indicator button with symbolic icons, color styles, and AT-SPI accessibility descriptions.
   - Interactive popover menu detailing active sessions.
   - Strictly non-blocking: communicates with the daemon via asynchronous `Gio.DBusProxy`.

## Multi-Session Priority Aggregation

When multiple agent sessions exist simultaneously across repositories or providers, WatchAI computes a single desktop aggregate state using a strict mathematical total order:

$$\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$$

### Aggregation Semantics
- **Zero Sessions**: Resolves to `IDLE` with all counters set to 0.
- **Single Session**: Reflects the session's active state.
- **Multiple Sessions**: Selects the state with the maximum effective priority score.
- **Deterministic Tie-Breaking**: When multiple sessions share the highest priority, contextual single-session selection chooses the session with the most recent `state_entered_at` timestamp, broken lexicographically by `session_id`.

### Decoupled Dwell vs. Retention Lifecycles
1. **10-Second Aggregate Completion Dwell**:
   - Completed sessions (`SUCCESS`, `CANCELLED`, `ERROR`) participate in the aggregate priority calculation for exactly 10 seconds after completion.
   - Once 10 seconds elapse, their effective priority score drops to 0, allowing the top-bar indicator to settle smoothly to `IDLE` (if no other sessions are active).
2. **60-Second Popover Session Retention**:
   - The session record in memory does **not** mutate to `IDLE`. It remains in `SUCCESS`, `CANCELLED`, or `ERROR` for 60 seconds so the user can inspect the outcome card in the popover menu.
   - Exactly 60 seconds after terminal entry, the session is purged from the registry and a `SessionRemoved` signal is broadcast.

## Process Liveness & PID Reuse Defense

- **Periodic Liveness Check**: Daemon inspects `/proc/[pid]/stat` every 2 seconds for all sessions with an assigned PID.
- **PID Recycling Protection**: Compares field 22 (`starttime` in clock ticks since boot) against the session's recorded start time. An unequal start time indicates the OS recycled the PID for an unrelated process; the session immediately transitions to `ERROR`.
- **Two-Consecutive-Failure Hysteresis**: Process death is confirmed only after 2 consecutive failed `/proc` reads (spanning 2.0 to 4.0 seconds, bounded by 5s max) before transitioning to `ERROR`. A single transient read failure is safely tolerated and resets to 0 on any subsequent successful read.

## Activity Knowledge Boundary

Observing a process in `/proc` proves OS process existence, **not** active task execution.
- Discovered processes without verified event telemetry are registered as `IDLE` with `adapter_status = AdapterStatus::DiscoveryRequired`.
- Active execution states (`WORKING`, `WAITING`) strictly require telemetry events from provider adapters.
- Unmonitored active sessions experience adaptive silence timeouts: sessions in `WORKING` transition to `UNKNOWN` after 300 seconds of silence, while `STARTING` sessions transition to `UNKNOWN` after 60 seconds.
- An unmonitored session recovering from `UNKNOWN` transitions back to `WORKING` only upon receipt of a valid telemetry event.
