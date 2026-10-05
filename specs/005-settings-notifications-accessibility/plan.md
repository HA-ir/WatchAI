# Implementation Plan: Phase 10 — Configuration, Notifications & Accessibility Polish

**Branch**: `010-settings-notifications-accessibility` | **Date**: 2026-10-06 | **Spec**: [specs/005-settings-notifications-accessibility/spec.md](spec.md)

---

## 1. Summary

Phase 10 completes the user-facing GNOME desktop integration for WatchAI by implementing user configuration via GSettings, actionable and spam-resistant desktop notifications, and comprehensive AT-SPI accessibility enhancements (Tasks `T062`–`T065` from `specs/001-agent-status-indicator/tasks.md`).

Key architectural deliverables:
1. **User Configuration via GSettings (`T062`, `T063`)**: Define XML schema `org.gnome.shell.extensions.watchai.gschema.xml` (`dwell-duration-seconds`, `enable-desktop-notifications`, `notify-on-waiting`, `notify-on-error`, `indicator-icon-style`). Implement a dedicated `SettingsManager` in `extension/settings.js` with dynamic `changed::` signal listeners and clean teardown in `disable()`. Support both compiled schema environments and headless testing via defensive in-memory fallback.
2. **Actionable Desktop Notifications (`T064`)**: Implement `NotificationManager` in `extension/notifications.js` with edge-triggered alert detection entering `WAITING` or `ERROR`, an invariant 5.0-second per-session cooldown with immediate suppression (no queueing / no delayed replay), per-session cooldown cleanup upon session removal, and zero-leakage privacy enforcement.
3. **AT-SPI Screen Reader Polish (`T065`)**: Standardize AT-SPI roles (`TOGGLE_BUTTON`, `MENU`, `PANEL`), accessible names, and descriptions across the indicator button, popover menu, and session cards, incorporating sanitized project names and dynamic session counter formatting.

---

## 2. Technical Context

- **Platform & Target**: Linux Desktop running GNOME Shell 45, 46, and 47 (GJS ESM, Mutter, systemd user session).
- **Primary Dependencies**:
  - GJS / GNOME Shell ESM: `gi://Gio`, `gi://GLib`, `gi://St`, `gi://Clutter`, `gi://GObject`, `gi://Atk`, `resource:///org/gnome/shell/ui/main.js`, `resource:///org/gnome/shell/extensions/extension.js`.
  - Rust Daemon & Core (Underlying IPC tier): `tokio`, `zbus 4.4`, `chrono`, `watchai-core`, `watchai-daemon`.
- **Architectural Separation**:
  - **Core/Daemon Tier (`crates/`)**: Sole authority for lifecycle state transitions, `/proc` liveness, and aggregate completion dwell (`COMPLETION_DWELL_SECONDS = 10`).
  - **IPC Tier (`crates/watchai-ipc`)**: Immutable D-Bus wire protocol `(sssssssus)` and interface `org.freedesktop.WatchAI`. Zero contract modifications in Phase 10.
  - **Extension Tier (`extension/`)**: Pure presentation, user settings, desktop notifications, and AT-SPI accessibility.
- **Constraints**:
  - Zero daemon source code modifications.
  - Zero D-Bus interface changes.
  - 100% local-first (zero external network egress).
  - Zero prompt, tool argument, diff, credential, or token leakage.
  - Fully testable in headless GJS environments (`gjs -m extension/tests/...`).

---

## 3. Constitution Check

