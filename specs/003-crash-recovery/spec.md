# Feature Specification: Crash & Recovery

**Feature Branch**: `003-crash-recovery`  
**Created**: 2026-10-04  
**Status**: Clarified  
**Input**: User description: "Define Phase 7 — Crash & Recovery for WatchAI. WatchAI is a local-first GNOME Shell application that monitors AI coding agents through provider adapters -> watchai-core -> local D-Bus IPC -> GNOME Shell extension..."

---

## 1. Overview & Goals

WatchAI operates as a distributed local system comprising background daemons, independent OS coding-agent processes (Claude Code, OpenAI Codex CLI, OpenCode), and the GNOME Shell desktop compositor extension. In real-world desktop environments, processes terminate abruptly (`SIGKILL`, segfaults, out-of-memory killer, system suspend/resume, user logout), daemons restart, and IPC connections disconnect.

Phase 7 establishes the authoritative specification for **Crash & Recovery** across all system components, ensuring that:
1. **Daemon Crash Resilience & Deterministic Rediscovery**: When `watchai-daemon` crashes or restarts, it reconstructs active sessions from surviving agent processes using stable surrogate keys without emitting false `ERROR` alerts or fabricating unobserved history.
2. **GNOME Shell Disconnect & Reconnection Safety**: When the daemon disappears, the GNOME Shell extension transitions to a clean "Offline/Cached" state without freezing, leaking listeners, or stalling the compositor, and automatically re-acquires the D-Bus proxy with a 5-second handshake timeout and exponential backoff when the daemon returns.
3. **Activity Knowledge Boundary Across Reboots**: Discovered surviving processes are classified as `IDLE` (`DISCOVERY_REQUIRED`); in-flight states prior to the crash (`WORKING`, `WAITING`) are not assumed without fresh telemetry.
4. **Failure Isolation & Rapid Crash Loop Defense**: Malformed `/proc` entries or unparseable commands never crash the daemon; rapid daemon restart loops are rate-limited with jittered backoff (1s–30s) to prevent desktop D-Bus storms without spamming user desktop notifications.
5. **Zero-Leakage Privacy Invariant**: Recovery diagnostics, error logs, and state reconstruction strictly operate on non-sensitive whitelisted metadata, completely excluding prompts, source code diffs, command arguments, and credentials.

### Non-Goals
- Adding persistent on-disk database storage (SQLite, JSON state files) for sessions; volatile in-memory registry remains authoritative.
- Inventing undocumented provider-specific hooks or implementing Task T022 (T022 remains strictly pending and discovery-gated).
- Adding new provider adapters (OpenAI Codex CLI, OpenCode) in this phase (scheduled for Phase 8).
- Implementing systemd user-service packaging (scheduled for Phase 9) or GSettings preference schemas (scheduled for Phase 10).
- Altering the public D-Bus wire signature `(sssssssus)` or breaking existing client contracts.

---

## Clarifications

### Session 2026-10-04
- **Q1: Disconnected Popover Behavior**: When the daemon disconnects while the popover is open, how should the popover and session cards behave?  
  → **A: Preserved Cached Presentation with Paused Timers**: The extension preserves the last successfully received session cards in memory, marks the entire displayed popover state as `OFFLINE/CACHED`, and pauses live duration ticking. Cached cards are NOT destroyed immediately, and state transitions or durations are NOT fabricated while offline. When the daemon reconnects, cached cards are replaced with authoritative state fetched via `GetAggregateState()` and `GetSessions()`. If synchronization fails, the UI remains in `OFFLINE/CACHED` mode and retries. Cached UI data is presentation-only and is never treated as authoritative daemon state.
- **Q2: Previously WAITING State After Daemon Crash**: If an agent was in `WAITING` before a daemon crash, should WatchAI display a hint saying the agent is probably waiting for interactive input?  
  → **A: Strict Epistemic Integrity (No Inferred WAITING)**: WatchAI must preserve epistemic integrity. After restart, `/proc` proves process existence only; without fresh provider telemetry, WatchAI cannot know whether the agent is `WORKING`, `WAITING`, `IDLE`, or something else. Recovered processes are strictly registered as `IDLE` with `adapter_status = AdapterStatus::DiscoveryRequired`. No `WAITING` hint or interactive prompt state is inferred. The UI may generically state that live agent activity has not yet been observed, but must not imply a specific activity.
