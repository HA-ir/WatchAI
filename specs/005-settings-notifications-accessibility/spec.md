# Feature Specification: Phase 10 — Configuration, Notifications & Accessibility Polish

**Feature Directory**: `specs/005-settings-notifications-accessibility`

**Created**: 2026-10-06

**Status**: Clarified (Ready for Planning)

**Input**: User description: "Phase 10 — Configuration, Notifications & Accessibility Polish"

---

## 1. Overview & Context

WatchAI has established a multi-provider daemon architecture, deterministic multi-session aggregation, process liveness monitoring, crash recovery, and desktop systemd user service integration (Phases 1–9).

Phase 10 completes the user-facing GNOME desktop integration by introducing:
1. **User Configuration via GSettings**: Desktop preferences for icon presentation style, notification behavior, and baseline dwell duration.
2. **Actionable, Spam-Resistant Desktop Notifications**: Native desktop alerts when an agent transitions into `WAITING` (requiring user input/approval) or `ERROR` (task failure or crash), protected by per-session cooldowns, an explicit no-queue suppression policy, and strict zero-leakage privacy.
3. **Comprehensive AT-SPI Accessibility (a11y)**: Standardized screen-reader accessible names, descriptions, and roles across the top-bar indicator, popover menu, and session cards for all 8 lifecycle states.

---

## 2. User Scenarios & Testing *(mandatory)*

### User Story 1 - Desktop User Preferences via GSettings (Priority: P1)

As a developer monitoring AI coding agents across multiple workspaces, I want to configure the visual style of the top-bar indicator and customize notification behavior through standard GNOME settings so that WatchAI matches my desktop aesthetics and notification preferences.

**Why this priority**: GSettings provides the foundational configuration storage for both notifications and visual styling. Without settings storage, notification toggles and icon preferences cannot persist or be altered by users.

**Independent Test**: Can be tested independently by changing keys via `gsettings set org.gnome.shell.extensions.watchai <key> <value>` and asserting that the GNOME extension dynamically updates its visual style and internal configuration without requiring a shell restart or daemon reconnect.

**Acceptance Scenarios**:
1. **Given** the extension is active with `indicator-icon-style` set to `'symbolic'`, **When** the user changes the setting to `'colored'`, **Then** the top-bar indicator icon immediately applies full-color CSS accents corresponding to the current aggregate state without restarting the extension.
2. **Given** `indicator-icon-style` is read by the extension, **When** an invalid or unrecognized string value is encountered (e.g. `'neon'`), **Then** the extension deterministically falls back to `'symbolic'` presentation.
3. **Given** `dwell-duration-seconds` is set to `10`, **When** the user modifies the setting to `25`, **Then** the extension reads and stores the updated preference value without attempting to synthesize client-side state divergence against daemon D-Bus signals.
4. **Given** the user sets `dwell-duration-seconds` to an invalid value (e.g. `0` or `75`), **Then** the GSettings schema boundary validation rejects the write, preserving the value within the valid range `[1, 60]`.
5. **Given** the extension is launched in a development environment or headless test suite where `gschemas.compiled` is not present, **When** `this.getSettings()` fails to locate the compiled schema, **Then** the extension catches the error, logs a non-fatal warning, and falls back to safe in-memory defaults without crashing GNOME Shell or failing tests.
6. **Given** the user disables the extension in GNOME Extensions Manager, **When** `disable()` executes, **Then** all GSettings change listeners are disconnected and no orphaned signals remain in memory.

---

### User Story 2 - Actionable, Spam-Resistant Desktop Notifications (Priority: P2)

As a developer working in a different application (browser, IDE, or terminal) while an agent runs in the background, I want to receive a native desktop notification when an agent needs my input (`WAITING`) or encounters a critical error (`ERROR`) so that I can unblock the agent promptly without constantly staring at the top bar.

**Why this priority**: Developers frequently multitask while AI coding agents execute long-running builds, tests, or code modifications. Proactive notification upon blocking states (`WAITING`) or fatal crashes (`ERROR`) prevents idle time while respecting attention boundaries through strict rate-limiting.

