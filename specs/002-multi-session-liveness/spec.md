# Feature Specification: Multi-Session Aggregation & Liveness

**Feature Branch**: `002-multi-session-liveness`

**Created**: 2026-10-03

**Status**: Draft

**Input**: User description: "Continue WatchAI development with Phase 6 — Multi-Session Aggregation & Liveness. The purpose is to define how WatchAI handles multiple concurrent agent sessions and how stale/dead sessions affect aggregate state..."

---

## 1. Overview & Goals

WatchAI monitors local AI coding agents across multiple workspaces and exposes their activity in the GNOME top bar. Phase 5 introduced the multi-session popover UI. Phase 6 establishes the authoritative behavioral and mathematical rules for:
1. **Multi-Session Priority Aggregation**: Formally calculating the single desktop aggregate state when 0, 1, or $N$ agent sessions are running concurrently across different repositories and providers.
2. **Hybrid Liveness & Crash Detection**: Periodically verifying OS process survival (`/proc/[pid]`), guarding against Linux PID reuse via process start-time verification, and detecting abrupt ungraceful process terminations (`kill -9`) within 5 seconds.
3. **Stale Session Transitions & Recovery**: Applying adaptive silence timeouts when PID monitoring is unavailable, transitioning unresponsive sessions to `UNKNOWN`, and seamlessly restoring active states if telemetry resumes.
4. **Terminal Dwell & Retention Pruning**: Enforcing a 10-second dwell window for completed states (`SUCCESS`, `CANCELLED`) before resetting to `IDLE`, and a 60-second retention window before pruning terminal sessions from memory.

### Non-Goals
- Inventing unverified Claude Code hooks or implementing task T022 (T022 remains pending).
- Integrating additional provider adapters (OpenAI Codex CLI, OpenCode) in this phase.
- Breaking or altering existing D-Bus signatures or redesigning the GNOME Shell popover.
- Collecting or persisting prompt text, code diffs, command arguments, or credentials.

---

## Clarifications

### Session 2026-10-03
- Q: When an individual agent session finishes (`SUCCESS`, `CANCELLED`), does the session entity itself remain in its terminal state in the popover during the 60-second retention window, or does the session entity transition to `IDLE` when the 10-second aggregate dwell expires? → A: Decoupled Aggregate Dwell (Option A): The session entity strictly remains in `SUCCESS`, `CANCELLED`, or `ERROR` for its entire 60-second retention lifetime in the popover. The 10-second dwell applies exclusively to the aggregate state engine, resetting the top-bar indicator to `IDLE` 10s after completion if no active sessions remain.
- Q: How should WatchAI determine whether a discovered or recovering `/proc` process is in `WORKING` versus `IDLE` when no fine-grained event telemetry is available? → A: Coarse Baseline with Event Gating (Option A): Without provider event telemetry, an uninstrumented `/proc` process presence is classified as `IDLE` (prompt-ready) and marked `DISCOVERY_REQUIRED`. Active states (`WORKING`, `WAITING`) strictly require telemetry events. If a process experiences telemetry silence and recovers, it returns to `WORKING` only upon receiving a new valid telemetry event.
- Q: How should the daemon handle process start time (`process_start_time`) from `/proc/[pid]/stat` field 22 regarding data modeling, error handling, and D-Bus exposure? → A: Internal Kernel Jiffies with Retry Guard (Option A): `process_start_time` is an internal `u64` (jiffies since boot) stored in the daemon's session registry and NOT exposed over D-Bus. A process termination/crash is confirmed only after 2 consecutive failed `/proc` reads (spanning 2–4 seconds) to prevent transient `/proc` read errors from triggering false crashes.
- Q: What exact criteria define whether an individual session is counted in `active_session_count`? → A: Non-Terminal + In-Dwell Sessions (Option A): Counts sessions in `STARTING`, `WORKING`, `WAITING`, and `UNKNOWN`, plus terminal sessions (`SUCCESS`, `CANCELLED`, `ERROR`) currently within their 10-second dwell window. Once the 10s dwell elapses, the session is excluded from `active_session_count` (even as it remains in the popover during the 60s retention window).
- Q: How should the daemon reconstruct session state and maintain identity stability if the daemon restarts while agent processes are still running? → A: Deterministic Immediate Discovery Sweep (Option A): On startup, the daemon immediately scans `/proc`. Surviving processes yield their stable deterministic session ID `SHA256(PID + process_start_time + project_path)[0..16]`, are registered in `IDLE` (marked `DISCOVERY_REQUIRED`), and avoid false `ERROR` signals.

