# Feature Specification: Baseline Agent Monitoring & GNOME Shell Indicator

**Feature Branch**: `001-agent-status-indicator`

**Created**: 2026-10-03

**Status**: Draft

**Input**: User description: "Create the baseline product specification for WatchAI. WatchAI is a Linux desktop application focused on monitoring AI coding agents running locally on the user's machine..."

## Clarifications

### Session 2026-10-03
- Q: How should WatchAI distinguish an agent blocked mid-task awaiting user permission from an agent that has completed its turn and is sitting at an interactive prompt? → A: Strict Split (Option A): `WAITING` strictly indicates mid-task blocked execution (tool permissions, confirmation prompts). Finishing a task enters `SUCCESS` (dwells for 10s) and transitions to `IDLE` even if the process remains open at its prompt.
- Q: What session data, if any, should be persisted across daemon restarts or system reboots? → A: Purely Ephemeral (Option A): All session state exists in memory only. On daemon restart, active sessions are rediscovered via adapter probes; historical/terminated sessions are cleared. Only user preferences are persisted to disk.
- Q: How should WatchAI verify that an agent session is still alive when no telemetry events have been received for an extended period? → A: Process Liveness + Adaptive Timeout (Option A): If a PID is known, verify OS process existence in `/proc` to keep state as `WORKING` regardless of telemetry silence. If no PID is known, fallback to an adaptive silence timeout (5 min for `WORKING`, 1 min for `STARTING`).
- Q: Which local IPC transport mechanism should be used between the background daemon and the GNOME Shell extension? → A: D-Bus Session Bus (Option A): Standard session bus service exporting properties and signals (`org.freedesktop.WatchAI`). Uses native GNOME `Gio.DBusProxy` asynchronously with user-session isolation.
- Q: How should WatchAI handle initial integration with providers (Claude Code, OpenAI Codex CLI, OpenCode) where official daemon event hooks are unverified or evolving? → A: Tiered Discovery Model (Option A): Adapters use standard process/PID inspection for baseline presence, coupled with documented opt-in hook scripts where available (e.g. Claude Code hooks). Unverified providers expose a `DISCOVERY_REQUIRED` badge in the UI and emit `UNKNOWN` if telemetry cannot be proven.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Real-Time Top-Bar Agent Status Monitoring (Priority: P1)

As a developer running local AI coding agents, I want an unobtrusive top-bar indicator in GNOME Shell that immediately communicates whether any agent is working, waiting for my input, succeeded, or encountered an error, so that I can stay aware of agent progress without keeping terminal windows visible or constantly alt-tabbing.

**Why this priority**: This is the core value proposition of WatchAI. Without immediate visual and accessible awareness of agent lifecycle states in the GNOME panel, the application provides no utility.

**Independent Test**: Can be tested by starting a simulated or real agent session, transitioning it through lifecycle states (`STARTING` → `WORKING` → `WAITING` → `SUCCESS` / `ERROR`), and verifying that the GNOME top-bar icon and accessible label update in real-time without latency or display flicker.

**Acceptance Scenarios**:

1. **Given** no active coding agent sessions, **When** the desktop session is running, **Then** the top-bar indicator displays the `IDLE` state (dim/neutral icon with text indicating no active agents).
2. **Given** an idle state, **When** an agent session starts executing a task, **Then** the top-bar indicator transitions immediately to `WORKING` with an active visual indicator and accessible description.
3. **Given** an agent is executing a task, **When** the agent pauses to request user approval or permission, **Then** the top-bar indicator transitions immediately to `WAITING` with prominent visual and textual cues directing user attention.
4. **Given** an agent finishes its task, **When** execution succeeds without errors, **Then** the indicator transitions to `SUCCESS` for a configured dwell time before settling back to `IDLE` if no other sessions are active.
5. **Given** an agent encounters an unrecoverable failure, **When** execution terminates abnormally, **Then** the indicator transitions to `ERROR` with a distinct failure presentation.

---

### User Story 2 - Detailed Session Inspection via Indicator Popover (Priority: P2)

As a developer running one or more AI coding agents, I want to click the top-bar indicator to open a structured session menu listing all observed sessions, their respective providers, working directories, active states, durations, and terminal/process identifiers, so that I can manage concurrent work across multiple repositories and locate the exact terminal requiring attention.