**Independent Test**: Can be tested independently by publishing state transitions into `WAITING` and `ERROR` for mock sessions and asserting that notifications are dispatched with provider name and project folder, while rapid state oscillations within 5 seconds or repeated identical states are immediately suppressed without queued replay.

**Acceptance Scenarios**:
1. **Given** `enable-desktop-notifications` and `notify-on-waiting` are enabled, **When** session $S$ transitions from `WORKING` to `WAITING` at $T=0$, **Then** a native desktop notification is displayed immediately and $S$'s cooldown timer is set to $T=0$.
2. **Given** session $S$ transitioned to `WAITING` at $T=0$, **When** session $S$ transitions to `WORKING` at $T=1$ and back to `WAITING` at $T=2$ (2 seconds later), **Then** the notification is suppressed immediately and is NOT queued for replay after the 5-second window expires.
3. **Given** session $S$ transitioned to `WAITING` at $T=0$, **When** session $S$ transitions to `WORKING` at $T=2$ and back to `WAITING` at $T=6$ (6 seconds later), **Then** a new desktop notification is dispatched normally and $S$'s cooldown timer is reset to $T=6$.
4. **Given** session $S$ transitioned to `ERROR` at $T=0$, **When** session $S$ transitions to `WORKING` and back to `ERROR` within 5 seconds, **Then** the second error notification is suppressed and NOT queued.
5. **Given** session $S$ transitioned to `ERROR` at $T=0$, **When** session $S$ transitions to `WORKING` and back to `ERROR` after 6 seconds, **Then** an error notification is dispatched normally.
6. **Given** session $S$ is in `WAITING`, **When** the daemon emits metadata updates or heartbeats keeping the session in `WAITING` (`WAITING` $\rightarrow$ `WAITING`), **Then** no additional desktop notification is emitted.
7. **Given** session $A$ receives a notification at $T=0$, **When** session $B$ transitions to `WAITING` at $T=1$, **Then** session $B$'s notification is dispatched normally because cooldown tracking is strictly independent per session.
8. **Given** session $S$ terminates and is removed via `onSessionRemoved(sessionId)`, **When** removal occurs, **Then** $S$'s cooldown tracking record is purged from memory.
9. **Given** `enable-desktop-notifications` is set to `false`, **When** an agent crashes into `ERROR`, **Then** zero desktop notifications are emitted.
10. **Given** any notification dispatched by WatchAI, **Then** the notification title and body contain strictly sanitized metadata (provider display name and sanitized project folder name) and NEVER contain prompt text, code diffs, command lines, or tokens.

---

### User Story 3 - Full Screen-Reader & AT-SPI Accessibility (Priority: P3)

As a visually impaired developer or keyboard-driven user utilizing assistive technologies (such as Orca screen reader), I want the top-bar indicator, popover menu, and individual session cards to expose standard AT-SPI roles, accessible names, and descriptions for all 8 states and active session counts so that I have equal situational awareness of my background AI agents.

**Why this priority**: Universal accessibility ensures WatchAI is accessible to all Linux desktop developers, fulfilling constitutional compliance (Principle V & VII) and GNOME Human Interface Guidelines (HIG).

**Independent Test**: Can be tested independently by inspecting the AT-SPI accessibility properties of the indicator button, popover menu, and session cards across all 8 states, asserting that accessible names, descriptions, and roles match the canonical specification and update dynamically when session counts change.