- **Q3: D-Bus Reconnection Handshake Timeout**: What deterministic timeout should apply to the D-Bus reconnection handshake?  
  → **A: 5-Second Deterministic Handshake Timeout**: Each reconnection handshake operation (acquiring/re-acquiring the proxy, fetching aggregate state, and fetching sessions) has a strict 5.0-second client-side timeout. The extension executes asynchronously and never blocks the GNOME Shell main loop. A timeout or failed handshake is treated as a failed attempt, triggering jittered exponential backoff (initial 1.0s, doubling each failure up to a 30.0s ceiling with $\pm 20\%$ random jitter). A complete, successful handshake resets the backoff interval to 1.0s.
- **Q4: Daemon Crash-Loop Circuit Breaker**: Should WatchAI introduce a desktop notification or circuit breaker for rapid daemon crash loops?  
  → **A: Bounded Backoff with Log Diagnostics (No Notification Spam)**: WatchAI remains an observer/monitor and does not generate desktop notification spam or attempt process supervision. Crash-loop process management belongs to systemd `--user` integration (Phase 9). The extension continues bounded exponential reconnect attempts up to the 30.0s ceiling, cleanly exposing the offline state in the top bar and popover without desktop popups. Crash diagnostics belong strictly in daemon and system logs.

---

## 2. User Scenarios & Acceptance Criteria *(mandatory)*

### User Story 1 - Deterministic Daemon Crash Recovery & Process Rediscovery (Priority: P1)

As a developer running local coding agents whose desktop session monitor restarts (due to package updates, crashes, or user restart), I want WatchAI to immediately scan `/proc` on startup, identify surviving agent processes using stable deterministic keys, and restore them to the session registry before announcing service readiness, so that surviving agents never trigger false crash alarms or phantom errors.

**Why this priority**: If daemon restarts cause surviving agents to be reported as crashed or create duplicate session entries, developer confidence in desktop alerts is completely destroyed.

**Independent Test**: Can be tested by starting a monitored agent process, killing `watchai-daemon` with `SIGKILL` (`kill -9`), restarting `watchai-daemon`, and verifying that the surviving agent is rediscovered with an identical `session_id` in `IDLE` (`DISCOVERY_REQUIRED`) with zero `ERROR` signals emitted.

**Acceptance Scenarios**:

1. **Given** an agent process running with PID $P$, start time $T$, and working directory $W$, **When** `watchai-daemon` crashes and restarts, **Then** the daemon executes an immediate `/proc` discovery sweep before claiming service readiness on D-Bus, computes $\text{SessionID} = \text{SHA256}(P + T + W)[0..16]$, and registers the session with identical identity.
2. **Given** an agent was running before daemon crash, **When** the daemon restarts and discovers the process, **Then** the session is assigned `current_state = LifecycleState::Idle` with `adapter_status = AdapterStatus::DiscoveryRequired`, avoiding false `ERROR` or assumed `WORKING` states.
3. **Given** multiple surviving agent processes in `/proc`, **When** the recovery sweep executes, **Then** sessions are registered in deterministic order sorted by provider ID ascending, project path ascending, and PID ascending.
4. **Given** an agent process terminated while the daemon was offline, **When** the daemon restarts, **Then** the dead process is absent from `/proc`, is not registered, and zero phantom sessions are resurrected.
5. **Given** a process with unreadable start time (zombie process or permission denied), **When** recovery discovery scans the process, **Then** the process is safely skipped without aborting the discovery sweep for sibling processes.

---

### User Story 2 - Resilient GNOME Shell Reconnection & Stale Client Recovery (Priority: P1)

As a desktop user, I want the GNOME Shell top-bar indicator and popover menu to gracefully reflect when the WatchAI daemon is offline, preserve cached cards without ticking stale timers, avoid freezing or crashing Mutter, and automatically reconnect with fresh state when the daemon restarts, so that my desktop environment remains completely stable.

**Why this priority**: GNOME Shell runs inside the compositor process. Any unhandled D-Bus disconnect, frozen duration loop, or blocking call can freeze the entire desktop session.

**Independent Test**: Can be tested by running GNOME Shell with the WatchAI extension, stopping the daemon, asserting that the top-bar icon switches to the offline appearance and open popover marks cards as cached/paused, restarting the daemon, and verifying that the extension executes the 5-step reconnection handshake upon daemon return and restores authoritative state (bounded by a 5.0-second timeout ceiling).

**Acceptance Scenarios**:

