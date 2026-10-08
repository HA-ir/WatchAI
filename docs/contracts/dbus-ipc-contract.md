# Contract: D-Bus Local IPC Interface (`org.freedesktop.WatchAI`)

**Specification**: Baseline Agent Monitoring & GNOME Shell Indicator (`001-agent-status-indicator`), Crash & Recovery (`003-crash-recovery`)  
**Transport**: D-Bus User Session Bus  
**Bus Name**: `org.freedesktop.WatchAI`  
**Object Path**: `/org/freedesktop/WatchAI`  
**Interface**: `org.freedesktop.WatchAI`  

---

## 1. Overview
This contract defines the local IPC interface exposed by the WatchAI background daemon to desktop clients, specifically the GNOME Shell top-bar extension and local CLI tools.

All interactions are strictly local to the user's desktop session. Communication is authenticated and authorized implicitly by the D-Bus daemon via peer credentials (`SO_PEERCRED`), preventing other OS users from intercepting or injecting events.

---

## 2. Methods

### 2.1 `GetAggregateState`
Returns the current computed desktop-wide state and session counters.

- **Direction**: In: None $\rightarrow$ Out: `(state: String, active_session_count: u32, working_session_count: u32, waiting_session_count: u32, success_session_count: u32, error_session_count: u32, updated_at: String)`
- **Behavior**: Fast in-memory lookup. Returns immediately (<5ms).

### 2.2 `GetSessions`
Returns a list of all currently tracked agent sessions (both active and retaining completed sessions).

- **Direction**: In: None $\rightarrow$ Out: `Array of Session Structs`
- **Session Struct Signature**: `(sssssssus)`
  1. `session_id` (`s`): Unique UUIDv4 string.
  2. `provider_id` (`s`): Slug (e.g., `claude-code`).
  3. `provider_display_name` (`s`): Human-readable name.
  4. `project_name` (`s`): Basename of the project directory.
  5. `current_state` (`s`): One of `IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`.
  6. `started_at` (`s`): ISO 8601 UTC timestamp.
  7. `state_entered_at` (`s`): ISO 8601 UTC timestamp.
  8. `process_id` (`u`): Process ID (0 if unavailable).
  9. `active_tool_category` (`s`): Sanitized category (or empty string if none).

### 2.3 `GetSession`
Fetches a single session record by its unique `session_id`.

- **Direction**: In: `session_id: String` $\rightarrow$ Out: `Session Struct (sssssssus)`
- **Errors**: `org.freedesktop.WatchAI.Error.SessionNotFound` if the session does not exist or has expired.

---

## 3. Signals (Asynchronous Broadcasts)

### 3.1 `AggregateStateChanged`
Emitted immediately whenever the aggregate state or active counters change.
- **Payload**: `(state: String, active_session_count: u32, working_session_count: u32, waiting_session_count: u32, success_session_count: u32, error_session_count: u32, updated_at: String)`
- **Client Action**: GNOME Shell extension updates the top-bar icon, badge, and accessibility description.
- **Throttling Invariant**: Emitted **only** when `state`, `active_session_count`, `working_session_count`, `waiting_session_count`, `success_session_count`, or `error_session_count` changes. Intermediate heartbeats, timestamp updates, and background liveness ticks that leave the state and counters unchanged are suppressed to prevent desktop compositor redraw storms.

### 3.2 `SessionAdded`
Emitted when a new agent session is discovered or initiated.
- **Payload**: `(session: Session Struct)`
- **Client Action**: Popover menu inserts a new session card.

### 3.3 `SessionUpdated`
Emitted when an existing session transitions to a new lifecycle state or updates its tool activity.
- **Payload**: `(session: Session Struct)`
- **Client Action**: Popover menu updates the corresponding session card state badge and duration.
- **Ordering on Process Crash**: If a session terminates abruptly, `SessionUpdated` is broadcast first with the updated `ERROR` state, followed immediately by `AggregateStateChanged`.

### 3.4 `SessionRemoved`
Emitted strictly when a completed or dead session's 60-second retention window expires and it is pruned from memory.
- **Payload**: `(session_id: String)`
- **Client Action**: Popover menu removes the card.
- **Ordering on Retention Pruning**: `SessionRemoved` is broadcast strictly after the session is purged from `SessionRegistry`. If the removal alters aggregate counters, `AggregateStateChanged` is broadcast immediately following removal.