| Principle | Status | Verification & Architectural Evidence |
| :--- | :---: | :--- |
| **I. Provider-Agnostic Architecture** | **PASS** | Notifications, GSettings preferences, and accessibility descriptions apply uniformly across all discovered providers (`claude-code`, `codex-cli`, `opencode`) using provider display names. |
| **II. Strict Layered Separation** | **PASS** | GSettings and notifications are strictly confined to `extension/`. The daemon remains 100% agnostic to desktop settings and notifications. Daemon owns lifecycle and aggregate dwell; extension does not synthesize client-side dwell. |
| **III. Explicit FSM Modeling** | **PASS** | Notifications evaluate strictly on actual state transition edges ($S_{t-1} \neq S_t$) entering `WAITING` or `ERROR`. Repeated states do not notify. Core FSM rules are unaltered. |
| **IV. Local-First & Zero-Leakage Privacy** | **PASS** | Notifications strictly display generic static templates with sanitized provider and project names. Prompt text, tool arguments, full paths, and secrets are prohibited. |
| **V. GNOME Extension Stability** | **PASS** | Modern GNOME 45+ ESM APIs used. All GSettings signal listeners explicitly disconnected in `disable()`. Schema loading includes defensive fallback for headless tests to prevent shell crashes. |
| **VI. Determinism & Quality Gates** | **PASS** | Exact 5.0-second per-session cooldown. Immediate suppression without queueing. Deterministic fallback to `'symbolic'` on invalid icon style. Comprehensive GJS automated test suites. |
| **VII. Contract Stability & Observability** | **PASS** | D-Bus tuple `(sssssssus)` and signals unchanged. Task T022 remains permanently discovery-gated and unchecked `[ ]`. |

---

## 4. Exact Files to Create / Modify

### Files to Create

1. **`extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml` (T062)**
   - Schema ID: `org.gnome.shell.extensions.watchai`
   - Defines keys:
     - `dwell-duration-seconds`: `type="u"`, default `10`, range `1`–`60`.
     - `enable-desktop-notifications`: `type="b"`, default `true`.
     - `notify-on-waiting`: `type="b"`, default `true`.
     - `notify-on-error`: `type="b"`, default `true`.
     - `indicator-icon-style`: `type="s"`, `<choices><choice value="symbolic"/><choice value="colored"/></choices>`, default `'symbolic'`.

2. **`extension/settings.js` (T063)**
   - `SettingsManager` class wrapping `extension.getSettings()` or `Gio.Settings`.
   - Implements safe fallback to in-memory defaults when `gschemas.compiled` is unavailable in development or test environments.
   - Provides typed getters (`getIconStyle()`, `getEnableNotifications()`, `getNotifyOnWaiting()`, `getNotifyOnError()`, `getDwellDurationSeconds()`).
   - Manages signal listener IDs and clean `disconnectAll()` in `destroy()`.

3. **`extension/notifications.js` (T064)**
   - `NotificationManager` class managing notification lifecycle and cooldowns.
   - State transition tracking: `lastStates` map (`sessionId -> previousState`).
   - Cooldown tracking: `lastNotifiedAt` map (`sessionId -> timestampMs`).
   - Enforces 5.0s per-session cooldown with immediate suppression (no queueing / no replay).
   - Injects notification function (`notifyFn`, defaults to `Main.notify`).
   - Formats sanitized titles and static bodies with zero prompt leakage.
   - Memory cleanup: `cleanupSession(sessionId)` removes entries from `lastStates` and `lastNotifiedAt`.

4. **`extension/tests/test_settings.js`**
   - Automated GJS test suite testing `SettingsManager`: default values, typed getters, fallback mode, invalid string handling, and listener connection/disconnection.

5. **`extension/tests/test_notifications.js`**
   - Automated GJS test suite testing `NotificationManager`:
     - Transition edge detection (transitions into `WAITING` and `ERROR` notify; `WORKING`, `SUCCESS`, `IDLE` do not).
     - Suppression of identical state heartbeats (`WAITING` $\rightarrow$ `WAITING`).
     - 5.0-second cooldown enforcement (second alert within 2s suppressed; alert after 6s dispatched).
     - Verification that suppressed alerts are NOT queued for delayed replay.
     - Per-session isolation (alert in session A does not throttle session B).
     - Cooldown record cleanup on session removal.
     - Project name sanitization and prompt-free privacy verification.

