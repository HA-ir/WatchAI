# Contract Confirmation: D-Bus Local IPC Interface (`org.freedesktop.WatchAI`)

**Feature**: Phase 7 — Crash & Recovery (`003-crash-recovery`)  
**Status**: 100% Backward-Compatible (No Breaking Changes)  
**Bus Name**: `org.freedesktop.WatchAI`  
**Object Path**: `/org/freedesktop/WatchAI`  

---

## 1. Compatibility Confirmation

Phase 7 defines crash resilience, client reconnection, and fault isolation without modifying any existing public D-Bus method signatures, signal signatures, or DTO serialization formats.

- `SessionDto` strictly preserves the canonical 9-tuple signature `(sssssssus)`.
- `AggregateStateDto` strictly preserves `(state: s, active: u, waiting: u, error: u, updated_at: s)`.
- All methods and signals defined in Phases 3, 5, and 6 remain unchanged.

---

## 2. D-Bus Bus Daemon Lifecycle Monitoring

The GNOME Shell extension monitors daemon availability using standard D-Bus broker signals provided by the message bus itself (`org.freedesktop.DBus`):

### Signal: `org.freedesktop.DBus.NameOwnerChanged`
- **Sender**: `org.freedesktop.DBus`
- **Path**: `/org/freedesktop/DBus`
- **Signature**: `(name: String, old_owner: String, new_owner: String)`
- **Match Filter**: `name == "org.freedesktop.WatchAI"`

### Client Transition Rules
1. **Daemon Departure**:
   - Condition: `name == "org.freedesktop.WatchAI" && new_owner == ""`
   - Action: Extension drops proxy, pauses card timers, transitions UI to `OFFLINE/CACHED` mode, and starts the exponential backoff reconnection timer.
2. **Daemon Arrival**:
   - Condition: `name == "org.freedesktop.WatchAI" && new_owner != ""`
   - Action: Extension immediately cancels pending backoff sleep and initiates the 5-step reconnection handshake.

---

## 3. Reconnection Handshake Protocol

When the daemon becomes available, the client executes an atomic synchronization sequence bounded by a **5.0-second client-side timeout**:

```text
Client (GNOME Shell)                                Server (watchai-daemon)
       │                                                       │
       │─── 1. Reacquire Gio.DBusProxy ───────────────────────►│
       │◄── Proxy Ready ───────────────────────────────────────│
       │                                                       │
       │─── 2. Call GetAggregateState() ──────────────────────►│
       │◄── (state, active, waiting, error, updated_at) ───────│
       │                                                       │
       │─── 3. Call GetSessions() ────────────────────────────►│
       │◄── Array of SessionDto (sssssssus) ───────────────────│
       │                                                       │
       │─── 4. Connect Signals (AggregateStateChanged, etc.) ──│ (Local match setup)
       │                                                       │
       │─── 5. Transition UI to LIVE & Unpause Timers ─────────│ (Client state confirmed)
```

### Timeout & Failure Handling
- If any asynchronous call exceeds 5.0 seconds or rejects with a D-Bus error (e.g. `ServiceUnknown`, `Timeout`, `Disconnected`), the handshake is aborted.
- The client drops the half-formed proxy, leaves the UI in `OFFLINE/CACHED` mode, and schedules a retry using the jittered exponential backoff formula.
- The GNOME Shell main loop is **never** blocked synchronously during any part of this handshake.

---

## 4. Privacy & Whitelist Invariant

No crash recovery telemetry or diagnostics transmitted over D-Bus shall contain:
- Prompt text, model instructions, or completions
- Source code, file diffs, or repository contents
- Process command-line arguments or environment variables
- API keys, tokens, or credentials