**Acceptance Scenarios**:
1. **Given** the system has zero active sessions (`IDLE`), **When** an assistive screen reader inspects the top-bar indicator, **Then** the accessible name reports `"WatchAI: No active coding agents"` with role `TOGGLE_BUTTON`.
2. **Given** three sessions are active with aggregate state `WORKING`, **When** the screen reader inspects the indicator, **Then** the accessible name reports `"WatchAI: Agent actively executing work (3 active sessions)"`.
3. **Given** the daemon crashes or is stopped, **When** the indicator transitions to offline mode, **Then** the accessible name reports `"WatchAI daemon offline"`.
4. **Given** the popover menu is opened, **When** navigated via screen reader, **Then** the popover container announces its accessible name `"WatchAI Agent Sessions"` with role `MENU`.
5. **Given** a session card is focused in the popover, **Then** the card announces an accessible name formatted as `"<Provider Display Name>, <Project Name>, state <State>, duration <Duration>"` with role `PANEL`.
6. **Given** a session card has an associated PID or active tool, **Then** the card exposes an accessible description formatted as `"Process ID <PID>, active tool <Tool>"`.
7. **Given** the active session count badge on the top bar, **When** active session count is 0 or 1, **Then** the badge is hidden from the assistive accessibility tree to prevent redundant announcements.

---

### Edge Cases

- **Rapid State Fluttering**: An agent rapidly oscillating between `WORKING` and `WAITING` (e.g. multi-step tool approvals within 2 seconds).
  - *Resolution*: The 5.0-second per-session cooldown suppresses subsequent alerts. Suppressed notifications are discarded and NOT queued for delayed replay.
- **Multiple Concurrent Errors**: Multiple background agents encountering errors simultaneously across different workspaces.
  - *Resolution*: Cooldown tracking is per-session (`session_id`). An error in Project A does not suppress an urgent error notification for Project B.
- **Missing GSettings Schema at Runtime**: The extension is loaded before `glib-compile-schemas` has been executed (e.g. running unpacked from source or in headless test suites).
  - *Resolution*: Extension detects uncompiled schema gracefully via try/catch, falls back to built-in default values (`symbolic`, 10s dwell, true toggles), and logs a non-crashing debug warning.
- **Dwell Duration Range Extremes**: User configures `dwell-duration-seconds` to minimum (`1`) or maximum (`60`).
  - *Resolution*: GSettings schema enforces `<range min="1" max="60"/>`. Values outside this range are rejected by GSettings before reaching the application.
- **Sanitization of Malicious or Pathological Project Folder Names**: A workspace path containing newlines, shell metacharacters, or HTML/Pango markup tags.
  - *Resolution*: Notification title and popover sanitization strips all newlines, carriage returns, tabs, and markup tags (`<`, `>`, `&`), truncates the name to a maximum of 32 characters, and falls back to `"workspace"` if empty.
- **GNOME Shell Screen Lock / Do Not Disturb**:
  - *Resolution*: Standard GNOME Shell notification queue handles Do Not Disturb; WatchAI sends notifications through standard `Main.notify()` or `Gio.Notification` with default priority.

---

## 3. Requirements *(mandatory)*

### Functional Requirements

#### GSettings Schema & Configuration (T062 & T063)
- **FR-001**: The system MUST define an XML GSettings schema at `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml` under schema ID `org.gnome.shell.extensions.watchai`.
- **FR-002**: The schema MUST define the key `dwell-duration-seconds` as an unsigned integer (`type="u"`) with default value `10` and valid range `1` to `60` enforced via `<range min="1" max="60"/>`.
  - *Architectural Scope Boundary*: The backend daemon owns aggregate completion dwell semantics (`COMPLETION_DWELL_SECONDS = 10` in `crates/watchai-core/src/aggregate.rs`). The extension reads `dwell-duration-seconds` as an informational preference representing user expectations, but does NOT synthesize client-side dwell delays or delay daemon signal propagation, maintaining zero state divergence with daemon D-Bus signals.
- **FR-003**: The schema MUST define the master notification toggle `enable-desktop-notifications` as a boolean (`type="b"`) with default value `true`.
- **FR-004**: The schema MUST define `notify-on-waiting` as a boolean (`type="b"`) with default value `true`.
- **FR-005**: The schema MUST define `notify-on-error` as a boolean (`type="b"`) with default value `true`.
- **FR-006**: The schema MUST define `indicator-icon-style` as a string (`type="s"`) with `<choices>` containing `<choice value="symbolic"/>` and `<choice value="colored"/>`, defaulting to `'symbolic'`.
  - *Fallback Rule*: If an unexpected or unvalidated string value is read at runtime, the extension MUST deterministically fall back to `'symbolic'`.
