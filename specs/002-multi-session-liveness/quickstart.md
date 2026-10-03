# Quickstart Validation Guide: Multi-Session Aggregation & Liveness

**Feature**: Phase 6 — Multi-Session Aggregation & Liveness (`002-multi-session-liveness`)  
**Date**: 2026-10-04  
**Status**: Verified (Passing) — Validated by `tests/quickstart_scenarios_test.rs`  

---

## 1. Prerequisites
- Linux development environment with Rust toolchain (`cargo test`).
- Running desktop session (or headless D-Bus test harness `dbus-run-session`).
- D-Bus inspection tools (`busctl`, `gdbus`).

---

## 2. Validation Scenarios

### Scenario 1: Multi-Session Priority Resolution
Verify that when multiple concurrent agent sessions exist, the aggregate state reflects the highest priority state and accurately tracks session counters.

1. **Setup**:
   - Start session S1 in `WORKING`.
   - Start session S2 in `WAITING`.
   - Start session S3 in `SUCCESS`.
2. **Execute Query**:
   ```bash
   gdbus call --session \
     --dest org.freedesktop.WatchAI \
     --object-path /org/freedesktop/WatchAI \
     --method org.freedesktop.WatchAI.GetAggregateState
   ```
3. **Assert Outcome**:
   - Aggregate state is `WAITING` (because $\text{WAITING} (70) > \text{WORKING} (60) > \text{SUCCESS} (30)$).
   - `active_session_count` = 3 (all 3 within active/dwell window).
   - `waiting_session_count` = 1.
   - `error_session_count` = 0.

---

### Scenario 2: 10-Second Aggregate Dwell vs. 60-Second Session Retention
Prove that after all sessions complete, the top-bar aggregate state resets to `IDLE` after 10 seconds while the session card remains visible in the popover for 60 seconds.

1. **Setup**:
   - Session S1 finishes and transitions to `SUCCESS`.
2. **At T = 5 seconds**:
   - `GetAggregateState()` returns `state = 'SUCCESS'`, `active_session_count = 1`.
   - `GetSessions()` returns S1 with `current_state = 'SUCCESS'`.
3. **At T = 12 seconds**:
   - 10-second dwell has expired!
   - `GetAggregateState()` returns `state = 'IDLE'`, `active_session_count = 0`.
   - `GetSessions()` returns S1 still with `current_state = 'SUCCESS'` (retained in popover).
4. **At T = 65 seconds**:
   - 60-second retention has expired!
   - `GetSessions()` returns 0 sessions (pruned).
   - `SessionRemoved("S1")` signal emitted.

---

### Scenario 3: Ungraceful Process Crash Detection via 2-Cycle Hysteresis
Verify that abruptly terminating an active agent process (`kill -9`) is detected within 5 seconds without relying on clean exit signals.

1. **Setup**:
   - Launch dummy sleep process:
     ```bash
     sleep 600 &
     PID=$!
     ```
   - Register session with PID.
2. **Action**:
   ```bash
   kill -9 $PID
   ```
3. **Assert Outcome**:
   - Within 2 consecutive liveness cycles (2–4 seconds, bounded by 5s max):
     - The daemon detects `/proc/[pid]` disappearance.
     - Session transitions to `ERROR`.
     - `SessionUpdated` and `AggregateStateChanged` signals are emitted.

---

### Scenario 4: Linux PID Reuse Protection
Prove that an unrelated process recycling the same PID is not hijacked as the original agent session.

1. **Setup**:
   - Record session with PID $P$ and recorded start time $T_1$.
   - Simulate process exit of $P$.
   - A new unrelated process spawns with recycled PID $P$ and start time $T_2 \ne T_1$.
2. **Execute Liveness Check**:
   - Daemon reads `/proc/[P]/stat` field 22.
   - Detects $T_2 \ne T_1$.
3. **Assert Outcome**:
   - The daemon immediately confirms original session death.
   - Session transitions to `ERROR`.
   - The new process is NOT adopted.

---

### Scenario 5: Daemon Restart Recovery with Stable Deterministic Session IDs
Verify that surviving agent processes are rediscovered with identical session IDs after daemon restart.

1. **Setup**:
   - Agent process running with PID $P$, start time $T$, working directory `/project`.
   - Initial session ID is `SHA256(P + T + /project)[0..16]`.
2. **Action**:
   - Stop and restart `watchai-daemon`.
3. **Assert Outcome**:
   - Immediate discovery sweep finds process $P$.
   - Generates exact same session ID `SHA256(P + T + /project)[0..16]`.
   - Registers session in `IDLE` (`DISCOVERY_REQUIRED`).
   - Zero duplicate sessions and zero false `ERROR` transitions.