**Why this priority**: When multiple agents run concurrently or an agent requires intervention, the aggregate top-bar icon alone cannot disambiguate which session or project needs the user. The popover provides actionable context.

**Independent Test**: Can be tested by running two concurrent agent sessions (e.g., across different projects or providers), opening the popover menu, and verifying that each session is displayed with accurate metadata, state badges, and elapsed duration counters.

**Acceptance Scenarios**:

1. **Given** one or more active agent sessions, **When** the user clicks the top-bar indicator, **Then** a popover menu opens showing each session's provider name, project folder, current lifecycle state, and elapsed duration.
2. **Given** multiple sessions with different states (e.g., one `WORKING` and one `WAITING`), **When** the popover is opened, **Then** sessions needing user attention (`WAITING`, `ERROR`) are highlighted or sorted to the top.
3. **Given** an active session with an associated terminal process, **When** viewing the session entry in the popover, **Then** the session displays safely available process information (such as PID or terminal title) to help the user locate the window.
4. **Given** zero active sessions, **When** the user clicks the top-bar indicator, **Then** the popover displays an informative empty state explaining that the monitor is active and waiting for agent sessions.

---

### User Story 3 - Multi-Session State Aggregation & Conflict Resolution (Priority: P3)

As a developer running multiple coding agents simultaneously across different projects, I want the single top-bar indicator to reflect the most urgent state among all active sessions based on a deterministic policy, so that critical prompts and errors are never masked by background background work.

**Why this priority**: Multi-agent multitasking is a core workflow requirement. If an aggregate indicator arbitrarily picks the newest session or an alphabetical one, a blocked session waiting for user input could be hidden behind another session that is actively working.

**Independent Test**: Can be tested by creating sessions with conflicting states (e.g., Session A `WORKING`, Session B `WAITING`, Session C `SUCCESS`) and confirming that the aggregate indicator deterministically displays `WAITING`.

**Acceptance Scenarios**:

1. **Given** Session A is `WORKING` and Session B transitions to `WAITING`, **When** the aggregate indicator updates, **Then** it reflects `WAITING` because user attention takes precedence over background work.
2. **Given** Session A is `WAITING` and Session B encounters an `ERROR`, **When** the aggregate indicator updates, **Then** it displays `ERROR` (or dual urgent indication) based on the deterministic priority hierarchy.
3. **Given** all sessions terminate or disconnect, **When** their retention period expires, **Then** the indicator smoothly returns to `IDLE`.

---

### User Story 4 - Resilient Lifecycle Tracking & Crash Recovery (Priority: P4)

As a Linux desktop user, I want WatchAI to handle daemon restarts, GNOME Shell restarts, agent process crashes, and ungraceful disconnections gracefully without freezing the desktop, losing session continuity, or showing phantom "stale" sessions indefinitely.

**Why this priority**: Linux desktop components and terminal processes crash or restart frequently (e.g., compositor reloads, terminal tab closures). Robustness prevents the indicator from becoming an untrusted distraction.

**Independent Test**: Can be tested by killing a running agent process (`kill -9`), restarting GNOME Shell, or stopping the backend service, verifying that sessions transition to appropriate recovery states (`UNKNOWN` / `ERROR` / disconnected) and reconnect automatically upon service resumption.

**Acceptance Scenarios**:

1. **Given** an observed agent process suddenly dies without sending a clean termination signal, **When** the daemon detects missing heartbeats or process exit, **Then** the session transitions to `ERROR` or is pruned after a deterministic grace period.
2. **Given** the GNOME Shell extension loses connection to the background daemon, **When** the daemon becomes unavailable, **Then** the extension displays a disconnected/neutral indicator without raising unhandled shell exceptions or freezing the desktop.
3. **Given** the background daemon restarts, **When** it becomes available again, **Then** the GNOME extension automatically reconnects with exponential backoff and refreshes current session states.

---

### Edge Cases

- **Agent session closes abruptly**: What happens when a user closes a terminal window or kills an agent with `kill -9` without a clean exit event?
  - *Behavior*: The monitoring service must detect liveness failure (via process monitoring, socket closure, or heartbeat timeout) within 5 seconds and transition the session to `ERROR` or `CANCELLED`, rather than leaving it in `WORKING`.
