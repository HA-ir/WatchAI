# Quickstart Validation Guide: Crash & Recovery

**Feature**: Phase 7 — Crash & Recovery (`003-crash-recovery`)  
**Date**: 2026-10-04  
**Status**: Draft  

---

## 1. Prerequisites
- Linux development environment with Rust toolchain (`cargo test`).
- GNOME Shell test environment (or headless GJS runner `gjs -m`).
- D-Bus inspection tools (`busctl`, `gdbus`).

---

## 2. Validation Scenarios

### Scenario 1: Daemon Ungraceful Crash & Process Rediscovery
Prove that when `watchai-daemon` crashes abruptly (`kill -9`), surviving agent processes are rediscovered with identical deterministic IDs without false `ERROR` alerts.

1. **Setup**:
   - Launch background mock process:
     ```bash
     sleep 600 &
     PID=$!
     ```
   - Register session with PID, start time $T$, project path `/workspace/app`.
   - Session ID is `SHA256(PID + T + /workspace/app)[0..16]`.
2. **Action**:
   - Terminate daemon abruptly: `kill -9 $(pidof watchai-daemon)`
3. **Restart Daemon**:
   - Relaunch daemon: `watchai-daemon`
4. **Assert Outcome**:
   - Discovery sweep detects PID.
   - Reconstructs exact same `session_id`.
   - Registers session with `current_state = IDLE` and `adapter_status = DiscoveryRequired`.
   - Zero `ERROR` transitions or alerts are emitted.

---

### Scenario 2: GNOME Shell Disconnect & Popover Cache Preservation
Verify that when the daemon disappears, open popover cards are preserved in a `CACHED` state and duration timers pause.

1. **Setup**:
   - Open GNOME Shell popover showing active session card in `WORKING` with running duration timer.
2. **Action**:
   - Stop daemon: `systemctl --user stop watchai` or kill daemon process.
3. **Assert Outcome**:
   - Top-bar indicator dims to "Offline" style.
   - AT-SPI accessible description updates to "WatchAI daemon offline".
   - Popover card displays `[CACHED]` status badge.
   - Live duration timer pauses; elapsed duration does not increment while offline.
   - No fake state transitions or crashes occur in GNOME Shell.

---

### Scenario 3: Automatic Reconnection with 5-Step Handshake
Prove that when the daemon returns, the extension automatically executes the 5-step handshake bounded by a 5.0-second timeout and restores authoritative state.

1. **Setup**:
   - Extension is in `OFFLINE/CACHED` state from Scenario 2.
2. **Action**:
   - Restart daemon: `watchai-daemon`
3. **Assert Outcome**:
   - Extension detects `NameOwnerChanged` signal for `org.freedesktop.WatchAI`.
   - Initiates asynchronous handshake bounded by 5.0s timeout.
   - Reacquires `Gio.DBusProxy`, queries `GetAggregateState()` and `GetSessions()`, attaches signal listeners.
   - Top-bar indicator restores live color icon.
   - Cached cards in popover are replaced with live cards; duration timers resume ticking.

---

### Scenario 4: Jittered Exponential Backoff During Crash Loops
Verify that repeated daemon crashes trigger exponential backoff without flooding D-Bus or generating notification popups.

1. **Setup**:
   - Simulate rapid daemon startup crashes (daemon starts, immediately panics, and exits).
2. **Assert Outcome**:
   - Extension retry intervals double: 1.0s $\rightarrow$ 2.0s $\rightarrow$ 4.0s $\rightarrow$ 8.0s $\rightarrow$ 16.0s $\rightarrow$ 30.0s (ceiling) with $\pm 20\%$ jitter.
   - D-Bus is not flooded with rapid connection spam.
   - Zero desktop notification popups are emitted.
   - Extension remains stable in Mutter without memory leaks.

---

### Scenario 5: Activity Recovery from Post-Restart IDLE
Verify that when an agent emits fresh telemetry after daemon restart, it transitions from `IDLE` to `WORKING`.

1. **Setup**:
   - Agent process recovered after restart in `IDLE` (`DISCOVERY_REQUIRED`).
2. **Action**:
   - Provider adapter delivers fresh telemetry event: `state = WORKING`.
3. **Assert Outcome**:
   - Session transitions from `IDLE` $\rightarrow$ `WORKING`.
   - `SessionUpdated` D-Bus signal broadcast.
   - Aggregate state updates to `WORKING`.
   - Popover card displays live amber `WORKING` badge.
