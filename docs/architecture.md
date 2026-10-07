# WatchAI Architecture Overview

WatchAI is a Linux desktop system designed to monitor local AI coding agents (Claude Code, OpenAI Codex CLI, OpenCode, and others) and present their real-time lifecycle states in the GNOME Shell top bar.

## Four-Tier Layered Architecture

1. **Provider Adapters (`crates/watchai-adapters`)**:
   - Detects active local agent sessions via non-invasive `/proc` scanning and opt-in event hooks.
   - Normalizes vendor-specific telemetry into generic `SessionLifecycleEvent` records.
   - Strictly enforces data sanitization: no prompts, code, tokens, or credentials enter the core domain.
   - Decoupled from the daemon via `AdapterRegistry`.

2. **Core Domain & State Machine (`crates/watchai-core`)**:
   - Implements the 8-state deterministic finite-state machine (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`).
   - Resolves multi-session priority aggregation ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$).
   - Performs hybrid process liveness tracking (`/proc` PID checks + adaptive silence timeouts).
   - Manages in-memory volatile session storage (zero session data on disk).

3. **Local IPC Layer (`crates/watchai-ipc`)**:
   - Exposes the `org.freedesktop.WatchAI` interface on the D-Bus user session bus.
   - Provides methods for aggregate and individual session inspection.
   - Broadcasts asynchronous signals (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`).

4. **GNOME Shell Extension (`extension/`)**:
   - ESM JavaScript extension for GNOME Shell (45, 46, 47).
   - Top-bar indicator button with symbolic icons, color styles, and AT-SPI accessibility descriptions.
   - Interactive popover menu detailing active sessions.
   - Strictly non-blocking: communicates with the daemon via asynchronous `Gio.DBusProxy`.

## Multi-Session Priority Aggregation

When multiple agent sessions exist simultaneously across repositories or providers, WatchAI computes a single desktop aggregate state using a strict mathematical total order:

$$\text{ERROR} (80) > \text{WAITING} (70) > \text{WORKING} (60) > \text{STARTING} (50) > \text{CANCELLED} (40) > \text{SUCCESS} (30) > \text{UNKNOWN} (20) > \text{IDLE} (10)$$

### Aggregation Semantics
- **Zero Sessions**: Resolves to `IDLE` with all counters set to 0.
- **Single Session**: Reflects the session's active state.
- **Multiple Sessions**: Selects the state with the maximum effective priority score.
- **Deterministic Tie-Breaking**: When multiple sessions share the highest priority, contextual single-session selection chooses the session with the most recent `state_entered_at` timestamp, broken lexicographically by `session_id`.

### Decoupled Dwell vs. Retention Lifecycles
1. **60-Second Aggregate Completion Dwell**:
   - Completed sessions (`SUCCESS`, `CANCELLED`, `ERROR`) participate in the aggregate priority calculation for exactly 60 seconds after completion.
   - Once 60 seconds elapse, their effective priority score drops to 0, allowing the top-bar indicator to settle smoothly to `IDLE` (if no other sessions are active).
2. **60-Second Popover Session Retention**:
   - The session record in memory does **not** prematurely mutate to `IDLE`. Live interactive sessions remain visible and settle to `IDLE` for the next turn, while terminated sessions remain in `SUCCESS`, `CANCELLED`, or `ERROR` for 60 seconds so the user can inspect the outcome card in the popover menu.
   - Exactly 60 seconds after terminal entry, terminated sessions are purged from the registry and a `SessionRemoved` signal is broadcast.

## Process Liveness & PID Reuse Defense

- **Periodic Liveness Check**: Daemon inspects `/proc/[pid]/stat` every 2 seconds for all sessions with an assigned PID.
- **PID Recycling Protection**: Compares field 22 (`starttime` in clock ticks since boot) against the session's recorded start time. An unequal start time indicates the OS recycled the PID for an unrelated process; the session immediately transitions to `ERROR`.
- **Two-Consecutive-Failure Hysteresis**: Process death is confirmed only after 2 consecutive failed `/proc` reads (spanning 2.0 to 4.0 seconds, bounded by 5s max) before transitioning to `ERROR`. A single transient read failure is safely tolerated and resets to 0 on any subsequent successful read.

## Activity Knowledge Boundary

Observing a process in `/proc` proves OS process existence, **not** active task execution.
- Discovered processes without verified event telemetry are registered as `IDLE` with `adapter_status = AdapterStatus::DiscoveryRequired`.
- Active execution states (`WORKING`, `WAITING`) strictly require telemetry events from provider adapters.
- Unmonitored active sessions experience adaptive silence timeouts: sessions in `WORKING` transition to `UNKNOWN` after 300 seconds of silence, while `STARTING` sessions transition to `UNKNOWN` after 60 seconds.
- An unmonitored session recovering from `UNKNOWN` transitions back to `WORKING` only upon receipt of a valid telemetry event.