- **Rapid state flapping**: How does the system handle an agent rapidly cycling between states within milliseconds?
  - *Behavior*: The daemon must throttle or debounce visual state emissions (e.g., maximum 10 UI updates per second) to prevent GNOME Shell rendering stutter and visual jitter.
- **Out-of-order and duplicate events**: How are asynchronous IPC events ordered?
  - *Behavior*: Events must carry monotonic sequence numbers or millisecond timestamps; older events arriving after a newer state transition must be discarded.
- **GNOME Shell restart (Wayland / X11)**: What happens when the GNOME Shell process is reloaded or the user locks/unlocks the desktop?
  - *Behavior*: The extension's enable/disable lifecycle must safely clean up all signals, timers, and D-Bus proxies, re-establishing subscriptions without leaking resources.
- **Simultaneous sessions on the same project**: What if two instances of Claude Code or Codex CLI run in the exact same directory?
  - *Behavior*: The system must disambiguate sessions using a unique local session ID and process PID, displaying them as distinct cards in the session popover.
- **Desktop notifications during focus**: Should the user be spammed with notifications?
  - *Behavior*: Notifications must be optional, rate-limited, and sent only for critical transitions (`WAITING` for user input or `ERROR`), never for routine `WORKING` ticks.

---

## Requirements *(mandatory)*

### Functional Requirements

#### Core Domain & State Model
- **FR-001**: The system MUST implement an explicit, finite-state machine (FSM) for agent session lifecycles supporting exactly these conceptual states:
  - `IDLE`: No active task processing is occurring; includes sessions sitting at an interactive prompt ready for new user commands.
  - `STARTING`: An agent session has been initiated but has not yet begun active task processing.
  - `WORKING`: An agent is actively processing a task, generating code, or executing tools.
  - `WAITING`: An agent is blocked mid-task awaiting user input, permission confirmation, or external approval.
  - `SUCCESS`: An agent task or command execution has completed successfully; dwells for a configured duration before transitioning to `IDLE`.
  - `ERROR`: An agent session stopped or failed due to an unrecoverable error.
  - `CANCELLED`: An agent session or active task was explicitly interrupted or aborted by the user.
  - `UNKNOWN`: The system cannot reliably determine current agent state due to partial observability or communication loss.
- **FR-002**: State transitions MUST be validated against allowed transition edges; invalid state transitions MUST be rejected and logged as diagnostic warnings.
- **FR-003**: The core domain model MUST remain entirely provider-agnostic, defining generic session entities, states, and lifecycle events without vendor-specific terminology.

#### Aggregate Indicator Behavior
- **FR-004**: The system MUST compute a single deterministic aggregate state from all active sessions according to the following strict priority order:
  $$\text{ERROR} > \text{WAITING} > \text{WORKING} > \text{STARTING} > \text{CANCELLED} > \text{SUCCESS} > \text{UNKNOWN} > \text{IDLE}$$
- **FR-005**: If at least one active session is in `WAITING` and no session is in `ERROR`, the aggregate indicator MUST display `WAITING`.
- **FR-006**: When all sessions reach a terminal state (`SUCCESS`, `CANCELLED`, `ERROR`), the aggregate indicator MUST reflect that state for a configurable dwell duration (default: 10 seconds) before resetting to `IDLE` if no active sessions remain.

#### GNOME Shell Presentation & Accessibility
- **FR-007**: The GNOME Shell extension MUST provide a top-bar indicator displaying visual cues corresponding to the aggregate state:
  - `IDLE`: Dim / inactive neutral icon.
  - `WORKING`: Active accent indicator (e.g., pulsating/steady yellow/amber icon).
  - `WAITING`: Attention-demanding indicator (e.g., distinctive red/orange icon).
  - `SUCCESS`: Positive completion indicator (e.g., green check/icon).
  - `ERROR`: Distinguishable error indicator (e.g., distinct alert badge/icon).
  - `CANCELLED` / `UNKNOWN`: Muted neutral indicator.
- **FR-008**: Color MUST NOT be the only mechanism used to convey state. The top-bar indicator MUST support symbolic iconography, accessible labels (accessible name and description via AT-SPI), and optional text badges (e.g., count of active sessions).
- **FR-009**: The GNOME Shell extension MUST provide a popover menu accessible by clicking the top-bar indicator, presenting:
  - An aggregate status summary header with active session counts.
  - A scrollable list of individual session cards.
  - For each session card: provider name, project folder/name, current state badge, elapsed time/duration, and process ID/terminal info if safely available.
  - Visual distinction for sessions in `WAITING` or `ERROR`.
  - An empty state message when zero sessions are present.