1. **Given** GNOME Shell extension is active, **When** the daemon terminates unexpectedly, **Then** the extension detects D-Bus name owner loss (`NameOwnerChanged`), transitions the top-bar indicator to a dimmed "Offline" style, updates AT-SPI description to "WatchAI daemon offline", pauses live duration ticking, and marks displayed popover cards as `CACHED`.
2. **Given** the popover menu is open during daemon disconnect, **When** the disconnect occurs, **Then** previously rendered cards remain visible with a distinct "Offline / Stale" notice, live duration timers are paused, and no fake state transitions are emitted.
3. **Given** the extension is in disconnected/offline state, **When** the daemon restarts and claims `org.freedesktop.WatchAI`, **Then** the extension initiates an asynchronous reconnection handshake with a 5.0-second timeout, executes the authoritative synchronization sequence, and replaces cached cards with live state.
4. **Given** the extension starts at login before the daemon is running, **When** the extension initializes, **Then** it does not crash or raise errors, entering offline waiting mode and scheduling exponential reconnection attempts.

---

### User Story 3 - Crash Loop Defense & Failure Isolation (Priority: P2)

As a desktop user whose daemon might encounter repeated startup panics or crash loops, I want the extension and daemon to employ bounded exponential backoff and isolate faults so that crash loops do not flood D-Bus with signal storms, exhaust system resources, or generate desktop notification spam.

**Why this priority**: Uncontrolled reconnect loops can consume 100% CPU in GNOME Shell and flood the D-Bus user bus, causing desktop stuttering and unresponsive input.

**Independent Test**: Can be tested by simulating repeated rapid daemon crashes and verifying that extension reconnection intervals double exponentially with jitter (1s, 2s, 4s, up to 30s ceiling) without flooding D-Bus or emitting desktop notifications.

**Acceptance Scenarios**:

1. **Given** the daemon crashes repeatedly within short intervals ($< 2$s), **When** the extension attempts reconnection, **Then** reconnection attempts follow exponential backoff with a minimum of 1.0s, doubling on each consecutive failure up to a strict ceiling of 30.0s with $\pm 20\%$ randomized jitter.
2. **Given** repeated daemon crashes occur, **When** the extension remains disconnected, **Then** the extension does NOT emit desktop notifications, does NOT invoke system process management (`systemctl`), and limits diagnostics to system logs.
3. **Given** an individual agent process in `/proc` has a corrupted, unreadable, or permission-locked stat file, **When** the daemon liveness cycle runs, **Then** the error is isolated to that specific process, logged at `debug` level, and does not crash or delay checks for other active sessions.
4. **Given** an adapter returns malformed UTF-8 in a process cmdline or working directory, **When** discovery inspects the process, **Then** the string is sanitized using lossy conversion, logged safely, and does not panic the daemon.

---

### User Story 4 - In-Flight Telemetry Re-Synchronization & Lifecycle Continuity (Priority: P3)

As a developer whose agent was mid-task (`WORKING` or `WAITING`) during a daemon restart, I want fresh incoming telemetry events after restart to restore the active state seamlessly without requiring process restarts, so that ongoing tasks continue to be monitored accurately.

**Why this priority**: Preserving the ability of surviving agents to resume emitting fine-grained telemetry ensures continuity of monitoring across daemon maintenance without manual process restarts.

**Independent Test**: Can be tested by recovering a session as `IDLE` after restart, injecting a valid telemetry event with an incremented sequence number, and verifying that the session smoothly transitions to `WORKING` and updates aggregate state.

**Acceptance Scenarios**:

1. **Given** a recovered process session in `IDLE` (`DISCOVERY_REQUIRED`), **When** a valid telemetry event arrives from a provider adapter, **Then** the session transitions to `WORKING` (or target state) and updates `last_seen_at`.
2. **Given** an unmonitored session recovered in `UNKNOWN`, **When** process presence alone is observed in `/proc`, **Then** the session remains in `UNKNOWN` and is NOT promoted to `WORKING` without verified activity telemetry.

---

## 3. Requirements *(mandatory)*

### Functional Requirements

#### Daemon Recovery & Discovery Sweep
- **FR-001**: On startup, `watchai-daemon` MUST execute an immediate `/proc` discovery sweep and compute the initial aggregate state prior to claiming the well-known D-Bus name `org.freedesktop.WatchAI` or announcing service readiness.
- **FR-002**: For each surviving agent process detected during the recovery sweep, the daemon MUST calculate the stable deterministic session identity:
  $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
- **FR-003**: The recovery sweep MUST verify that `/proc/[pid]/stat` field 22 (`process_start_time`) is a positive integer ($> 0$); if unreadable or 0, the process MUST be skipped for the current sweep and allowed to be rediscovered on a subsequent cycle.
- **FR-004**: Surviving processes discovered during recovery MUST be initialized strictly with:
  - `current_state = LifecycleState::Idle`
  - `adapter_status = AdapterStatus::DiscoveryRequired`
  - `consecutive_proc_failures = 0`
  - `process_start_time = Some(start_time)`