## Crash Recovery & Resilience Architecture

### 1. Deterministic Daemon Crash Recovery & Process Rediscovery
- **Startup Recovery Sweep**: On daemon restart (following abrupt termination `kill -9` or graceful restart), an immediate `/proc` recovery sweep runs *prior* to claiming the well-known D-Bus bus name `org.freedesktop.WatchAI`.
- **Deterministic Stable Surrogate IDs**: Discovered surviving processes are reconstructed using the deterministic identity formula:
  $$\text{SessionID} = \text{SHA256}(\text{PID} + \text{process\_start\_time} + \text{project\_path})[0..16]$$
- **Deterministic Process Sorting**: Discovered processes are sorted in canonical ascending order before registry insertion:
  1. `provider_id` ASC
  2. `project_path` ASC
  3. `process_id` ASC
- **Zero Phantom Sessions & Zero False Errors**: Sessions whose processes terminated while the daemon was offline are omitted from discovery; surviving processes start in `IDLE + DISCOVERY_REQUIRED` with zero false `ERROR` emissions.
- **Per-Process Fault Isolation**: Reading `/proc/[pid]` is isolated per process. Unreadable, locked, or transiently disappearing processes are skipped with debug logging without aborting sibling process scanning.

### 2. GNOME Shell Reconnection State Machine
The client extension implements an explicit 4-state connection state machine:
- `DISCONNECTED`: Initial state before first proxy acquisition attempt.
- `CONNECTING`: Acquiring D-Bus proxy and initiating synchronization handshake.
- `CONNECTED`: Active proxy, signal listeners attached, and live session cards rendered.
- `RECONNECTING`: Daemon absent; cached presentation mode active, retrying with backoff.