6. **`extension/tests/test_accessibility.js`**
   - Automated GJS test suite verifying AT-SPI properties:
     - All 8 states produce correct accessible names and roles (`TOGGLE_BUTTON`).
     - Dynamic multi-session count announcements.
     - Offline state announcement.
     - Popover menu role (`MENU`) and session card role (`PANEL`).
     - Session card structured accessible names and descriptions (PID / tool).
     - Count badge exclusion from accessibility tree when count $\le 1$.

### Files to Modify

1. **`extension/utils.js`**
   - Add and export `sanitizeProjectName(rawName)`:
     - Strips newlines (`\n`), carriage returns (`\r`), tabs (`\t`), and control characters.
     - Strips markup characters (`<`, `>`, `&`).
     - Truncates to max 32 characters.
     - Falls back to `"workspace"` if empty or whitespace.

2. **`extension/indicator.js` (T063, T064, T065)**
   - Import `SettingsManager` and `NotificationManager`.
   - Wire `indicator-icon-style`: apply `.watchai-icon-colored` or `.watchai-icon-symbolic`.
   - Update `set_accessible_name()` and add accessible description `"Click to open agent session popover menu"`.
   - Wire session updates into `NotificationManager.onSessionUpdated()`.
   - Clean up notifications on `onSessionRemoved()`.

3. **`extension/popover.js` (T065)**
   - Update `WatchAISessionPopover`: set accessible role `Atk.Role.MENU` and accessible name `"WatchAI Agent Sessions"`.
   - Update `WatchAISessionCard`: set accessible role `Atk.Role.PANEL`.
   - Build structured accessible name: `"${providerDisplayName}, ${sanitizedProjectName}, state ${currentState}, duration ${formattedDuration}"`.
   - Build accessible description if PID or tool present: `"Process ID ${processId}, active tool ${activeToolCategory}"`.
   - Use `sanitizeProjectName()` for workspace labels.

4. **`extension/extension.js` (T063)**
   - Initialize `SettingsManager` in `enable()` and pass reference to `WatchAIIndicator`.
   - Cleanly destroy `SettingsManager` in `disable()`.

5. **`extension/stylesheet.css` (T063)**
   - Add `.watchai-icon-colored` and `.watchai-icon-symbolic` rules to dynamically toggle colored state accents vs monochrome symbolic styling on the top-bar icon.

6. **`extension/tests/test_indicator.js`**
   - Update with test assertions for icon presentation styling and accessibility descriptions.

7. **`extension/tests/test_popover.js`**
   - Update with test assertions for card accessible names, roles, and sanitized workspace names.

8. **`docs/architecture.md`**
   - Document Phase 10 desktop integration architecture (GSettings schema, notification throttling, AT-SPI accessibility mappings).

---

## 5. Architectural Responsibilities of Each Component