- **FR-007**: The GNOME Shell extension MUST load GSettings using standard GNOME 45+ ESM APIs (`this.getSettings()`).
  - *Schema Availability Rule*: If the compiled schema (`gschemas.compiled`) is unavailable (e.g. running from uncompiled source or headless test suites), the extension MUST catch the error, log a non-fatal warning, and fall back to in-memory default settings without throwing or crashing.
- **FR-008**: The extension MUST react dynamically to GSettings `changed::` signal events, updating top-bar icon classes and notification filters immediately without requiring an extension or shell reload.
- **FR-009**: When the extension is disabled (`disable()`), all GSettings signal connections MUST be explicitly disconnected.

#### Desktop Notifications (T064)
- **FR-010**: The system MUST dispatch a native desktop notification when an observed session transitions into `WAITING`, provided `enable-desktop-notifications` and `notify-on-waiting` are `true` and the session's cooldown timer has expired.
- **FR-011**: The system MUST dispatch a native desktop notification when an observed session transitions into `ERROR`, provided `enable-desktop-notifications` and `notify-on-error` are `true` and the session's cooldown timer has expired.
- **FR-012**: Notifications MUST be gated strictly on an actual state transition into the target state ($S_{t-1} \neq S_t$). Repeated events maintaining the same state (`WAITING` $\rightarrow$ `WAITING` or `ERROR` $\rightarrow$ `ERROR`) MUST NOT emit notifications.
- **FR-013**: The notification engine MUST enforce a minimum 5.0-second cooldown per session:
  - If a transition into `WAITING` or `ERROR` occurs within $< 5.0\text{s}$ of the previous notification for that session, the notification MUST be immediately suppressed.
  - Suppressed notifications MUST NOT be queued, delayed, or replayed when the cooldown timer expires.
  - The cooldown affects ONLY notification emission; internal session state tracking and UI rendering MUST evaluate and display immediately.
- **FR-014**: Cooldown state MUST be tracked independently per session (`sessionId`). Notification emission in session $A$ MUST NOT suppress notifications for session $B$.
- **FR-015**: When a session is removed (`onSessionRemoved`), its cooldown tracking state MUST be purged immediately from memory.
- **FR-016**: Notification titles MUST follow the standardized format: `WatchAI: <Provider Display Name> (<Sanitized Project Name>)`.
- **FR-017**: Project names displayed in notifications and accessibility labels MUST be sanitized:
  - All newlines (`\n`), carriage returns (`\r`), tabs (`\t`), and control characters MUST be stripped.
  - Pango/HTML markup characters (`<`, `>`, `&`) MUST be stripped or escaped.
  - The project name MUST be truncated to at most 32 characters.
  - If the sanitized name is empty, it MUST fall back to `"workspace"`.
- **FR-018**: Notification bodies MUST strictly use sanitized generic descriptions:
  - For `WAITING`: `"Agent is waiting for user input or approval."`
  - For `ERROR`: `"Agent encountered an error or crashed."`
- **FR-019**: Dispatched notifications, titles, and accessibility labels MUST NEVER contain prompt text, tool arguments, full filesystem paths, code diffs, or credentials (Zero-Leakage Privacy Invariant).

#### Accessibility & Screen Reader Polish (T065)
- **FR-020**: The top-bar indicator button MUST expose AT-SPI role `TOGGLE_BUTTON` (or `PUSH_BUTTON`) and descriptive accessible names for all 8 lifecycle states:
  - `IDLE`: `"WatchAI: No active coding agents"`
  - `STARTING`: `"WatchAI: Agent session initializing"`
  - `WORKING`: `"WatchAI: Agent actively executing work"`
  - `WAITING`: `"WatchAI: Agent blocked waiting for user approval"`
  - `SUCCESS`: `"WatchAI: Agent task completed successfully"`
  - `ERROR`: `"WatchAI: Agent encountered an error"`
  - `CANCELLED`: `"WatchAI: Agent session cancelled"`
  - `UNKNOWN`: `"WatchAI: Agent state unverified"`