### 3. Asynchronous 5-Step Handshake with 5.0-Second Deadline
When the daemon appears on D-Bus (`NameOwnerChanged`), the client initiates an atomic 5-step handshake:
1. Reacquire `Gio.DBusProxy`.
2. Query `GetAggregateState()` for authoritative top-bar state.
3. Query `GetSessions()` for authoritative active session records.
4. Attach D-Bus signal listeners (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`).
5. Mark connection online (`CONNECTED`), replace cached cards with fresh state, and resume live duration ticking.

- **5.0-Second Timeout**: Each handshake is bounded by a strict asynchronous **5.0-second client-side timeout** (`HANDSHAKE_TIMEOUT_MS = 5000`). If any step times out or fails, the handshake is aborted, the proxy reference is discarded, and a backoff retry is scheduled.
- **Generation-Based Invalidation**: Each handshake is stamped with a monotonic generation token (`_handshakeGeneration`). Any timeout, disconnect, or extension `disable()` increments the generation, instantly invalidating in-flight proxy callbacks and guaranteeing that stale responses can never mark the client online or mutate UI state.

### 4. Jittered Exponential Backoff & Crash Loop Defense
To prevent D-Bus message storms and CPU thrashing during daemon crash loops:
- **Base Interval**: 1.0s initial delay.
- **Multiplier**: 2.0x per consecutive failure.
- **Hard Ceiling**: 30.0s clamp enforced *after* jitter calculation:
  $$\text{Delay} = \min\left(30000\text{ms}, \text{round}\left(\min(30000\text{ms}, 1000\text{ms} \times 2.0^n) \times [0.80, 1.20]\right)\right)$$
- **Randomized Jitter**: $\pm 20\%$ offset applied to each interval (effective delay strictly bounded between 800ms and 30,000ms).
- **Success Reset**: A complete, successful handshake resets consecutive failures to 0.
- **Silent Recovery**: The client suppresses desktop notification alerts during crash loops and never executes external process managers.

## Multi-Provider Discovery & Capability Modeling (Phase 8)

### 1. Supported Built-In Providers
WatchAI registers three default provider adapters in `AdapterRegistry::default_registry()`:
1. **Claude Code (`claude-code`)**: Scans `/proc` for `claude` CLI processes.
2. **OpenAI Codex CLI (`codex-cli`)**: Scans `/proc` for `codex` / `codex-cli` binaries and runtime runners (`node .../codex`).
3. **OpenCode (`opencode`)**: Scans `/proc` for `opencode` binaries and runtime runners (`python .../opencode`).

### 2. Provider Observational Capabilities
Each adapter declares its capabilities via `ProviderCapabilities`:
- `telemetry_tier`: Set to `TelemetryTier::ProcessDiscoveryOnly` for Phase 8.
- `supports_tool_categories`: `false`.
- `supports_activity_events`: `false`.

### 3. Decoupled Prioritized Event Ingestion Channel
Adapters communicate with the daemon engine through a bounded `tokio::sync::mpsc::channel(256)`:
- **Critical Transitions (`StateTransition`, `SessionTerminated`)**: Use `.send().await` under cooperative cancellation, ensuring guaranteed delivery without silent drops.
- **Telemetry Heartbeats (`Heartbeat`)**: Use non-blocking `try_send()`. Coalesced or dropped when channel capacity exceeds 80% utilization to guarantee buffer headroom for critical state transitions.
- **Event Ordering**: State transitions validate timestamp monotonicity against `session.state_entered_at` and increment the daemon's internal `sequence_number`.

## systemd --user Service & Desktop Lifecycle Integration (Phase 9)

### 1. User-Space Service Architecture (`Type=notify` + `BusName`)
WatchAI runs as an unprivileged systemd user service (`systemd/watchai.service`), bound to the graphical desktop session:
- **Unit Configuration**: `PartOf=graphical-session.target`, `After=graphical-session.target`. User D-Bus socket activation is ordered implicitly via `BusName=org.freedesktop.WatchAI`, avoiding distribution-specific service dependencies (such as `dbus.service` vs. `dbus-broker.service`).
- **D-Bus & Readiness Dual Barrier**: Uses `Type=notify` paired with `BusName=org.freedesktop.WatchAI`. In systemd, this configuration requires **both** `sd_notify("READY=1")` to be received and the D-Bus bus name to be acquired before systemd declares the service active.
- **Auto-Restart**: `Restart=on-failure` with `RestartSec=2s` under `Slice=session.slice`.
- **Least-Privilege Isolation**: Pure user-space execution with zero root or `sudo` requirements.

### 2. Deterministic Startup Readiness & Status Notification
- **Readiness Notification**: Communicates with systemd via `NOTIFY_SOCKET` using `sd_notify("READY=1")`, `STATUS=...`, and `STOPPING=1`. Note: WatchAI implements lifecycle notification; periodic watchdog heartbeats (`WATCHDOG=1`) are not configured in Phase 9.
- **Strict Startup Ordering**: `READY=1` is sent **only after** all startup phases complete: adapter environment checks, initial `/proc` recovery sweeps, D-Bus service publication, and background task loops are fully operational.
- **Graceful Fallback**: When launched outside systemd (manual execution, unit tests, containerized CI), the notifier detects the absence of `NOTIFY_SOCKET` and cleanly operates as a no-op without errors or panics.

### 3. Graceful Signal Handling & Bounded D-Bus Name Release
- **Signals Handled**: Traps `SIGTERM` and `SIGINT` (Ctrl+C) asynchronously.
- **D-Bus Disconnect Notice**: On shutdown initiation, the daemon calls `connection.release_name(BUS_NAME)`. This causes the D-Bus broker to emit `NameOwnerChanged(BUS_NAME, old_owner, "")`. Connected clients (like the GNOME Shell extension) immediately transition to `OFFLINE`.
- **Bounded Worker Drain**: Background liveness and event ingestion tasks receive a cooperative shutdown broadcast, draining any remaining critical events with an explicit 5.0-second timeout guarantee before bus name release.
- **Systemd Termination Notification**: Emits `sd_notify("STOPPING=1")` before exiting cleanly.
- **Re-entrant Signal Protection**: Repeated signals during drain are logged without aborting cleanup.

### 4. Line-Buffered Structured Logging & Zero-Leakage Privacy Redaction
- **Structured Tracing**: Built on `tracing` with configurable filter levels (`RUST_LOG`).
- **Line-Buffered Privacy Redaction**: Log output passes through a line-buffered `RedactingWriter` that buffers partial chunks until complete newline-delimited lines are formed, preventing sensitive tokens from crossing chunk boundaries unredacted. The scrubber automatically redacts:
  - API keys (`sk-ant-...`, `sk-...`).
  - Bearer tokens (`Bearer <token>`).
  - Key-value secrets (`password=...`, `token=...`, `secret=...`, `api_key=...`).
  - Prompt payloads (`prompt=...`).
  - Code diffs and patch lines (`diff --git ...`).
- **Operational Clarity**: Preserves non-sensitive operational diagnostics (session counts, lifecycle states, process IDs, timing benchmarks, indented JSON/log formats).

## Configuration, Notifications & Accessibility Polish (Phase 10)

### 1. GSettings Configuration & Schema Architecture
WatchAI integrates with the GNOME desktop configuration system via GSettings (`extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml`):
- **Schema ID**: `org.gnome.shell.extensions.watchai` under path `/org/gnome/shell/extensions/watchai/`.
- **Preference Keys**:
  - `dwell-duration-seconds` (`type="u"`, `<range min="1" max="60"/>`, default `60`): Configured baseline completion dwell expectation.
  - `enable-desktop-notifications` (`type="b"`, default `true`): Master switch for desktop notifications.
  - `notify-on-waiting` (`type="b"`, default `true`): Toggles alerts when an agent enters `WAITING` requiring user input or approval.
  - `notify-on-error` (`type="b"`, default `true`): Toggles alerts when an agent enters `ERROR` or crashes.
  - `indicator-icon-style` (`type="s"`, `<choices><choice value="symbolic"/><choice value="colored"/></choices>`, default `'symbolic'`): Visual presentation style of the indicator icon.
- **Informational Dwell Boundary**: The backend daemon (`watchai-core`) remains the sole authority for lifecycle state transitions and aggregate completion dwell (`COMPLETION_DWELL_SECONDS = 60`). The extension respects daemon-emitted signals directly and does NOT synthesize client-side dwell delays, preventing UI-daemon state divergence.
- **SettingsManager & Headless Fallback**: `SettingsManager` in `extension/settings.js` wraps GNOME 45+ ESM `this.getSettings()`, tracks all `changed::` signal IDs for leak-free disconnection in `disable()`, and provides an in-memory `FallbackSettings` adapter for headless testing when `gschemas.compiled` is unavailable. Invalid icon style strings deterministically fall back to `'symbolic'`.

### 2. Edge-Triggered Desktop Notifications & 5.0-Second Cooldown
The desktop notification engine (`NotificationManager` in `extension/notifications.js`) is designed to eliminate alert spam while ensuring critical blocking states are surfaced promptly:
- **Edge-Triggered State Gating**: Notifications are dispatched strictly upon an actual state transition entering `WAITING` or `ERROR` ($S_{t-1} \neq S_t$). Transitions into non-alert states (`WORKING`, `SUCCESS`, `STARTING`, `CANCELLED`, `UNKNOWN`, `IDLE`) never notify.
- **Identical-State Suppression**: Repeated metadata updates or heartbeats keeping an agent in `WAITING` or `ERROR` are suppressed.
- **5.0-Second Per-Session Cooldown**: If an alert transition occurs within $< 5.0\text{s}$ of the previous notification for that session, the notification is suppressed immediately.
- **Immediate Suppression / No-Queue Policy**: Suppressed notifications are discarded and are NOT queued or replayed when the cooldown expires, preventing confusing delayed alerts.
- **Per-Session Isolation**: Cooldown timestamps are tracked independently per `sessionId`. Alert events in Session A never throttle or delay notifications for Session B.
- **Session Cleanup**: When a session is removed (`onSessionRemoved`), all tracking state in `NotificationManager` is purged immediately to prevent memory leaks.
- **Zero-Leakage Privacy & Sanitization**: Notifications strictly display generic static templates (`"Agent is waiting for user input or approval."` / `"Agent encountered an error or crashed."`) with sanitized project workspace names (`sanitizeProjectName()`). Prompts, tool arguments, diffs, code snippets, credentials, and full filesystem paths are strictly excluded.

### 3. Comprehensive AT-SPI Screen Reader Accessibility
The extension provides full, standardized AT-SPI accessibility across all UI elements:
- **AT-SPI Roles**: Top-bar indicator button exposes `Atk.Role.TOGGLE_BUTTON` / `PUSH_BUTTON`; popover menu container exposes `Atk.Role.MENU`; session cards expose `Atk.Role.PANEL`.
- **Accessible Names across All 8 States**: Top-bar indicator exposes unambiguous descriptive accessible names for `IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, and `UNKNOWN`.
- **Dynamic Multi-Session Counter**: When `activeCount > 1`, the indicator's accessible name dynamically appends `" (${activeCount} active sessions)"`.
- **Offline Announcement**: When disconnected from the daemon, the indicator announces `"WatchAI daemon offline"`.
- **Accessible Help Description**: Indicator button exposes accessible description `"Click to open agent session popover menu"`.
- **Session Card Structured Descriptions**: Each session card exposes an accessible name formatted as `"${providerDisplayName}, ${sanitizedProjectName}, state ${currentState}, duration ${formattedDuration}"` and an accessible description detailing process ID and active tool category when present.
- **Badge Exclusion**: The top-bar session count badge is excluded from the AT-SPI tree when active session count is $\le 1$ to prevent redundant screen reader speech output.
