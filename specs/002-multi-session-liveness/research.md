# Phase 0: Research & Architecture Decisions — Multi-Session Aggregation & Liveness

**Feature**: Phase 6 — Multi-Session Aggregation & Liveness (`002-multi-session-liveness`)  
**Date**: 2026-10-03  
**Status**: Completed  

---

## 1. Process Liveness & PID Reuse Defense Strategy

### Context & Problem
In Linux systems, process IDs (PIDs) are rapidly recycled by the kernel. If WatchAI merely checks `kill(pid, 0)` or verifies that `/proc/[pid]` exists, a terminated agent process whose PID is recycled by an unrelated short-lived process (e.g. `cat` or `sed`) would falsely appear alive. Furthermore, reading `/proc/[pid]/stat` during rapid process state changes can occasionally trigger transient `ENOENT` or `ESRCH` errors.

### Architecture Decision: `starttime` Matching with 2-Cycle Hysteresis
1. **Process Start Identity**:
   - Field 22 of `/proc/[pid]/stat` (`starttime`) represents the process start time measured in clock ticks since system boot (as a `u64`).
   - When a session is discovered or registered with a PID, the daemon captures this value as `process_start_time: Option<u64>`.
   - On each liveness check cycle (running every 2 seconds):
     - The daemon reads `/proc/[pid]/stat`.
     - It parses `starttime` and asserts that `current_starttime == recorded_starttime`.
     - If the directory exists but `starttime` does NOT match, the original process is dead and the PID has been recycled. The session is immediately marked for termination without waiting for a second cycle.
2. **Transient Error Hysteresis**:
   - If `/proc/[pid]/stat` cannot be read due to file missing or I/O error, the daemon increments an internal counter: `consecutive_proc_failures`.
   - If `consecutive_proc_failures >= 2` (spanning 2 to 4 seconds), the daemon confirms that the process has terminated and transitions the session to `ERROR`.
   - If a read succeeds, `consecutive_proc_failures` is reset to 0.
   - This ensures that transient `/proc` read hiccups never cause false-positive error notifications, while ungraceful terminations (`kill -9`) are reliably transitioned to `ERROR` within the 5-second maximum window.
3. **Data Protection Invariant**:
   - `process_start_time` and `consecutive_proc_failures` are strictly internal runtime fields of `AgentSession` in `watchai-core`.
   - They are NOT exposed in `SessionDto` or transmitted across D-Bus.

---

## 2. Activity Knowledge Boundary (/proc vs. Telemetry)

### Context & Problem
In early prototyping, process detection was naively mapped to `WORKING`. However, a running `claude` or other CLI agent frequently sits idle at an interactive prompt waiting for user commands. Claiming an agent is `WORKING` solely because its binary is in `/proc` would cause the desktop top bar to permanently display an active working state whenever a developer leaves a terminal window open.

### Architecture Decision: Strict Event Gating
1. **Uninstrumented Process Baseline**:
   - When `/proc` discovery finds a running agent process without verified event telemetry, the session is initialized in `LifecycleState::Idle` with `AdapterStatus::DiscoveryRequired`.
   - The daemon does NOT infer `WORKING` or `WAITING` without telemetry events.
2. **Active State Verification**:
   - Transitions into `WORKING` or `WAITING` strictly require explicit telemetry events emitted by the provider (e.g., tool execution hooks or prompt submission events).
3. **Recovery from `UNKNOWN`**:
   - If a session experiences telemetry silence and transitions to `UNKNOWN`, process presence alone keeps the session alive in `UNKNOWN`.
   - Transitioning back from `UNKNOWN` to `WORKING` strictly requires a fresh telemetry event confirming active task execution.

---

## 3. Mathematical Priority Aggregation & Deterministic Tie-Breaking

### Context & Requirements
With $N$ concurrent sessions across multiple workspaces, the desktop indicator must compute a single deterministic state representing the desktop. The aggregation engine must:
- Resolve conflicting states with zero race conditions.
- Prevent background work from masking critical user-intervention states (`WAITING`, `ERROR`).
- Calculate accurate session counters.

### Priority Hierarchy & Computation Rule
$$\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$$

$$\text{AggregateState}(S) = \begin{cases}
\text{IDLE} & \text{if } S = \emptyset \\
\arg\max_{s \in S} (\text{Prio}(s)) & \text{otherwise}
\end{cases}$$

### Deterministic Tie-Breaking
- If multiple sessions share the highest priority score (e.g., two sessions in `WAITING`), the aggregate state is unambiguously `WAITING`.
- If a client requests a single contextual session to focus on, tie-breaking selects:
  1. Most recent `state_entered_at` timestamp.
  2. If timestamps are identical, lexicographical comparison of `session_id`.

---

## 4. Decoupled Completion Dwell (10s) vs. Session Retention (60s)

### Architecture Decision
To prevent conflicting lifecycle states between the top-bar indicator and the session popover:
1. **10-Second Completion Dwell (Aggregate Engine)**:
   - When all sessions reach a terminal state (`SUCCESS`, `CANCELLED`, `ERROR`), the aggregate indicator reflects that terminal state for exactly 10 seconds.
   - When the 10-second timer expires, the aggregate state transitions back to `IDLE` (if no active sessions exist).
2. **60-Second Retention Window (Session Registry)**:
   - The individual session entity in `SessionRegistry` does NOT mutate to `IDLE`. It remains in `SUCCESS`, `CANCELLED`, or `ERROR` so that the popover card accurately displays the outcome badge.
   - Exactly 60 seconds after terminal state entry, the daemon prunes the session from the registry and emits `SessionRemoved(session_id)`.
3. **Counter Boundary**:
   - `active_session_count` includes non-terminal sessions plus terminal sessions currently within their 10-second completion dwell. Once dwell elapses, the session is excluded from `active_session_count` while remaining visible in the popover during the remaining 50 seconds of retention.

---

## 5. Daemon Restart Recovery Without Disk Persistence

### Context & Requirements
Per Constitution Principle IV, session tracking is 100% volatile in-memory; no session records are written to disk. On daemon restart, existing running agent processes must be re-discovered without generating duplicate sessions or false crash transitions.

### Architecture Decision: Immediate Deterministic Discovery
1. On daemon startup, `watchai-daemon` immediately executes an initial `/proc` discovery sweep before listening for events.
2. For each discovered process, it calculates the deterministic surrogate key:
   $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
3. Because `process_start_time` and `PID` remain unchanged while the process survives, the generated session ID is identical across daemon restarts.
4. The session is registered in `IDLE` (`DISCOVERY_REQUIRED`), liveness tracking begins immediately, and zero false `ERROR` signals are emitted.