- **FR-021**: When more than one session is active (`activeCount > 1`), the indicator accessible name MUST dynamically append `" (${activeCount} active sessions)"`.
- **FR-022**: When the daemon is offline, the indicator accessible name MUST report `"WatchAI daemon offline"`.
- **FR-023**: The top-bar indicator button MUST set its AT-SPI accessible description to `"Click to open agent session popover menu"`.
- **FR-024**: The session popover menu container MUST expose AT-SPI role `MENU` and accessible name `"WatchAI Agent Sessions"`.
- **FR-025**: Each session card within the popover menu MUST expose AT-SPI role `PANEL` and an accessible name structured as:
  `"${providerDisplayName}, ${sanitizedProjectName}, state ${currentState}, duration ${formattedDuration}"`
- **FR-026**: If a session card contains an associated process ID or active tool, it MUST expose an accessible description structured as:
  `"Process ID ${processId}, active tool ${activeToolCategory}"`
- **FR-027**: When the active session count is 0 or 1, the top-bar count badge MUST be excluded from the assistive accessibility tree to prevent redundant speech output.

---

## 4. Notification Transition State Matrix & Cooldown Policy

The notification engine operates as an immediate edge-triggered evaluator with per-session cooldown gating. Suppressed alerts are discarded immediately (no queue / no replay).

| Trigger Event | Prior State ($S_{t-1}$) | New State ($S_t$) | Elapsed Since Last Notification | Action | Resulting Cooldown State |
| :--- | :--- | :--- | :---: | :---: | :---: |
| First alert transition | `WORKING` | `WAITING` | N/A (None) | **DISPATCH NOTIFICATION** | `last_notified = now` |
| First alert transition | `STARTING` | `ERROR` | N/A (None) | **DISPATCH NOTIFICATION** | `last_notified = now` |
| Identical state heartbeat | `WAITING` | `WAITING` | Any | **SUPPRESS** (No edge) | Unchanged |
| Identical state error | `ERROR` | `ERROR` | Any | **SUPPRESS** (No edge) | Unchanged |
| Rapid oscillation | `WAITING` $\rightarrow$ `WORKING` $\rightarrow$ | `WAITING` | $< 5.0\text{ seconds}$ | **SUPPRESS** (Cooldown active; **DO NOT QUEUE**) | Unchanged |
| Eligible oscillation | `WAITING` $\rightarrow$ `WORKING` $\rightarrow$ | `WAITING` | $\ge 5.0\text{ seconds}$ | **DISPATCH NOTIFICATION** | `last_notified = now` |
| Rapid cross-alert | `WAITING` $\rightarrow$ `WORKING` $\rightarrow$ | `ERROR` | $< 5.0\text{ seconds}$ | **SUPPRESS** (Per-session cooldown; **DO NOT QUEUE**) | Unchanged |
| Eligible cross-alert | `WAITING` $\rightarrow$ `WORKING` $\rightarrow$ | `ERROR` | $\ge 5.0\text{ seconds}$ | **DISPATCH NOTIFICATION** | `last_notified = now` |
| Normal execution | Any | `WORKING` | Any | **NO NOTIFICATION** (Non-alert state) | Unchanged |
| Clean completion | Any | `SUCCESS` | Any | **NO NOTIFICATION** (Non-alert state) | Unchanged |
| Session removed | Any | N/A | Any | **NO NOTIFICATION** | Cooldown record purged |

---

## 5. Key Entities

