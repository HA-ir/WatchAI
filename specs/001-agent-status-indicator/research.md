# Phase 0: Research & Technology Decisions

**Feature**: Baseline Agent Monitoring & GNOME Shell Indicator (`001-agent-status-indicator`)  
**Date**: 2026-10-03  
**Status**: Completed  

---

## 1. Local IPC Mechanism Evaluation

### Context & Requirements
The WatchAI background daemon must communicate bidirectionally with the GNOME Shell top-bar extension. The IPC channel must support:
- High performance, low latency (<50ms).
- Zero-blocking asynchronous event streaming into the GNOME Shell (GJS / Mutter) main loop.
- User-space isolation (restricting access to the local desktop session user).
- Developer introspection and diagnostics via standard Linux CLI utilities.

### Tradeoff Analysis

| IPC Mechanism | GNOME Shell (GJS) Native Support | Event Broadcasting / Signals | Authentication & Security | Implementation Complexity | Tooling & Debuggability |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **D-Bus Session Bus (`org.freedesktop.WatchAI`)** | **Native** via `Gio.DBusProxy` / `Gio.DBusConnection` (built into GLib/GJS) | **First-class** (`PropertiesChanged`, custom signals) | Handled by D-Bus daemon (SO_PEERCRED, locked to session UID) | Low (declarative XML interface specification) | **Excellent** (`busctl`, `gdbus`, `d-feet`) |
| **Unix Domain Socket (JSON-RPC 2.0)** | Requires manual `Gio.SocketClient` / `Gio.DataInputStream` framing in GJS | Requires custom pub/sub fanout loop in daemon & client | File permissions on `$XDG_RUNTIME_DIR/watchai/ipc.sock` | Moderate (must handle framing, reconnection, buffering) | Moderate (`socat`, `nc -U`, `jq`) |
| **Shared Memory (POSIX shm / mmap)** | Difficult in GJS; lacks reactive signal notifications | None; requires spinlock or secondary signaling channel | File permissions on `/dev/shm` | High (synchronization, locking, serialization) | Poor |

### Decision
**Selected Approach: D-Bus User Session Bus (`org.freedesktop.WatchAI`)**
- **Rationale**:
  1. D-Bus is the native, idiomatic IPC mechanism for GNOME Shell. `Gio.DBusProxy` handles caching, method calls, and signal dispatch natively and asynchronously within the GJS event loop without third-party libraries.
  2. D-Bus handles multi-client broadcasting out of the box. Both the GNOME Shell indicator and any future CLI inspection tool (`watchai-cli`) can subscribe to state signals simultaneously without custom multiplexing in the daemon.
  3. Standard XML introspection allows automated contract testing and verification using `busctl` or `gdbus`.

---

## 2. Daemon Runtime & Language Selection

### Context & Requirements
The central daemon is responsible for:
- Monitoring child processes and filesystem markers across multiple workspaces.
- Managing concurrent state machines with millisecond-level responsiveness.
- Exposing the D-Bus interface.
- Running continuously as a lightweight `systemd --user` background service (<30MB RAM footprint).

### Evaluated Options

1. **Rust (`tokio` + `zbus`)**:
   - *Pros*: Single native binary, zero external runtime dependencies, minimal memory footprint (<15MB), fearless concurrency, best-in-class async D-Bus library (`zbus`), compile-time state-machine guarantees.
   - *Cons*: Longer initial compile times; strict type modeling required up front.
2. **Python 3.12+ (`asyncio` + `sdbus-async` or `dasbus`)**:
   - *Pros*: Rapid iteration, ubiquitous availability on Linux distributions (Debian/Ubuntu/Fedora).
   - *Cons*: Runtime interpreter dependency, higher memory consumption (~35-60MB RAM), GIL considerations, potential packaging version drift.
3. **Go (`godbus`)**:
   - *Pros*: Native binary, simple concurrency.
   - *Cons*: `godbus` has less mature GObject-style property change emission compared to `zbus`.

### Decision
**Selected Approach: Rust with `zbus` and `tokio` (or Python 3.12+ as accessible alternative)**
- **Architecture Choice**: The core daemon is planned as a modular Rust service leveraging `zbus 4.x` for D-Bus and `tokio` for async event processing.
- **Rationale**: Rust's type system natively enforces the FSM invariants mandated by the Constitution (preventing invalid state transitions at compile time), produces a single self-contained binary for Linux packaging, and runs with negligible memory overhead in background desktop environments.

---

## 3. GNOME Shell Extension Architecture (GNOME 45+)

### Context & Requirements
Modern GNOME Shell (versions 45, 46, 47, and beyond) mandates the ESM (ECMAScript Modules) architecture:
- Extensions must use native `import` statements rather than legacy `imports.gi`.
- Top-bar indicators inherit from `PanelMenu.Button`.
- Menus use `PopupMenu.PopupMenuSection` and custom Clutter/St widgets.
- Any blocking I/O freezes the compositor thread (Mutter).

### Key Architectural Patterns
1. **Asynchronous Proxy Initialization**:
   - Use `Gio.DBusProxy.new_for_bus()` with `Gio.DBusProxyFlags.NONE` and an async callback.
   - Handle daemon unavailability gracefully: if the daemon is not running, the indicator renders an inactive/offline badge without errors.
2. **Signal-Driven Reactivity**:
   - Subscribe to `g-signal` on the proxy for `SessionStateChanged` and `AggregateStateChanged`.
   - Update St label and Clutter icon styles reactively on signal receipt.