---

## 2. User Scenarios & Acceptance Criteria *(mandatory)*

### User Story 1 - Deterministic Multi-Session Status Aggregation (Priority: P1)

As a developer running multiple coding agents simultaneously across different projects, I want the single top-bar indicator to reflect the highest-priority status using an unambiguous mathematical hierarchy, so that critical blocked states (`WAITING`) or failures (`ERROR`) are immediately visible and never obscured by background execution (`WORKING`) or completion (`SUCCESS`).

**Why this priority**: When multitasking across several agent sessions, user attention is the scarcest resource. The desktop indicator must deterministically surface actionable states (`WAITING` for permission or `ERROR`) over passive background execution.

**Independent Test**: Can be tested by instantiating sessions with conflicting states (e.g., Session 1 `WORKING`, Session 2 `WAITING`, Session 3 `SUCCESS`) in the core engine and verifying that the computed aggregate state is deterministically `WAITING`.

**Acceptance Scenarios**:

1. **Given** zero active or retaining sessions in memory, **When** aggregate state is computed, **Then** the aggregate state is `IDLE`, with active session count = 0, waiting count = 0, and error count = 0.
2. **Given** exactly one active session, **When** aggregate state is computed, **Then** the aggregate state matches that session's `current_state` exactly.
3. **Given** Session A is `WORKING` and Session B transitions to `WAITING`, **When** the aggregate state updates, **Then** the aggregate state reflects `WAITING` because $\text{WAITING} (70) > \text{WORKING} (60)$.
4. **Given** Session A is `WAITING` and Session B transitions to `ERROR`, **When** the aggregate state updates, **Then** the aggregate state reflects `ERROR` because $\text{ERROR} (80) > \text{WAITING} (70)$.
5. **Given** multiple sessions sharing the exact same highest priority (e.g., two sessions in `WAITING`), **When** aggregate state is computed, **Then** the aggregate state is `WAITING`, and tie-breaking for single-session contextual display deterministically selects the session with the most recent `state_entered_at` (broken lexicographically by `session_id`).

---

### User Story 2 - Resilient Process Liveness & PID Reuse Protection (Priority: P2)

As a developer whose agent sessions may crash, be killed abruptly (`kill -9`), or run inside terminal tabs that get closed, I want WatchAI to verify OS process liveness every 2 seconds, guard against PID recycling, and detect process death within 5 seconds, so that phantom active sessions never linger indefinitely on my desktop.

**Why this priority**: Without active process monitoring, dead or crashed agents remain permanently in `WORKING` or `WAITING` until manual intervention, destroying user trust in the monitoring system.

**Independent Test**: Can be tested by starting a monitored process, forcefully terminating it with `SIGKILL` (`kill -9`), and asserting that the daemon detects process disappearance and transitions the session to `ERROR` within 5 seconds.

**Acceptance Scenarios**:

1. **Given** an active session with a known PID, **When** `/proc/[pid]` exists and its start time in `/proc/[pid]/stat` matches the session's recorded start time, **Then** the session is confirmed alive and remains in its current state.
2. **Given** an active session with PID $P$, **When** the agent process terminates and a completely different OS process recycles PID $P$, **Then** the daemon detects that `/proc/[P]/stat` start time differs from the recorded session start time, treats the agent as dead, and transitions the session to `ERROR`.
3. **Given** an active session in `WORKING`, **When** the process is terminated abruptly (`kill -9`), **Then** the daemon detects process exit within 5 seconds, transitions the session to `ERROR`, and emits `SessionUpdated` and `AggregateStateChanged` D-Bus signals.