```
┌────────────────────────────────────────────────────────────────────────┐
│                        GNOME Shell Extension                           │
│                                                                        │
│  ┌───────────────────────┐              ┌───────────────────────────┐  │
│  │    SettingsManager    │              │    NotificationManager    │  │
│  │ (extension/settings.js)              │(extension/notifications.js)│  │
│  │ - GSettings Binding   │              │ - Edge-Triggered Alerts   │  │
│  │ - Headless Fallback   │              │ - 5.0s Cooldown / No-Queue│  │
│  │ - Dynamic Listeners   │              │ - Zero-Leakage Privacy    │  │
│  └───────────┬───────────┘              └─────────────▲─────────────┘  │
│              │                                        │                │
│              ▼                                        │                │
│  ┌────────────────────────────────────────────────────┴─────────────┐  │
│  │                         WatchAIIndicator                         │  │
│  │                    (extension/indicator.js)                      │  │
│  │ - Top-Bar Icon Presentation ('symbolic' vs 'colored')            │  │
│  │ - AT-SPI Accessible Name & Description (Role: TOGGLE_BUTTON)     │  │
│  │ - Forwards Session Updates to NotificationManager                │  │
│  └──────────────────────────────────┬───────────────────────────────┘  │
│                                     │                                  │
│                                     ▼                                  │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                   WatchAISessionPopover & Card                   │  │
│  │                      (extension/popover.js)                      │  │
│  │ - Popover Menu AT-SPI Name & Role (Role: MENU)                   │  │
│  │ - Session Card AT-SPI Name & Description (Role: PANEL)           │  │
│  │ - Sanitized Workspace Project Names (max 32 chars)               │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────▲──────────────────────────────────┘
                                      │ D-Bus Signals (Unchanged)
                                      │ (SessionUpdated, AggregateStateChanged)
┌─────────────────────────────────────┴──────────────────────────────────┐
│                   WatchAI Daemon & Core (Rust Tier)                    │
│                     (Authoritative Lifecycle & Dwell)                  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Implementation Ordering & Dependency Graph

```
Step 1: Schema Definition & Validation
        extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml
        [glib-compile-schemas --strict --dry-run validation]
             │
             ▼
Step 2: Project Name Sanitization
        extension/utils.js (sanitizeProjectName helper)
             │
             ├───────────────────────────────────────┐
             ▼                                       ▼
Step 3: Settings Management             Step 4: Notification Management
        extension/settings.js                   extension/notifications.js
        extension/tests/test_settings.js        extension/tests/test_notifications.js
             │                                       │
             └───────────────────┬───────────────────┘
                                 ▼
Step 5: Top-Bar Indicator & CSS Accents
        extension/stylesheet.css
        extension/indicator.js
        extension/tests/test_indicator.js
             │
             ▼
Step 6: Popover & Session Card Accessibility
        extension/popover.js
        extension/tests/test_popover.js
        extension/tests/test_accessibility.js
             │
             ▼
Step 7: Extension Lifecycle & Cleanup Wiring
        extension/extension.js
             │
             ▼
Step 8: Full Verification Suite
        - glib-compile-schemas compilation
        - Headless GJS unit test suite (5 suites)
        - cargo test --workspace (zero Rust regression)
             │
             ▼
Step 9: Documentation Synchronization
        - docs/architecture.md
```

---

## 7. Test Strategy & Exact Test Locations

### Automated GJS Unit Tests (Headless Execution)
Each test runs cleanly via `gjs -m <test-file>` without requiring an active GNOME Shell graphical session or X11/Wayland display:

1. **`extension/tests/test_settings.js`**:
   - Verify default schema key values (`dwell-duration-seconds: 10`, `enable-desktop-notifications: true`, `notify-on-waiting: true`, `notify-on-error: true`, `indicator-icon-style: 'symbolic'`).
   - Verify fallback mechanism when compiled schema is absent.
   - Verify deterministic fallback to `'symbolic'` when invalid style string (e.g. `'neon'`) is passed.
   - Verify signal listener dispatch and `disconnectAll()` cleanup.

2. **`extension/tests/test_notifications.js`**:
   - Verify state transition edge detection: `WORKING` $\rightarrow$ `WAITING` triggers notification; `IDLE` $\rightarrow$ `WORKING` does not.
   - Verify identical state suppression: `WAITING` $\rightarrow$ `WAITING` heartbeat emits zero notifications.
   - Verify 5.0-second cooldown enforcement: second `WAITING` transition at $T=2\text{s}$ is suppressed; third transition at $T=6\text{s}$ is dispatched.
   - Verify suppressed alerts are discarded and NOT queued for delayed replay.
   - Verify per-session cooldown isolation: alert in session A does not throttle session B.
   - Verify session cleanup: `onSessionRemoved(sessionId)` deletes cooldown tracking state.
   - Verify project name sanitization: strips newlines, carriage returns, tabs, `<tags>`, limits to 32 chars, falls back to `"workspace"`.
   - Verify Zero-Leakage Privacy: title and body strictly match approved templates.

3. **`extension/tests/test_accessibility.js`**:
   - Verify top-bar indicator accessible names for all 8 states (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`).
   - Verify multi-session counter string: `" (${activeCount} active sessions)"`.
   - Verify offline accessible name: `"WatchAI daemon offline"`.
   - Verify accessible description: `"Click to open agent session popover menu"`.
   - Verify popover role `Atk.Role.MENU` and accessible name `"WatchAI Agent Sessions"`.
   - Verify session card role `Atk.Role.PANEL`, accessible name, and accessible description (PID / tool).
   - Verify count badge exclusion when `activeCount <= 1`.

