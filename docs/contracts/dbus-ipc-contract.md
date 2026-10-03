# Contract: D-Bus Local IPC Interface (`org.freedesktop.WatchAI`)

**Specification**: Baseline Agent Monitoring & GNOME Shell Indicator (`001-agent-status-indicator`)  
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

- **Direction**: In: None $\rightarrow$ Out: `(state: String, active_session_count: u32, waiting_session_count: u32, error_session_count: u32, updated_at: String)`
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
- **Payload**: `(state: String, active_session_count: u32, waiting_session_count: u32, error_session_count: u32, updated_at: String)`
- **Client Action**: GNOME Shell extension updates the top-bar icon, badge, and accessibility description.
- **Throttling Invariant**: Emitted **only** when `state`, `active_session_count`, `waiting_session_count`, or `error_session_count` changes. Intermediate heartbeats, timestamp updates, and background liveness ticks that leave the state and counters unchanged are suppressed to prevent desktop compositor redraw storms.

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

## 4. Privacy & Internal Field Invariant

To guarantee absolute compliance with Constitution Principle IV:
- Public `SessionDto` strictly exposes the approved 9-field tuple `(sssssssus)`.
- Internal daemon metadata—specifically `process_start_time` (clock ticks since boot) and `consecutive_proc_failures`—are private in-memory fields and are **never** exposed over the public D-Bus contract.
- No prompts, model completions, source code, git diffs, file contents, command-line arguments, shell output, or credentials are transmitted over D-Bus.

---

## 5. GJS / GNOME Shell Client Pattern

The GNOME Shell extension consumes this interface via `Gio.DBusProxy`:

```javascript
// Native asynchronous instantiation in GNOME Shell ESM
const WatchAIProxy = Gio.DBusProxy.makeProxyWrapper(`
<node>
  <interface name="org.freedesktop.WatchAI">
    <!-- Introspection XML -->
  </interface>
</node>
`);

// Connect asynchronously without blocking Mutter
this._proxy = new WatchAIProxy(
    Gio.DBus.session,
    'org.freedesktop.WatchAI',
    '/org/freedesktop/WatchAI',
    (initProxy, error) => {
        if (!error) {
            this._proxy.connectSignal('AggregateStateChanged', (proxy, sender, params) => {
                this._onAggregateStateChanged(...params);
            });
        }
    }
);
```