---

### User Story 3 - Stale Session Recovery & Terminal Retention Pruning (Priority: P3)

As a developer running long model generations, compilations, or sessions without PID access, I want telemetry silence to transition sessions gracefully to `UNKNOWN` rather than prematurely deleting them, and completed sessions to remain visible in the popover for 60 seconds before being automatically pruned, so that I have visibility into recent task outcomes without stale clutter.

**Why this priority**: Long-running tool executions (compiles, test suites) can take minutes without state changes. Distinguishing temporary silence (`UNKNOWN`) from process termination prevents premature failure alerts while ensuring dead sessions are eventually cleaned up.

**Independent Test**: Can be tested by simulating 5 minutes of telemetry silence for an unmonitored session to verify transition to `UNKNOWN`, followed by an incoming heartbeat to verify recovery to `WORKING`.

**Acceptance Scenarios**:

1. **Given** an unmonitored session (PID unavailable) in `WORKING`, **When** no telemetry event is received for 300 seconds (5 minutes), **Then** the session transitions to `UNKNOWN`.
2. **Given** a session in `UNKNOWN`, **When** a new valid telemetry event or heartbeat arrives, **Then** the session immediately recovers to `WORKING` and updates `last_seen_at`.
3. **Given** a session reaches a terminal state (`SUCCESS`, `CANCELLED`, `ERROR`), **When** 60 seconds elapse after terminal state entry, **Then** the session is pruned from the registry and a `SessionRemoved` signal is emitted.

---

### Edge Cases

- **All active sessions finish simultaneously**: What happens when two concurrent sessions both transition to `SUCCESS`?
  - *Behavior*: Aggregate state reflects `SUCCESS` for 10 seconds (dwell duration). When the dwell timer expires, aggregate state smoothly resets to `IDLE`.
- **Clock skew / system time adjustments**: What happens if the local machine time shifts (NTP sync or manual adjustment)?
  - *Behavior*: Duration and elapsed calculations must use monotonic instant offsets (`tokio::time::Instant` or `GLib.DateTime` unix timestamps clamped to $\ge 0$) to prevent negative duration calculations or premature timeouts.
- **Rapid process flapping**: What happens if an agent process exits and restarts multiple times in a few seconds?
  - *Behavior*: Each run is assigned a unique deterministic surrogate session ID (`derive_process_session_id`). The old dead PID session transitions to `ERROR`, while the new process is registered as a distinct new session.
- **Daemon restart with surviving agent processes**: What happens if `watchai-daemon` crashes and restarts while an agent continues running?
  - *Behavior*: Because session storage is purely in-memory (volatile), the daemon restarts with an empty registry and immediately runs a `/proc` discovery sweep. Surviving agent processes are rediscovered with their stable deterministic session ID and restored to the registry.

---

## 3. Requirements *(mandatory)*

### Functional Requirements

#### Multi-Session Priority Aggregation
- **FR-001**: The system MUST compute aggregate desktop state from all tracked sessions using the strict priority hierarchy:
  $$\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$$
- **FR-002**: If zero sessions exist in the registry, aggregate state MUST be `IDLE`, with `active_session_count` = 0, `waiting_session_count` = 0, and `error_session_count` = 0.
- **FR-003**: If multiple sessions share the highest priority score, deterministic tie-breaking for contextual inspection MUST choose the session with the most recent `state_entered_at` timestamp; ties with identical timestamps MUST be broken lexicographically by `session_id`.
- **FR-004**: `active_session_count` MUST equal the count of sessions whose `current_state` is non-terminal (`STARTING`, `WORKING`, `WAITING`, `UNKNOWN`) plus any terminal sessions (`SUCCESS`, `CANCELLED`, `ERROR`) currently within their active 10-second completion dwell window.
- **FR-005**: `waiting_session_count` MUST equal the count of sessions currently in `WAITING`.
- **FR-006**: `error_session_count` MUST equal the count of sessions currently in `ERROR`.
- **FR-007**: When all sessions reach a terminal state, the aggregate state MUST reflect the highest-priority terminal state (`ERROR` > `CANCELLED` > `SUCCESS`) for exactly 10 seconds before transitioning to `IDLE` if no new sessions become active.