4. **Existing Regression Test Suites**:
   - `extension/tests/test_indicator.js`
   - `extension/tests/test_popover.js`
   - `extension/tests/test_reconnect.js`

### Schema Syntax & Compilation Validation
- Run `/usr/bin/glib-compile-schemas --strict --dry-run extension/schemas/` to guarantee that the XML schema is syntactically valid and satisfies all GLib schema constraints.

### Rust Workspace Regression Verification
- Run `cargo test --workspace` (all 68 unit/integration tests) to ensure zero regressions in backend daemon, adapters, core FSM, and D-Bus IPC.

---

## 8. Schema Compilation & Testing Strategy

To avoid brittle environment coupling:
1. **Source Schema**: Maintained in XML at `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml`.
2. **Local Compilation**: Compiled via `glib-compile-schemas extension/schemas/` during extension build/install.
3. **Headless Test Ergonomics**: `SettingsManager` includes a mock/in-memory fallback adapter that automatically activates if `gschemas.compiled` is not found, logging a non-fatal warning while providing full preference read/write/notify semantics for headless test runners.

---

## 9. Regression Risks & Mitigations

| Risk | Impact | Mitigation |
| :--- | :---: | :--- |
| **GSettings Signal Leakage** | Memory leak / GNOME Shell crash across extension reloads. | `SettingsManager` tracks all handler IDs and explicitly disconnects every listener in `destroy()`. |
| **Notification Spam / Alert Storms** | Distraction and alert fatigue for developers during active tool execution. | Strict transition edge detection ($S_{t-1} \neq S_t$) + 5.0-second per-session cooldown + immediate suppression without queueing. |
| **Unbounded Cooldown Memory Growth** | Memory accumulation from many ephemeral agent sessions. | `NotificationManager` purges tracking entries immediately when `onSessionRemoved(sessionId)` is received. |
| **State Divergence on Dwell Setting** | Top bar showing `SUCCESS` while daemon reports `IDLE`, or vice versa. | Explicitly bound `dwell-duration-seconds` as an informational user preference. The extension strictly follows daemon D-Bus signals and does NOT synthesize client-side dwell delays. |
| **Markup Injection in Notifications** | Desktop notification crash or formatting corruption from unusual workspace folder names. | `sanitizeProjectName()` strips XML/HTML/Pango markup (`<`, `>`, `&`), control characters, and truncates to 32 characters. |
| **Screen Reader Verbosity** | Assistive technologies overwhelmed by frequent badge updates. | Count badge excluded from accessibility tree when count $\le 1$; announced atomically as part of the indicator's accessible name when $> 1$. |

---

## 10. Documentation Updates

1. **`docs/architecture.md`**:
   - Document Phase 10 desktop integration layer:
     - GSettings configuration schema structure.
     - Desktop notification transition matrix and 5.0-second cooldown policy.
     - AT-SPI accessibility mappings across all widgets.
2. **`specs/001-agent-status-indicator/tasks.md`**:
   - Synchronize completion of tasks `T062`–`T065` upon implementation completion.
   - Verify Task `T022` remains permanently discovery-gated and unchecked `[ ]`.