- **FR-005**: Recovered sessions MUST be registered in a deterministic order sorted by `provider_id` ascending, `project_path` ascending, and `process_id` ascending.
- **FR-006**: The daemon MUST NOT emit `SessionUpdated` or `AggregateStateChanged` with state `ERROR` for processes that merely survived a daemon restart.
- **FR-007**: If an agent process terminated while the daemon was offline, the daemon MUST NOT reconstruct or resurrect that session upon startup.

#### GNOME Shell Extension Disconnect & Reconnection
- **FR-008**: The GNOME Shell extension MUST monitor D-Bus `org.freedesktop.DBus` signal `NameOwnerChanged` for the bus name `org.freedesktop.WatchAI`.
- **FR-009**: When `NameOwnerChanged` indicates the daemon has lost its bus name (new owner is empty string):
  - The extension MUST transition the top-bar indicator icon to the "Offline" visual state (dimmed/grayed symbolic icon).
  - The extension MUST update AT-SPI accessible description to indicate "WatchAI daemon offline".
  - The extension MUST preserve previously received session cards in memory, marking their visual state as `CACHED / OFFLINE`.
  - The extension MUST pause live duration ticking on all session cards.
  - The extension MUST NOT fabricate state transitions, completions, or durations while offline.
- **FR-010**: When `NameOwnerChanged` indicates the daemon has claimed its bus name (new owner non-empty):
  - The extension MUST execute an asynchronous reconnection handshake with a **5.0-second timeout**.
  - The handshake MUST execute the authoritative synchronization sequence:
    1. Reacquire `Gio.DBusProxy`.
    2. Call `GetAggregateState()` to obtain authoritative desktop status.
    3. Call `GetSessions()` to obtain authoritative session records.
    4. Connect/re-attach to all 4 D-Bus signals (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`).
    5. Replace cached session cards with authoritative session cards and restore live duration ticking.
  - If any step of the handshake fails or times out (5s), the extension MUST discard the proxy, remain in `OFFLINE/CACHED` mode, and schedule a retry.
- **FR-011**: If the daemon is not running when the extension enables at desktop login, the extension MUST enter offline mode and schedule reconnection attempts without raising exceptions or blocking Mutter.
- **FR-012**: Reconnection attempts MUST follow exponential backoff:
  - Initial interval: **1.0 second**.
  - Multiplier: **2.0** on each consecutive failure.
  - Maximum ceiling: **30.0 seconds**.
  - Jitter: **$\pm 20\%$ randomized offset** applied to each interval to prevent thundering herd bus contention.
  - Success reset: A complete, successful synchronization handshake MUST reset the backoff interval to 1.0 second.
- **FR-013**: The extension MUST NOT emit desktop notification popups during daemon crash loops, and MUST NOT attempt to invoke system process managers (`systemctl`) or restart the daemon.
- **FR-014**: The extension MUST strictly disconnect all GLib timer source IDs and D-Bus proxy signal handler IDs when disabled, disconnected, or reconnecting to prevent memory leaks in GNOME Shell.

#### Activity Knowledge Boundary Across Restarts
- **FR-015**: The daemon MUST NOT assume or infer `WORKING`, `WAITING`, or interactive prompt states for recovered processes; active states strictly require live telemetry events.
- **FR-016**: The popover UI MUST NOT display hints implying that a recovered agent is waiting for interactive input; it may explain generically that live activity has not yet been observed.
- **FR-017**: If a recovered session receives a valid telemetry event confirming active task execution, the daemon MUST transition the session to the target state, update `last_seen_at`, and emit `SessionUpdated` and throttled `AggregateStateChanged` signals.
- **FR-018**: For recovered sessions that remain in `IDLE` (`DISCOVERY_REQUIRED`), process survival checks every 2 seconds MUST verify liveness via `/proc` without mutating `last_seen_at`.

#### Fault Isolation & Crash Loop Safety
- **FR-019**: A failure reading one process's `/proc` entry (permission denied, missing stat file, unparseable line) MUST be isolated to that process and MUST NOT abort discovery or liveness checks for sibling processes.
- **FR-020**: Malformed or unexpected D-Bus payload structures received by the extension MUST be caught defensively and logged safely without crashing GNOME Shell.
- **FR-021**: The daemon MUST rate-limit discovery error logging to prevent log file filling if an unreadable process persists in `/proc`.

#### Clean Shutdown & Process Lifecycle
- **FR-022**: The daemon MUST handle both `SIGINT` (Ctrl+C) and `SIGTERM` (systemd/session termination), using cooperative task cancellation to ensure all background loops complete or abort cleanly before process exit.
- **FR-023**: When the daemon shuts down cleanly, it MUST release its D-Bus well-known name so that clients immediately receive `NameOwnerChanged`.

#### Privacy Invariant
- **FR-024**: Crash recovery, diagnostics, and logging MUST strictly operate on whitelisted non-sensitive metadata (`session_id`, `provider_id`, `project_name`, `current_state`, `process_id`, timestamps, error codes). Prompts, model completions, source code diffs, command arguments, and credentials MUST NEVER be collected, persisted, or logged.

---

## 4. State Transitions & Recovery Matrix

### Component Lifecycle Scenarios

| Scenario | Daemon State | Extension State | Discovered Agents | Resulting UI State |
| :--- | :--- | :--- | :--- | :--- |
| **Normal Execution** | Running | Connected | $N$ active | Indicator displays computed aggregate state; cards live with active timers. |
| **Daemon Abrupt Crash** | Terminated | Disconnected | Alive in OS | Indicator switches to "Offline" (dimmed); cards preserved as `CACHED`; duration timers paused. |
| **Daemon Inactive at Login** | Not started | Initializing | Alive in OS | Extension starts in "Offline" mode; listens for `NameOwnerChanged`; schedules backoff reconnect. |
| **Daemon Restart Recovery** | Starting sweep | Offline / Cached | Rediscovered | Daemon registers agents as `IDLE + DISCOVERY_REQUIRED`; computes initial aggregate state before D-Bus claim. |
| **D-Bus Reconnection (Success)** | Running | Reconnecting | $N$ active | Handshake completes $<5$s: queries `GetAggregateState()`, replaces cached cards with `GetSessions()`, restores live UI. |
| **D-Bus Reconnection (Timeout)** | Stalled | Offline / Cached | Unsynchronized | Handshake exceeds 5s: drops proxy, remains `OFFLINE/CACHED`, schedules next attempt with doubled backoff. |
| **Agent Death While Daemon Down** | Restarted | Connected | None | Dead process is omitted from discovery; no false `ERROR` or resurrected phantom cards. |
| **Agent Active Telemetry Resumes** | Running | Connected | Emits event | Session transitions from `IDLE` $\rightarrow$ `WORKING`; `SessionUpdated` and `AggregateStateChanged` emitted. |

---

## 5. Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of surviving agent processes with readable start time are rediscovered within **500 milliseconds** of daemon restart.
- **SC-002**: Zero false `ERROR` transitions or alerts are emitted for agent processes that survive a daemon restart.
- **SC-003**: GNOME Shell extension detects daemon loss within **250 milliseconds** of D-Bus disconnection and switches to offline presentation without compositor freeze.
- **SC-004**: GNOME Shell extension automatically executes the 5-step reconnection handshake upon daemon name re-acquisition and synchronizes full desktop state, strictly bounded by a **5.0-second client-side timeout**.
- **SC-005**: 100% of GLib timers and D-Bus proxy signal connections are cleaned up upon extension disconnect/disable (0 memory leaks in GNOME Shell).
- **SC-006**: Extension reconnection backoff doubles exponentially up to a strict ceiling of **30 seconds** with random jitter, preventing D-Bus thundering herds during rapid daemon crash loops without desktop notification popups.
- **SC-007**: Zero sensitive data (prompts, source code, credentials, command arguments) is logged in crash recovery or diagnostics output.

---

## 6. Assumptions & Constraints

- **Volatile Storage Invariant**: In-memory volatile session storage is maintained; no on-disk database or state serialization files are introduced.
- **D-Bus Interface Stability**: 100% backward compatibility with `org.freedesktop.WatchAI` method and signal signatures (`SessionDto` `(sssssssus)` and `GetAggregateState` `(s, u, u, u, s)`).
- **Linux Platform**: Process inspection relies on standard Linux `/proc` filesystem semantics.
- **Task T022 Status**: Task T022 (Claude Code opt-in hook receiver) remains strictly pending (`[ ]`) and discovery-gated; recovery relies on baseline `/proc` inspection.
- **Task Identity Governance**: Phase 7 implementation tasks MUST NOT reuse global task ID `T022`. Global task IDs must remain globally unique across all phases (Phase 7 tasks will start from `T070`+).
- **Scope Limits**: No implementation of OpenAI Codex CLI, OpenCode, systemd units, GSettings schemas, or Meson build targets in this phase.