### GSettings Configuration Schema (`org.gnome.shell.extensions.watchai`)
```xml
<?xml version="1.0" encoding="UTF-8"?>
<schemalist gettext-domain="watchai">
  <schema id="org.gnome.shell.extensions.watchai" path="/org/gnome/shell/extensions/watchai/">
    <key name="dwell-duration-seconds" type="u">
      <range min="1" max="60"/>
      <default>10</default>
      <summary>Completion dwell duration</summary>
      <description>Configured baseline completion dwell duration in seconds (1-60).</description>
    </key>
    <key name="enable-desktop-notifications" type="b">
      <default>true</default>
      <summary>Enable desktop notifications</summary>
      <description>Master switch to enable or disable desktop notifications for agent lifecycle transitions.</description>
    </key>
    <key name="notify-on-waiting" type="b">
      <default>true</default>
      <summary>Notify on waiting for approval</summary>
      <description>Dispatch a notification when an agent enters WAITING requiring user interaction or approval.</description>
    </key>
    <key name="notify-on-error" type="b">
      <default>true</default>
      <summary>Notify on error or crash</summary>
      <description>Dispatch a notification when an agent enters ERROR or crashes.</description>
    </key>
    <key name="indicator-icon-style" type="s">
      <choices>
        <choice value="symbolic"/>
        <choice value="colored"/>
      </choices>
      <default>'symbolic'</default>
      <summary>Top-bar indicator icon style</summary>
      <description>Visual presentation style of the indicator icon: 'symbolic' (monochrome desktop theme) or 'colored' (state-colored accents).</description>
    </key>
  </schema>
</schemalist>
```

### Notification Cooldown Tracker (`NotificationManager`)
- **`cooldowns`**: In-memory `Map<string, number>` mapping `sessionId` $\rightarrow$ timestamp of last dispatched notification in milliseconds.
- **`lastStates`**: In-memory `Map<string, string>` mapping `sessionId` $\rightarrow$ previous state string ($S_{t-1}$) to enforce transition edge detection.

---

## 6. Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001**: 100% of setting changes in GSettings are reflected in the running extension within 100ms without requiring an extension or shell reload.
- **SC-002**: Zero notification duplicate alerts are emitted when an agent remains in `WAITING` or `ERROR` across multiple telemetry updates.
- **SC-003**: 100% of notification events for the same session occurring within 5.0 seconds of an earlier notification are suppressed immediately and zero suppressed notifications are replayed upon timer expiry.
- **SC-004**: 100% of dispatched desktop notifications contain zero prompt text, tool arguments, credentials, or file paths (Zero-Leakage Privacy verified).
- **SC-005**: All 8 lifecycle states produce valid, unique AT-SPI accessible names and descriptions verified via automated GJS accessibility test suites.
- **SC-006**: Missing or uncompiled GSettings schemas during development or testing fall back gracefully to default values with zero unhandled exceptions or shell crashes.
- **SC-007**: Disabling the extension cleanly disconnects 100% of GSettings signal connections with zero memory leaks.

---

## 7. Assumptions & Technical Constraints

- **Architectural Separation**:
  - The daemon binary (`watchai-daemon`) and core domain (`watchai-core`) own all state transitions, process liveness, and completion dwell lifecycles.
  - The GNOME Shell extension tier (`extension/`) acts strictly as a presentation and desktop integration layer.
  - The D-Bus wire protocol `(sssssssus)` and interface definition remain 100% immutable in Phase 10.
- **Dwell Scope Boundary**:
  - `dwell-duration-seconds` in GSettings represents the user's preferred completion dwell window, aligning with the daemon's 10-second default completion dwell (`COMPLETION_DWELL_SECONDS`).
  - Because no bidirectional D-Bus configuration method exists, the extension does NOT synthesize client-side dwell delays. The daemon's emitted signals remain authoritative.
- **GNOME Shell 45+ Compatibility**:
  - Uses standard ESM `import` statements and modern `Extension` subclass lifecycle methods (`this.getSettings()`).
  - Schema files reside at `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml`.

---

## 8. Explicit Out-of-Scope Items

- **Task T022**: Claude Code opt-in hook event telemetry receiver remains strictly discovery-gated and unchecked `[ ]`.
- Modifying the core 8-state FSM, state transition rules, or dwell constants in `watchai-core`.
- Altering the mathematical priority aggregation hierarchy.
- Adding custom sound effects or audio playback to notifications.
- Interactive notification action buttons that execute shell commands or kill processes.
- Bidirectional D-Bus RPCs for daemon reconfiguration from GSettings.
- Meson, RPM, DEB, or Flatpak packaging (reserved for Phase 11).