- **FR-010**: All UI updates and IPC interactions within the GNOME Shell extension MUST be asynchronous and non-blocking; the extension MUST NOT execute synchronous I/O or heavy operations in the compositor thread.

#### Provider Adapters & Discovery
- **FR-011**: The system MUST support provider-specific adapters that translate provider events into standard WatchAI lifecycle events.
- **FR-012**: The initial architecture MUST support adapters for:
  - Claude Code
  - OpenAI Codex CLI
  - OpenCode
- **FR-013**: The system MUST implement a Tiered Discovery Model for provider integration:
  - Adapters MUST rely on non-invasive baseline process and workspace detection (e.g., verifying active agent binary execution and project directories via `/proc`) to establish initial presence.
  - Where supported by the provider, adapters MAY register documented opt-in event hooks (e.g., Claude Code hook scripts or notifications) to stream fine-grained lifecycle transitions.
  - Providers without verified or active event mechanisms MUST be marked with a `DISCOVERY_REQUIRED` badge in the UI, gracefully falling back to coarse `WORKING` / `IDLE` / `UNKNOWN` based solely on verified process execution rather than speculative log parsing.
- **FR-014**: The adapter architecture MUST allow adding new provider adapters without modifying the core state machine, IPC schemas, or GNOME UI.

#### Session Lifecycle & Resilient Tracking
- **FR-015**: Each observed session MUST be assigned a stable, unique identifier that persists for the entire duration of the session.
- **FR-016**: The system MUST distinguish multiple concurrent sessions from the same provider across different projects or within the same project.
- **FR-017**: The system MUST implement robust liveness tracking combining process verification and adaptive silence timeouts:
  - If a session process ID (PID) is known, the daemon MUST check OS process existence in `/proc` periodically; as long as the process is alive, the session MUST NOT be marked stale due to telemetry silence.
  - If no PID is available, the daemon MUST apply adaptive telemetry silence timeouts (default: 5 minutes for `WORKING`, 1 minute for `STARTING`) before transitioning the session to `UNKNOWN` or `ERROR`.
  - If a known PID terminates without sending a clean exit event, the session MUST transition to `ERROR` or `CANCELLED` within 5 seconds.
- **FR-018**: When an agent session terminates, its record MAY remain in the popover as a completed session for a configurable retention window (default: 60 seconds) before being automatically pruned.

#### Local Service & IPC Architecture
- **FR-019**: A background daemon service MUST manage all adapter discovery, session tracking, state aggregation, and IPC publishing.
- **FR-020**: The daemon MUST be deployable as a standard Linux systemd user service (`systemd --user`).
- **FR-021**: Communication between the daemon and GNOME Shell extension MUST use the standard D-Bus user session bus:
  - The daemon MUST register a well-known service name (`org.freedesktop.WatchAI` or `org.gnome.WatchAI`) on the session bus.
  - The daemon MUST expose standard properties and signals (such as `SessionAdded`, `SessionRemoved`, `SessionUpdated`, and `AggregateStateChanged`) for reactive, asynchronous client updates.
  - The GNOME Shell extension MUST connect via asynchronous `Gio.DBusProxy` calls and signal subscriptions, strictly avoiding any synchronous D-Bus calls in the compositor thread.
- **FR-022**: The system MUST operate entirely locally with zero reliance on cloud APIs, internet connectivity, or external servers.

#### Privacy & Security
- **FR-023**: The system MUST NOT collect, store, transmit, or expose agent prompts, model completions, source code snippets, patch diffs, file contents, environment credentials, API keys, or raw shell output.
- **FR-024**: Telemetry and IPC payloads MUST be strictly limited to operational metadata: session ID, provider ID, project name/path, state enum, timestamps, and high-level tool categories without sensitive arguments.
- **FR-025**: Local IPC endpoints MUST enforce user-only access permissions, rejecting connections from other OS users.
- **FR-028**: All session entities, timestamps, and state tracking MUST reside purely in volatile memory. The system MUST NOT persist session history, workspace paths, or agent execution records to non-volatile disk storage. Only user settings and preferences (e.g., via GSettings or configuration files in `$XDG_CONFIG_HOME`) MAY be persisted.