#### Process Liveness & PID Reuse Protection
- **FR-008**: The daemon MUST execute a background liveness check against all tracked sessions with a known `process_id` at a fixed interval of **2 seconds**.
- **FR-009**: For each session with a known `process_id`, the liveness check MUST verify process survival via `/proc/[pid]/stat`.
- **FR-010**: The daemon MUST store `process_start_time` (u64 clock ticks since boot from `/proc/[pid]/stat` field 22) as private runtime session metadata (internal-only, not exposed over D-Bus). On each check, the daemon MUST verify that the live process start time matches the recorded value; an unequal start time confirms PID reuse by an unrelated process.
- **FR-011**: When a monitored process terminates or fails PID start-time verification, termination MUST be confirmed after 2 consecutive failed `/proc` checks (interval: 2s; failure confirmed within 2–4 seconds, strictly bounded by 5s max) before transitioning the session to `ERROR` and emitting `SessionUpdated` and `AggregateStateChanged` signals. Transient single-read I/O errors SHALL NOT trigger premature failure.

#### Adaptive Silence & Stale Session Handling
- **FR-012**: If a session does not have a verified PID, the daemon MUST apply adaptive silence timeouts based on `last_seen_at`:
  - Sessions in `WORKING` without telemetry for $> 300$ seconds (5 minutes) MUST transition to `UNKNOWN`.
  - Sessions in `STARTING` without telemetry for $> 60$ seconds (1 minute) MUST transition to `UNKNOWN`.
- **FR-013**: Sessions in `UNKNOWN` MUST remain visible in the popover menu with an `UNKNOWN` state badge.
- **FR-014**: If an `UNKNOWN` session emits a valid telemetry event confirming active task execution, the daemon MUST transition the session to `WORKING` and update `last_seen_at`. Merely verifying that an uninstrumented PID exists in `/proc` SHALL NOT transition a session from `UNKNOWN` to `WORKING`, but keeps the session verified as alive without inventing active execution evidence.
- **FR-015**: Terminal sessions (`SUCCESS`, `CANCELLED`, `ERROR`) MUST be retained in memory for exactly **60 seconds** after entering the terminal state, after which they MUST be pruned from the registry and emit a `SessionRemoved` signal.

#### D-Bus Contract & Signal Emission Invariants
- **FR-016**: Phase 6 MUST preserve 100% backward compatibility with the existing `org.freedesktop.WatchAI` D-Bus contract:
  - Methods: `GetAggregateState() -> (s, u, u, u, s)`, `GetSessions() -> a(sssssssus)`, `GetSession(s) -> (sssssssus)`.
  - Signals: `AggregateStateChanged(s, u, u, u, s)`, `SessionAdded((sssssssus))`, `SessionUpdated((sssssssus))`, `SessionRemoved(s)`.
- **FR-017**: The daemon MUST suppress duplicate identical `AggregateStateChanged` signals; the signal MUST be emitted only when `state`, `active_session_count`, `waiting_session_count`, or `error_session_count` changes.

#### Privacy Invariant
- **FR-018**: Liveness and aggregation processing MUST strictly operate on the approved non-sensitive metadata whitelist: `session_id`, `provider_id`, `project_name`, `current_state`, `process_id`, `active_tool_category`, timestamps. No prompts, code diffs, command outputs, or credentials shall be collected or exposed.

---

