# Contract Confirmation: D-Bus Local IPC Interface (`org.freedesktop.WatchAI`)

**Feature**: Phase 6 — Multi-Session Aggregation & Liveness (`002-multi-session-liveness`)  
**Status**: 100% Backward-Compatible (No Breaking Changes)  
**Bus Name**: `org.freedesktop.WatchAI`  
**Object Path**: `/org/freedesktop/WatchAI`  

---

## 1. Compatibility Confirmation
Phase 6 introduces advanced multi-session aggregation, liveness checking, and retention pruning **entirely within the core daemon runtime engine**. 

Inspection of the existing `org.freedesktop.WatchAI` interface implemented in Phase 3 and consumed by GNOME Shell in Phase 5 confirms that **zero contract changes or signature adjustments are required**:
- `process_start_time` and `consecutive_proc_failures` remain private internal runtime fields and are deliberately excluded from public D-Bus structs.
- `SessionDto` retains its exact `(sssssssus)` signature.
- `GetAggregateState` retains its exact `(state: s, active: u, waiting: u, error: u, updated_at: s)` signature.

---

## 2. Signal Emission Conditions

### 2.1 `AggregateStateChanged`
- **Emission Trigger**: Emitted when any of the following 4 values changes:
  1. `state` (e.g. from `IDLE` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING`).
  2. `active_session_count`.
  3. `waiting_session_count`.
  4. `error_session_count`.
- **Suppression Invariant**: If an individual session transitions or touches its heartbeat without altering the computed aggregate state or any of the 3 counters, `AggregateStateChanged` **MUST NOT** be emitted. This prevents unnecessary D-Bus traffic and GNOME top-bar redraws.

### 2.2 `SessionUpdated`
- **Emission Trigger**: Emitted when a session transitions state (e.g. `WORKING` $\rightarrow$ `WAITING`, or `WORKING` $\rightarrow$ `ERROR` upon crash detection) or updates its active tool category.
- **Ordering**: If a session transitions to `ERROR` upon process death, `SessionUpdated` is emitted first, followed immediately by `AggregateStateChanged`.

### 2.3 `SessionRemoved`
- **Emission Trigger**: Emitted strictly when a terminal session's 60-second retention timer expires and the session is pruned from `SessionRegistry`.
- **Ordering**: When a session is pruned, `SessionRemoved(session_id)` is emitted. If the pruned session was altering any count, `AggregateStateChanged` is emitted after removal.