3. **Strict Cleanup on Disable**:
   - In `disable()`, disconnect all D-Bus signal handlers, cancel pending timers/dwell timeouts, and nullify UI references to prevent GJS memory leaks across extension reloads or lock-screen events.

---

## 4. Provider Adapter Discovery & Integration Strategy

### Context & Requirements
Initial providers: **Claude Code**, **OpenAI Codex CLI**, and **OpenCode**.
Constitutional Invariant: *Do not assume undocumented APIs or private hooks.*

### Discovery & Validation Matrix

| Provider | Baseline Discovery (Unprivileged) | Fine-Grained Event Mechanism | Discovery / Validation Plan |
| :--- | :--- | :--- | :--- |
| **Claude Code** | Process scanning: binary named `claude`, inspect `/proc/[pid]/cmdline` and `/proc/[pid]/cwd`. | Documented hook scripts (e.g., in `.claude/settings.json` or plugin hooks `SessionStart`, `UserPromptSubmit`, `ToolUse`, `Notification`, `Stop`). | Validate hook firing in dedicated test harnesses; implement lightweight hook wrapper that notifies WatchAI via local D-Bus or socket; fallback to process liveness if hooks are not configured. |
| **OpenAI Codex CLI** | Process scanning: binary name `codex` / `codex-cli`, inspect `/proc/[pid]/cmdline` and working directory. | CLI wrapper / wrapper script, or local lockfile/socket if present. | Perform discovery pass to test CLI wrapper interception; mark as `DISCOVERY_REQUIRED` in initial UI until verified. |
| **OpenCode** | Process scanning: binary name `opencode`. | Extension/plugin API or event stream if exposed. | Perform discovery pass against OpenCode repository/docs; mark as `DISCOVERY_REQUIRED` initially. |

### Architectural Safeguard: Tiered Adapter Model
1. **Tier 1 (Process Liveness)**: Every adapter implements `/proc` binary and workspace detection. If the agent binary is running in a workspace, it is detected as a valid session in state `WORKING` or `IDLE`.
2. **Tier 2 (Event-Driven Telemetry)**: If verified hooks or event streams are installed and active, the adapter upgrades the session to fine-grained states (`WAITING`, `STARTING`, `SUCCESS`, `ERROR`).
3. **Tier 3 (Unverified / Discovery Required)**: If an adapter detects a process but lacks fine-grained hook confirmation, it sets `adapter_status = DISCOVERY_REQUIRED`.

---

## 5. systemd --user Lifecycle & Process Resilience

### Service Definition Pattern
- Service file: `watchai.service` installed to `~/.config/systemd/user/watchai.service` (or `/usr/lib/systemd/user/watchai.service`).
- `Type=dbus`
- `BusName=org.freedesktop.WatchAI`
- `ExecStart=/usr/bin/watchai-daemon`
- `Restart=on-failure`
- `RestartSec=2s`
- `Slice=session.slice`

### Failure & Restart Recovery Matrix

| Failure Event | Impact on Daemon | Impact on GNOME Shell Extension | Recovery Behavior |
| :--- | :--- | :--- | :--- |
| **Daemon process crashes** | Process exits abnormally; systemd restarts it in 2s. | D-Bus proxy receives `name-owner-changed` signal indicating owner became empty; UI transitions to `OFFLINE`. | When daemon re-registers `org.freedesktop.WatchAI`, proxy detects new owner, calls `GetState()`, and restores active indicator state immediately. |
| **GNOME Shell restarts (`Alt+F2 r` or Wayland logout)** | Daemon continues running uninterrupted in systemd user session. | Extension is torn down (`disable()`) and re-instantiated (`enable()`). | New extension instance connects to live D-Bus session bus, retrieves current aggregate and session states, and renders instantly. |
| **Monitored Agent crashes (`kill -9`)** | Telemetry halts abruptly. | Unaware until daemon signals. | Daemon's `/proc` liveness checker detects dead PID within 5 seconds; transitions session state to `ERROR`/`CANCELLED`; emits D-Bus signal; extension updates. |

---

## 6. Accessibility & Non-Color Semantics

### Standardized Visual & Semantic Mapping

| State | Color Token | Symbolic Icon (Freedesktop/GNOME naming) | Accessible Description (AT-SPI) |
| :--- | :--- | :--- | :--- |
| **IDLE** | Dim / Neutral Gray | `system-run-symbolic` (dimmed) / `user-idle-symbolic` | "WatchAI: No active coding agents" |
| **STARTING** | Blue / Cyan | `process-working-symbolic` | "WatchAI: Agent session initializing" |
| **WORKING** | Amber / Yellow | `media-playback-start-symbolic` / pulsating dot | "WatchAI: Agent actively executing work" |
| **WAITING** | Bold Red / Orange | `dialog-warning-symbolic` / `action-unavailable-symbolic` | "WatchAI: Agent blocked waiting for user approval" |
| **SUCCESS** | Vibrant Green | `emblem-ok-symbolic` / `object-select-symbolic` | "WatchAI: Agent task completed successfully" |
| **ERROR** | Crimson Red (alert badge) | `dialog-error-symbolic` / `software-update-urgent-symbolic` | "WatchAI: Agent encountered an unrecoverable error" |
| **CANCELLED** | Neutral Slate | `process-stop-symbolic` | "WatchAI: Agent session cancelled" |
| **UNKNOWN** | Muted Violet / Gray | `dialog-question-symbolic` | "WatchAI: Agent status unverified" |