#### Notifications & User Preferences
- **FR-026**: The system MUST support optional desktop notifications when an agent transitions into `WAITING` (requiring user attention) or `ERROR`.
- **FR-027**: Users MUST be able to configure:
  - Notification toggles per event type (`WAITING`, `ERROR`, `SUCCESS`).
  - Stale session timeout thresholds.
  - Top-bar display options (icon only, icon + counter, or icon + text label).

---

### Key Entities *(include if feature involves data)*

- **AgentSession**:
  - `sessionId`: Unique string identifier (UUID or local cryptographic hash).
  - `providerId`: Identifier of the agent provider (e.g., `claude-code`, `codex-cli`, `opencode`).
  - `projectPath`: Absolute or user-relative path of the monitored workspace.
  - `projectName`: User-friendly display name of the workspace folder.
  - `currentState`: One of `IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`.
  - `startedAt`: Timestamp when the session was initiated.
  - `stateEnteredAt`: Timestamp when the current state was entered.
  - `lastSeenAt`: Timestamp of the most recent telemetry event or heartbeat.
  - `processId`: Optional process ID (PID) of the agent process if safely discoverable.
  - `activeToolCategory`: Optional high-level tool category (e.g., `FileRead`, `ShellExecution`, `ModelThinking`) strictly excluding arguments and content.

- **AggregateState**:
  - `state`: Computed top-level state based on priority rules (`ERROR` > `WAITING` > `WORKING` > ...).
  - `activeSessionCount`: Number of currently non-terminal sessions.
  - `waitingSessionCount`: Number of sessions currently in `WAITING`.
  - `errorSessionCount`: Number of sessions currently in `ERROR`.
  - `updatedAt`: Timestamp of the latest aggregate computation.

- **ProviderAdapterContract**:
  - `providerId`: Unique provider string.
  - `displayName`: Human-readable name (e.g., "Claude Code", "OpenAI Codex CLI").
  - `status`: One of `ACTIVE`, `DISCOVERY_REQUIRED`, `UNAVAILABLE`.
  - `eventStream`: Observable stream emitting normalized session lifecycle events.

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Top-bar indicator updates reflect agent state changes within **100 milliseconds** of event receipt by the local daemon.
- **SC-002**: Zero compositor frame drops or UI latency introduced into GNOME Shell; all IPC calls and UI rendering run asynchronously.
- **SC-003**: 100% of conflicting multi-session states resolve according to the deterministic priority rule (`ERROR` > `WAITING` > `WORKING` > `STARTING` > `CANCELLED` > `SUCCESS` > `UNKNOWN` > `IDLE`).
- **SC-004**: Stale or crashed agent processes are detected and visually updated within **5 seconds** of process disappearance or telemetry silence.
- **SC-005**: Zero leakage of sensitive user data: automated test verification confirms that prompts, source code, file contents, and API credentials never appear in IPC payloads, logs, or persistent stores.
- **SC-006**: The system recovers seamlessly from daemon or GNOME Shell restarts, restoring state within **2 seconds** of process reconnection without requiring user intervention.
- **SC-007**: 100% of user interface elements meet accessibility standards with non-color indicators and valid AT-SPI descriptions.

---

## Assumptions

- **Desktop Environment**: The primary target desktop environment is GNOME Shell running on modern Linux distributions (e.g., Ubuntu, Fedora, Debian) on either Wayland or X11.
- **User Permissions**: WatchAI runs entirely within the unprivileged user session (`systemd --user`) and does not require administrative or root privileges.
- **Provider Telemetry Verification**: Because the internal hooks and IPC mechanisms of Claude Code, OpenAI Codex CLI, and OpenCode may vary or evolve across versions, adapter implementations will treat specific telemetry capture mechanisms (e.g., hook scripts, directory logs, process monitoring) as discovery-backed modules subject to ongoing verification.
- **Local IPC Mechanism**: D-Bus user session bus (`org.freedesktop.WatchAI` or similar) or Unix domain sockets under `$XDG_RUNTIME_DIR` are standard and ubiquitously available in Linux desktop user sessions.
- **Storage & State Persistence**: WatchAI relies exclusively on volatile in-memory state tracking for active sessions; no session execution data or history is persisted to disk. Only user preferences and settings are stored in standard desktop configuration facilities (GSettings / `$XDG_CONFIG_HOME/watchai/`).