## 4. Key Entities & Formulas

### Mathematical Aggregation Function

Let $S$ be the set of tracked sessions in `SessionRegistry`.

For any session $s \in S$ and current time $t$:
$$\text{within\_dwell}(s, t) = (s.\text{current\_state} \in \{\text{SUCCESS}, \text{CANCELLED}, \text{ERROR}\}) \land (t - s.\text{state\_entered\_at} \le 10\text{ seconds})$$

$$\text{Prio}(s, t) = \begin{cases} 
80 & \text{if } s.\text{current\_state} = \text{ERROR} \text{ and } \text{within\_dwell}(s, t) \\
70 & \text{if } s.\text{current\_state} = \text{WAITING} \\
60 & \text{if } s.\text{current\_state} = \text{WORKING} \\
50 & \text{if } s.\text{current\_state} = \text{STARTING} \\
40 & \text{if } s.\text{current\_state} = \text{CANCELLED} \text{ and } \text{within\_dwell}(s, t) \\
30 & \text{if } s.\text{current\_state} = \text{SUCCESS} \text{ and } \text{within\_dwell}(s, t) \\
20 & \text{if } s.\text{current\_state} = \text{UNKNOWN} \\
10 & \text{if } s.\text{current\_state} = \text{IDLE} \\
0  & \text{otherwise (e.g. terminal session past 10s dwell)}
\end{cases}$$

$$\text{AggregateState}(S, t) = \begin{cases}
\text{IDLE} & \text{if } S = \emptyset \lor \forall s \in S, \text{Prio}(s, t) = 0 \\
\arg\max_{s \in S} (\text{Prio}(s, t)) & \text{otherwise}
\end{cases}$$

### Active Session Counter Function

$$\text{ActiveCount}(S, t) = \big| \{ s \in S \mid s.\text{current\_state} \notin \{\text{SUCCESS}, \text{CANCELLED}, \text{ERROR}\} \lor \text{within\_dwell}(s, t) \} \big|$$

$$\text{WaitingCount}(S) = \big| \{ s \in S \mid s.\text{current\_state} = \text{WAITING} \} \big|$$

$$\text{ErrorCount}(S) = \big| \{ s \in S \mid s.\text{current\_state} = \text{ERROR} \} \big|$$

---

## 5. Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Aggregate state recalculation completes in **< 1 millisecond** for up to 50 concurrent sessions in memory.
- **SC-002**: 100% of conflicting multi-session states resolve deterministically matching the priority formula.
- **SC-003**: Ungraceful agent crashes (`kill -9`) and PID reuse events are detected and transitioned to `ERROR` within **5 seconds**.
- **SC-004**: Stale sessions with lost telemetry transition to `UNKNOWN` within **300 seconds** (for `WORKING`) and **60 seconds** (for `STARTING`) without false termination.
- **SC-005**: 100% of completed sessions remain visible for exactly **10 seconds** of aggregate dwell and **60 seconds** of popover retention before memory pruning.
- **SC-006**: Zero duplicate `AggregateStateChanged` signals are emitted when session updates do not alter aggregate state or counters.
- **SC-007**: Zero memory leaks or stale background timer tasks survive session removal.

---

## 6. Assumptions & Constraints

- **Linux `/proc` Filesystem**: Standard Linux `/proc/[pid]/stat` is accessible for user-owned processes without elevated privileges.
- **T022 Status**: Task T022 (Claude Code opt-in hook receiver) remains intentionally pending; baseline `/proc` process detection provides active process PIDs for liveness tracking.
- **Storage Model**: In-memory volatile storage is maintained; no session state or liveness records are written to disk.
- **D-Bus Interface**: Zero breaking changes to `org.freedesktop.WatchAI` contract; existing methods and signals fully support Phase 6.
- **GNOME Shell UI**: The popover UI implemented in Phase 5 dynamically updates via existing `SessionUpdated` and `SessionRemoved` signals without UI code redesign.
