# Quickstart Validation Guide: WatchAI Baseline Agent Monitoring

**Feature**: Baseline Agent Monitoring & GNOME Shell Indicator (`001-agent-status-indicator`)  
**Date**: 2026-10-03  
**Status**: Draft  

---

## 1. Prerequisites & Environment Setup

### Required Tools & System Packages
- **Linux Distribution**: Ubuntu 24.04 LTS, Fedora 40+, or Debian 12 running GNOME Shell (45+).
- **Desktop Session**: X11 or Wayland user session.
- **Developer Tools**:
  - D-Bus inspection tools: `dbus-tools` / `gdbus` / `busctl`.
  - GNOME Shell extension tools: `gnome-extensions-app`, `gnome-shell`.
  - Rust toolchain (`cargo`, `rustc` 1.80+) for daemon building.

---

## 2. Validation Scenarios

### Scenario 1: Daemon Startup & D-Bus Registration Check
Verify that the daemon successfully registers the well-known D-Bus bus name on the user session bus.

1. **Start the daemon locally**:
   ```bash
   cargo run --bin watchai-daemon
   ```
2. **Verify D-Bus service ownership in another terminal**:
   ```bash
   busctl --user status org.freedesktop.WatchAI
   ```
   *Expected Output*: Service is active, PID matches daemon, owned by the active user.
3. **Query current aggregate state**:
   ```bash
   gdbus call --session \
     --dest org.freedesktop.WatchAI \
     --object-path /org/freedesktop/WatchAI \
     --method org.freedesktop.WatchAI.GetAggregateState
   ```
   *Expected Output*: `('IDLE', 0, 0, 0, '<ISO_TIMESTAMP>')`

---

### Scenario 2: Mock Agent Session Lifecycle & State Transitions
Verify that simulated agent lifecycle events trigger correct state transitions and emit D-Bus signals.

1. **Monitor D-Bus signals in real time**:
   ```bash
   busctl --user monitor org.freedesktop.WatchAI
   ```
2. **Trigger mock session transitions (via development test CLI)**:
   ```bash
   # Start mock agent session
   cargo run --bin watchai-mock -- session start --provider claude-code --project my-app
   # Transition to WORKING
   cargo run --bin watchai-mock -- session transition --state WORKING
   # Transition to WAITING (requires approval)
   cargo run --bin watchai-mock -- session transition --state WAITING
   # Transition to SUCCESS
   cargo run --bin watchai-mock -- session transition --state SUCCESS
   ```
3. **Verify Signal Emission**:
   - `busctl` logs show `SessionAdded`, followed by `SessionUpdated` events.
   - `AggregateStateChanged` fires on every transition, updating state from `STARTING` $\rightarrow$ `WORKING` $\rightarrow$ `WAITING` $\rightarrow$ `SUCCESS`.
   - After 10 seconds of dwell time in `SUCCESS`, state resets to `IDLE`.

---

### Scenario 3: Multi-Session Priority Aggregation
Prove that a session in `WAITING` or `ERROR` takes precedence over concurrent `WORKING` sessions.

1. **Spawn concurrent sessions**:
   ```bash
   cargo run --bin watchai-mock -- session start --id session-1 --state WORKING
   cargo run --bin watchai-mock -- session start --id session-2 --state WAITING
   ```
2. **Query aggregate state**:
   ```bash
   gdbus call --session \
     --dest org.freedesktop.WatchAI \
     --object-path /org/freedesktop/WatchAI \
     --method org.freedesktop.WatchAI.GetAggregateState
   ```
   *Expected Output*: `('WAITING', 2, 1, 0, '...')` — verifying `WAITING` overrides `WORKING`.
3. **Introduce an error in Session 1**:
   ```bash
   cargo run --bin watchai-mock -- session transition --id session-1 --state ERROR
   ```
   *Expected Output*: Aggregate state immediately transitions to `ERROR` (`('ERROR', 2, 1, 1, '...')`).

---

### Scenario 4: Process Crash & Liveness Timeout Detection
Verify that dead or abruptly killed agent processes are detected within 5 seconds without freezing.

1. **Start a dummy sleep process to simulate an agent**:
   ```bash
   sleep 300 &
   MOCK_PID=$!
   cargo run --bin watchai-mock -- session start --id mock-crash --pid $MOCK_PID --state WORKING
   ```
2. **Abruptly kill the process**:
   ```bash
   kill -9 $MOCK_PID
   ```
3. **Observe Daemon Detection**:
   - Within 5 seconds, the daemon's `/proc` monitor detects PID disappearance.
   - `SessionUpdated` is emitted transitioning `mock-crash` to `ERROR` or `CANCELLED`.
   - Aggregate state transitions accordingly.

---

### Scenario 5: GNOME Shell Extension End-to-End Test
Verify visual presentation, indicator icons, popover menu cards, and reconnection resilience.

1. **Link extension into local GNOME extensions directory**:
   ```bash
   mkdir -p ~/.local/share/gnome-shell/extensions/watchai@gnome.org
   cp -r extension/* ~/.local/share/gnome-shell/extensions/watchai@gnome.org/
   gnome-extensions enable watchai@gnome.org
   ```
2. **Visual Inspection**:
   - Top-bar indicator shows a dim neutral icon when daemon is idle.
   - Running mock transitions updates icon color and symbol:
     - `WORKING` $\rightarrow$ Amber/Yellow icon.
     - `WAITING` $\rightarrow$ Bold Red icon with alert symbol.
     - `SUCCESS` $\rightarrow$ Green icon.
   - Clicking the indicator opens the popover menu with session cards, elapsed time counters, and provider labels.
3. **Restart Resilience**:
   - Restart the daemon (`cargo run --bin watchai-daemon`). Verify the extension reconnects seamlessly without throwing GJS errors.
