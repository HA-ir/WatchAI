# Technical Research & Architecture Decisions: Crash & Recovery

**Feature**: Phase 7 — Crash & Recovery (`003-crash-recovery`)  
**Date**: 2026-10-04  
**Status**: Complete  

---

## 1. Context & Research Scope
Phase 7 specifies failure handling, recovery discovery, and client reconnection across three decoupled components:
1. `watchai-daemon`: Background Rust daemon orchestrating process discovery, FSM liveness, and D-Bus server.
2. `extension/`: GNOME Shell 45+ ESM extension in GJS communicating over D-Bus via `Gio.DBusProxy`.
3. Agent Processes: Independent OS processes running in terminal sessions (`claude`, `codex`, `opencode`).

This document records the technical trade-offs, evaluated alternatives, and architectural decisions governing recovery.

---

## 2. Research Decisions

### Decision 1: Convergence via Startup Discovery vs. Write-Ahead Disk Logging
- **Decision**: Reject on-disk persistence (SQLite, JSON state files); maintain 100% volatile in-memory registry and reconstruct state via an immediate `/proc` recovery sweep on daemon startup before claiming the well-known bus name.
- **Rationale**:
  - Writing session states to disk introduces corruption risks on abrupt power failure or kernel panic, creates disk I/O latency, and requires complex journal recovery.
  - More critically, persisting session state to disk violates Constitution Principle IV (Local-First & Zero-Leakage Privacy) by creating persistent artifacts on the user's filesystem that could inadvertently store workspace paths or metadata.
  - The OS `/proc` filesystem is the living, authoritative source of truth. If a process is still running, its PID, start time ticks, and working directory exist in kernel memory. Recomputing $\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$ restores the exact same session identity deterministically.
- **Alternatives Considered**:
  - *SQLite Database*: Heavyweight, introduces foreign library dependencies and schema migration overhead for ephemeral monitoring data.
  - *JSON Snapshot on Clean Exit*: Fails during ungraceful terminations (`kill -9`, power cuts), creating inconsistent recovery paths between clean restarts and crashes.

---

### Decision 2: Disconnected Popover State Presentation vs. Immediate Teardown
- **Decision**: Preserve previously rendered session cards in memory, mark their visual state as `CACHED / OFFLINE`, pause live duration ticking, and display an informative offline status banner. When the daemon reconnects, replace the cached state atomically with authoritative data.
- **Rationale**:
  - Immediately tearing down the popover menu when D-Bus drops causes jarring UI flicker, closes open user menus, and destroys desktop spatial continuity.
  - Conversely, continuing live duration timers while offline is deceptive, violating the Activity Knowledge Boundary by pretending the system is actively monitoring processes when the daemon is dead.
  - Marking cards as `CACHED / OFFLINE` and freezing timers truthfully communicates: "These are the sessions that were active when connection was lost; live tracking is paused."
- **Alternatives Considered**:
  - *Immediate Empty State*: Destroys all card widgets and shows an error banner. Rejected due to poor user experience during transient daemon restarts.
  - *Keep Ticking as Live*: Deceptive and inaccurate; risks showing active work for agents that may have crashed while the daemon was down.

---

### Decision 3: Reconnection Handshake Sequencing & 5.0-Second Timeout
- **Decision**: Enforce a strict, asynchronous 5.0-second client-side timeout for the reconnection handshake and execute an atomic 5-step synchronization sequence:
  $$\text{1. Reacquire Proxy} \longrightarrow \text{2. GetAggregateState}() \longrightarrow \text{3. GetSessions}() \longrightarrow \text{4. Attach Signals} \longrightarrow \text{5. Mark Online}$$
- **Rationale**:
  - In GJS / GNOME Shell, asynchronous operations run on the main GLib event loop. If a remote method call hangs because the daemon is blocked in startup I/O, an unbounded call could leak callbacks or leave the UI in an indeterminate half-connected state.
  - A strict 5-second timeout (implemented via `GLib.timeout_add`) cancels the pending handshake, drops the proxy, and schedules a retry with exponential backoff.
  - Fetching snapshots (`GetAggregateState` and `GetSessions`) *before* establishing signal handlers ensures that initial cards and top-bar badges reflect consistent daemon state, avoiding race conditions where early signals arrive before initial lists are unpacked.
- **Alternatives Considered**:
  - *Infinite Wait*: Risks hanging GJS callbacks if the daemon deadlocks.
  - *Signal-Driven Lazy Query*: Attaching signals first without querying initial state leaves the UI blank until a session changes state.

---

### Decision 4: Jittered Exponential Backoff vs. Fixed Polling
- **Decision**: Apply exponential backoff with $\pm 20\%$ randomized jitter:
  $$\text{Interval}_{n+1} = \min(30.0\text{s}, \text{Interval}_n \times 2.0) \times (1 \pm \text{Jitter}_{20\%})$$
  Starting interval is 1.0s, ceiling is 30.0s. A successful complete handshake resets the interval to 1.0s.
- **Rationale**:
  - During rapid daemon crash loops (e.g. crashing on startup every 500ms), fixed 1-second polling floods the D-Bus daemon with connection requests and spikes CPU in Mutter.
  - Doubling intervals with jitter prevents synchronization lock-step and relieves bus contention while bounding the maximum reconnect delay to 30 seconds.
- **Alternatives Considered**:
  - *Fixed 2-Second Polling*: Floods D-Bus during sustained crash loops.
  - *Linear Backoff ($n \times 1\text{s}$)*: Too slow to ramp down during crash storms.

---

### Decision 5: Fault Isolation in Discovery and Liveness
- **Decision**: Isolate all per-process `/proc` reads inside `Result` checks. If a single `/proc/[pid]/stat` or `cwd` read fails (permission denied, zombie process, race condition during process termination), log at `debug` level and skip that process for the current cycle without aborting discovery for sibling processes.
- **Rationale**:
  - In multi-tenant or sandboxed Linux setups, foreign processes or ephemeral short-lived sub-processes can vanish between `read_dir("/proc")` and `read_to_string("/proc/[pid]/stat")`.
  - Aborting the discovery loop on any single I/O error would cause complete monitoring failure for all running agents.
- **Alternatives Considered**:
  - *Fail-Fast Daemon Panic*: Panics on I/O error. Unacceptable for desktop system software.