---

## 4. Daemon Presence & Reconnection Protocol

### 4.1 Broker Signal Monitoring (`NameOwnerChanged`)
Clients monitor the D-Bus daemon's standard presence signal:
- **Interface**: `org.freedesktop.DBus`
- **Signal**: `NameOwnerChanged(name: String, old_owner: String, new_owner: String)`
- **Target Name**: `org.freedesktop.WatchAI`

#### Client State Transitions:
1. **Daemon Termination** (`new_owner == ""`):
   - Proxy signal handlers are immediately disconnected.
   - Indicator enters dimmed "Offline" styling (`watchai-state-offline`).
   - AT-SPI accessible label updates to `"WatchAI daemon offline"`.
   - Open session cards transition to `[CACHED]` presentation mode.
   - Live duration timers are frozen.
   - Client initiates jittered exponential backoff retries.

2. **Daemon Emergence** (`new_owner != ""`):
   - Any pending backoff retry timer is immediately cancelled.
   - Client begins the atomic 5-step synchronization handshake.

### 4.2 The 5-Step Synchronization Handshake
When the daemon claims its bus name, synchronization proceeds in strictly sequential order:
```text
Step 1: Reacquire Proxy (Gio.DBusProxy for org.freedesktop.WatchAI)
  │
Step 2: GetAggregateState() ──► Updates top-bar indicator state
  │
Step 3: GetSessions()        ──► Replaces cached cards with fresh sessions
  │
Step 4: Attach Signals       ──► Connects AggregateStateChanged & Session*
  │
Step 5: Mark Online          ──► Restores live indicator & unfreezes timers
```

### 4.3 5.0-Second Handshake Deadline & Generation Invalidation
Each reconnection handshake is bounded by a client-side asynchronous timeout of **5.0 seconds** (`HANDSHAKE_TIMEOUT_MS = 5000`) and protected by a monotonic generation token (`_handshakeGeneration`). If the daemon hangs, stalls, or disconnects before completion:
1. Generation token increments, instantly invalidating all in-flight asynchronous callbacks.
2. In-flight proxy references are discarded.
3. UI remains safely in `CACHED / OFFLINE` mode without freezing Mutter.
4. Retry is scheduled with jittered exponential backoff.

### 4.4 Jittered Exponential Backoff
$$\text{Delay} = \min\left(30000\text{ms}, \text{round}\left(\min(30000\text{ms}, 1000\text{ms} \times 2.0^n) \times [0.80, 1.20]\right)\right)$$
- **Initial Interval**: 1.0s (1000ms).
- **Multiplier**: 2.0x per consecutive failure.
- **Hard Ceiling**: 30.0s (30,000ms) enforced after jitter calculation (delays strictly bounded between 800ms and 30,000ms).
- **Jitter**: $\pm 20\%$ randomized variation.
- **Success Reset**: Successfully completing Step 5 resets consecutive failures to 0.

---

## 5. Privacy & Internal Field Invariant

To guarantee absolute compliance with Constitution Principle IV:
- Public `SessionDto` strictly exposes the approved 9-field tuple `(sssssssus)`.
- Internal daemon metadata—specifically `process_start_time` (clock ticks since boot) and `consecutive_proc_failures`—are private in-memory fields and are **never** exposed over the public D-Bus contract.
- No prompts, model completions, source code, git diffs, file contents, command-line arguments, shell output, or credentials are transmitted over D-Bus.

---

## 6. GJS / GNOME Shell Client Pattern

The GNOME Shell extension consumes this interface defensively via `Gio.DBusProxy`:

```javascript
// Native asynchronous instantiation in GNOME Shell ESM with NameOwnerChanged monitoring
const WatchAIProxy = Gio.DBusProxy.makeProxyWrapper(WatchAIDbusInterface);

this._proxy = new WatchAIProxy(
    Gio.DBus.session,
    'org.freedesktop.WatchAI',
    '/org/freedesktop/WatchAI',
    (initProxy, error) => {
        if (!error && initProxy && initProxy.g_name_owner) {
            // Initiate 5-step handshake bounded by 5.0s timeout
            this._runHandshake(initProxy);
        } else {
            this._handleDisconnect();
        }
    }
);
```
