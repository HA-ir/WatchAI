# Data Model: Multi-Session Aggregation & Liveness

**Feature**: Phase 6 — Multi-Session Aggregation & Liveness (`002-multi-session-liveness`)  
**Date**: 2026-10-03  
**Status**: Draft  

---

## 1. Domain Entities & Schemas

### 1.1 `LifecycleState` (Enum)
The finite set of valid lifecycle states for any observed agent session:

```text
IDLE       (Priority: 10) - No active task processing; session is at prompt or unmonitored
STARTING   (Priority: 50) - Process initialized, bootstrapping or loading model context
WORKING    (Priority: 60) - Actively processing, executing tools, or generating code
WAITING    (Priority: 70) - Blocked mid-task awaiting user permission, input, or confirmation
SUCCESS    (Priority: 30) - Task completed successfully (dwells 10s on top bar, retained 60s in popover)
ERROR      (Priority: 80) - Terminated due to error, crash, or unexpected failure
CANCELLED  (Priority: 40) - Aborted or interrupted by user action
UNKNOWN    (Priority: 20) - State unverified due to telemetry silence or communication loss
```

---

### 1.2 `AgentSession` (Runtime Model)

| Field | Type | Storage / Visibility | Description |
| :--- | :--- | :--- | :--- |
| `session_id` | `String` | In-memory / Public D-Bus | Stable identifier (UUID from telemetry, or deterministic hash from process discovery). |
| `provider_id` | `String` | In-memory / Public D-Bus | Canonical provider identifier (e.g. `claude-code`). |
| `provider_display_name` | `String` | In-memory / Public D-Bus | Human-friendly name (e.g. "Claude Code"). |
| `project_name` | `String` | In-memory / Public D-Bus | Basename of project workspace directory. |
| `project_path` | `PathBuf` | In-memory only | Absolute filesystem path (kept private, stripped to basename over D-Bus). |
| `current_state` | `LifecycleState` | In-memory / Public D-Bus | Current active FSM state. |
| `sequence_number` | `u64` | In-memory only | Monotonic sequence number to drop out-of-order events. |
| `started_at` | `DateTime<Utc>` | In-memory / Public D-Bus | Timestamp when session was first discovered. |
| `state_entered_at` | `DateTime<Utc>` | In-memory / Public D-Bus | Timestamp when current state was entered. |
| `last_seen_at` | `DateTime<Utc>` | In-memory only | Timestamp of most recent telemetry event or successful `/proc` check. |
| `process_id` | `Option<u32>` | In-memory / Public D-Bus | OS PID if safely discoverable. |
| `active_tool_category` | `Option<ToolCategory>` | In-memory / Public D-Bus | Sanitized high-level category (`FileRead`, `FileWrite`, `ShellExecution`, etc.). |
| `adapter_status` | `AdapterStatus` | In-memory only | `Active`, `DiscoveryRequired`, or `Unavailable`. |
| `process_start_time` | `Option<u64>` | **In-memory only (INTERNAL)** | Process start time in clock ticks from `/proc/[pid]/stat` field 22. Used to detect PID reuse. |
| `consecutive_proc_failures` | `u32` | **In-memory only (INTERNAL)** | Failure count for `/proc` reads. Reaching 2 triggers ungraceful termination. |

---

### 1.3 `AggregateState` (Desktop Model)

| Field | Type | Visibility | Description |
| :--- | :--- | :--- | :--- |
| `state` | `LifecycleState` | Public D-Bus | Computed aggregate state representing all active sessions. |
| `active_session_count` | `u32` | Public D-Bus | Count of non-terminal sessions plus terminal sessions within their 10s completion dwell. |
| `waiting_session_count` | `u32` | Public D-Bus | Count of sessions currently in `WAITING`. |
| `error_session_count` | `u32` | Public D-Bus | Count of sessions currently in `ERROR`. |
| `updated_at` | `DateTime<Utc>` | Public D-Bus | Timestamp of the latest calculation. |

---

## 2. Mathematical Aggregation & Counting Formulas

Let $S$ be the set of all sessions in `SessionRegistry`.

### 2.1 Completion Dwell Predicate
For any session $s \in S$ and current time $t$:

$$\text{WithinDwell}(s, t) = (s.\text{current\_state} \in \{\text{SUCCESS}, \text{CANCELLED}, \text{ERROR}\}) \land (t - s.\text{state\_entered\_at} \le 10\text{ seconds})$$

### 2.2 Effective Aggregate Priority Score
$$\text{Prio}(s, t) = \begin{cases}
80 & \text{if } s.\text{current\_state} = \text{ERROR} \land \text{WithinDwell}(s, t) \\
70 & \text{if } s.\text{current\_state} = \text{WAITING} \\
60 & \text{if } s.\text{current\_state} = \text{WORKING} \\
50 & \text{if } s.\text{current\_state} = \text{STARTING} \\
40 & \text{if } s.\text{current\_state} = \text{CANCELLED} \land \text{WithinDwell}(s, t) \\
30 & \text{if } s.\text{current\_state} = \text{SUCCESS} \land \text{WithinDwell}(s, t) \\
20 & \text{if } s.\text{current\_state} = \text{UNKNOWN} \\
10 & \text{if } s.\text{current\_state} = \text{IDLE} \\
0  & \text{otherwise (e.g. terminal session past 10s dwell)}
\end{cases}$$

### 2.3 Computed Aggregate State
$$\text{AggregateState}(S, t) = \begin{cases}
\text{IDLE} & \text{if } S = \emptyset \lor \forall s \in S, \text{Prio}(s, t) = 0 \\
\arg\max_{s \in S} (\text{Prio}(s, t)) & \text{otherwise}
\end{cases}$$

### 2.4 Active Counters
$$\text{ActiveCount}(S, t) = \big| \{ s \in S \mid s.\text{current\_state} \notin \{\text{SUCCESS}, \text{CANCELLED}, \text{ERROR}\} \lor \text{WithinDwell}(s, t) \} \big|$$

$$\text{WaitingCount}(S) = \big| \{ s \in S \mid s.\text{current\_state} = \text{WAITING} \} \big|$$

$$\text{ErrorCount}(S) = \big| \{ s \in S \mid s.\text{current\_state} = \text{ERROR} \} \big|$$

---

## 3. Session Lifecycle & Retention Timeline

```text
Discovery / Event
      │
      ▼
   STARTING
      │
      ▼
   WORKING ◄───────► WAITING
      │
      ├───► UNKNOWN (silence > 5m, recoverable on new telemetry)
      │
      ├───► SUCCESS ──┐
      ├───► CANCELLED ─┼─► [10s Top-Bar Dwell] ──► (Aggregate returns to IDLE)
      └───► ERROR ────┘           │
                                  ▼
                       [60s Popover Retention] ──► Pruned from Memory (SessionRemoved)
```

---

## 4. Privacy & Whitelist Invariant

To guarantee absolute compliance with Constitution Principle IV, only whitelisted metadata fields are processed:
- Permitted: `session_id`, `provider_id`, `provider_display_name`, `project_name`, `current_state`, `started_at`, `state_entered_at`, `process_id`, `active_tool_category`.
- Internal-only: `process_start_time`, `consecutive_proc_failures`.
- Strictly Prohibited: Prompts, model completions, source code, git diffs, file contents, command-line arguments, shell output, tokens, environment variables.
