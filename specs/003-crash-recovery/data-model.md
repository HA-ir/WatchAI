# Data Model & State Transitions: Crash & Recovery

**Feature**: Phase 7 — Crash & Recovery (`003-crash-recovery`)  
**Date**: 2026-10-04  
**Status**: Draft  

---

## 1. Client Connection State Machine (GNOME Shell Extension)

To prevent compositor freezes and guarantee clean resource disposal, the extension's D-Bus client operates as an explicit 4-state state machine:

```text
       ┌──────────────┐
       │ Disconnected │◄────────────────────────────────────────┐
       └──────┬───────┘                                         │
              │ initialize / enable()                           │
              ▼                                                 │
       ┌──────────────┐   Handshake Failed / Timeout (5s)       │
       │  Connecting  ├─────────────────────────────────────────┤
       └──────┬───────┘                                         │
              │ Handshake Success (<5s)                         │
              ▼                                                 │
       ┌──────────────┐   NameOwnerChanged (Owner = "")         │
       │  Connected   ├────────────────────────┐                │
       └──────┬───────┘                        ▼                │
              │                         ┌──────────────┐        │
              │ disable()               │ Reconnecting │────────┘
              ▼                         └──────┬───────┘  Handshake Failed (backoff timer)
       ┌──────────────┐                        │
       │   Disabled   │                        ▼
       └──────────────┘                [Exponential Backoff]
                                       (1.0s -> 2.0s -> ... -> 30.0s)
```

### 1.1 Client Connection States

| State | UI Representation | Timers & Handlers | Description |
| :--- | :--- | :--- | :--- |
| `Disconnected` | Top-bar dimmed "Offline"; popover shows empty offline banner. | All D-Bus proxies and signal listeners dropped. | Daemon is absent; extension is idle or waiting for initial probe. |
| `Connecting` | Top-bar dimmed "Connecting..."; popover shows connecting spinner. | 5.0-second handshake timeout timer active. | Attempting initial asynchronous D-Bus proxy acquisition and state fetch. |
| `Connected` | Top-bar displays live aggregate icon/badge; popover shows live cards. | Live 1s duration timers active; 4 D-Bus signal listeners connected. | Normal operation: live telemetry and signal dispatch active. |
| `Reconnecting` | Top-bar dimmed "Offline"; popover preserves cached cards (`CACHED`). | Live duration timers paused; backoff retry timer active; signal listeners dropped. | Daemon vanished; extension is retrying with exponential backoff and jitter. |

---

## 2. Reconnection Backoff Model

### 2.1 Parameters & Formulas

| Parameter | Type | Value | Description |
| :--- | :--- | :--- | :--- |
| `INITIAL_INTERVAL_MS` | `u32` | `1000` (1.0s) | Initial backoff delay before first retry attempt. |
| `MULTIPLIER` | `f64` | `2.0` | Exponential scaling factor on consecutive failures. |
| `MAX_INTERVAL_MS` | `u32` | `30000` (30.0s) | Absolute ceiling for retry delay. |
| `JITTER_RATIO` | `f64` | `0.20` ($\pm 20\%$) | Uniform random factor to prevent synchronized client bursts. |
| `HANDSHAKE_TIMEOUT_MS` | `u32` | `5000` (5.0s) | Client-side timeout for each handshake operation. |

### 2.2 Interval Calculation Formula

Let $n$ be the number of consecutive failed handshake attempts ($n \ge 0$):

$$\text{BaseInterval}(n) = \min(\text{MAX\_INTERVAL\_MS}, \text{INITIAL\_INTERVAL\_MS} \times 2.0^n)$$

$$\text{JitterFactor} \sim \text{Uniform}(1.0 - \text{JITTER\_RATIO}, 1.0 + \text{JITTER\_RATIO}) = [0.80, 1.20]$$

$$\text{NextRetryDelay}(n) = \text{round}(\text{BaseInterval}(n) \times \text{JitterFactor})$$

Upon successful handshake completion ($n \rightarrow 0$), `BaseInterval` immediately resets to `INITIAL_INTERVAL_MS` (1000ms).

---

## 3. Popover Card Presentation Modes

Each session card widget in the popover operates in one of two presentation modes:

```text
┌────────────────────────────────────────────────────────┐
│ Mode: LIVE                                             │
│ [Claude Code] my-project                               │
│ State: WORKING (amber)        Duration: 02:45 (ticking)│
└────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────┐
│ Mode: CACHED (Offline)                                 │
│ [Claude Code] my-project                               │
│ State: WORKING [CACHED]       Duration: 02:45 (PAUSED) │
└────────────────────────────────────────────────────────┘
```

### Mode Semantics
1. **`LIVE`**:
   - Rendered when `ConnectionState == Connected`.
   - Card displays full color badge corresponding to active FSM state.
   - Duration timer increments elapsed time every 1.0 second.
   - Card responds dynamically to incoming `SessionUpdated` and `SessionRemoved` signals.
2. **`CACHED`**:
   - Rendered when `ConnectionState == Reconnecting` or `Disconnected`.
   - Card displays a dimmed/desaturated state badge with an explicit `[CACHED]` tag.
   - Duration timer is frozen at the moment of disconnect.
   - No fake state changes or completion events are generated.

---

## 4. Recovery Discovery Ordering & Sorting

When `watchai-daemon` executes its startup recovery sweep over `/proc`, discovered processes are inserted into the `SessionRegistry` in a strictly deterministic order:

```text
Primary Sort:    provider_id ASC      ("claude-code" before "codex-cli")
Secondary Sort:  project_path ASC     ("/home/user/app1" before "/home/user/app2")
Tertiary Sort:   process_id ASC       (PID 1001 before PID 1002)
```

This ensures that regardless of the non-deterministic order in which the kernel returns entries from `fs::read_dir("/proc")`, the initial registry snapshot, surrogate key evaluations, and D-Bus properties remain byte-for-byte reproducible.

---

## 5. Epistemic Truth Table Across Restarts

| State Before Daemon Crash | Discovered in `/proc` on Restart? | Post-Restart State Assigned | Adapter Status Assigned | Rationale |
| :--- | :---: | :--- | :--- | :--- |
| `STARTING` | **YES** | `IDLE` | `DiscoveryRequired` | Process exists, but bootstrapping telemetry was lost during crash. |
| `WORKING` | **YES** | `IDLE` | `DiscoveryRequired` | Process exists, but active tool/model execution cannot be verified without telemetry. |
| `WAITING` | **YES** | `IDLE` | `DiscoveryRequired` | Process exists, but blocked prompt state cannot be inferred without telemetry. |
| `IDLE` | **YES** | `IDLE` | `DiscoveryRequired` | Process exists at prompt; matches physical observation. |
| Any Active State | **NO** (died while down) | **NOT REGISTERED** | **N/A** | Process terminated; zero records created; no false crash alerts. |
| `SUCCESS` (in dwell) | **YES** | `IDLE` | `DiscoveryRequired` | Process returned to prompt after task; survives as prompt-ready agent. |
| `SUCCESS` (in dwell) | **NO** (exited) | **NOT REGISTERED** | **N/A** | Clean completion followed by process exit; no post-crash resurrection. |
