---
description: "Actionable implementation task breakdown for WatchAI Phase 10 — Configuration, Notifications & Accessibility Polish"
---

# Tasks: Configuration, Notifications & Accessibility Polish

**Input**: Design documents from `/specs/005-settings-notifications-accessibility/` (`spec.md`, `plan.md`, `checklists/requirements.md`, `checklists/implementation.md`).
**Prerequisites**: Approved specification, clarified failure decisions, constitution v1.0.0, Phase 9 merged (commit `0b99935`).
**Branch**: `010-settings-notifications-accessibility`

## Format: `- [ ] [TaskID] [P?] [US?] [CanonicalID] Description with file path`
- **[P]**: Can run in parallel (different files, no blocking dependencies).
- **[US?]**: Maps task directly to user stories (`[US1]`, `[US2]`, `[US3]`) from `spec.md`.
- **[CanonicalID]**: Traces directly to canonical tasks (`[T062]`, `[T063]`, `[T064]`, `[T065]`) from `specs/001-agent-status-indicator/tasks.md`.
- **Traceability**: Every task cites corresponding `FR-*` requirements, `CHK-*` checklist items, and verification evidence.

---

## Phase 1: Setup & Foundational Infrastructure

**Purpose**: Define the GSettings XML schema, validate schema syntax via GLib tooling, and implement defensive project workspace name sanitization.

- [X] T126 [P] [T062] Create GSettings XML schema file defining schema ID `org.gnome.shell.extensions.watchai`, path `/org/gnome/shell/extensions/watchai/`, keys `dwell-duration-seconds` (`type="u"`, `<range min="1" max="60"/>`, default `10`), `enable-desktop-notifications` (`type="b"`, default `true`), `notify-on-waiting` (`type="b"`, default `true`), `notify-on-error` (`type="b"`, default `true`), and `indicator-icon-style` (`type="s"`, `<choices><choice value="symbolic"/><choice value="colored"/></choices>`, default `'symbolic'`) in `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml`.
  *Traceability*: FR-001, FR-002, FR-003, FR-004, FR-005, FR-006; CHK-SET-001, CHK-SET-002, CHK-SET-003, CHK-SET-004, CHK-SET-005, CHK-SET-006.
  *Verification*: Schema file inspection.

- [X] T127 [T062] Validate GSettings XML schema syntax and constraint legality using `/usr/bin/glib-compile-schemas --strict --dry-run extension/schemas/` to ensure zero compilation warnings or errors (depends on T126).
  *Traceability*: CHK-SET-007.
  *Verification*: Shell command exit code 0.

- [X] T128 [P] [T064] Implement `sanitizeProjectName(rawName)` helper function in `extension/utils.js` stripping newlines (`\n`), carriage returns (`\r`), tabs (`\t`), control characters, and markup tags (`<`, `>`, `&`), capping string length at 32 characters, and falling back to `"workspace"` if input is null, empty, or whitespace-only.
  *Traceability*: FR-017; CHK-PRIV-004.
  *Verification*: Unit tests in T134.

**Checkpoint**: XML schema is strictly validated by GLib tools, and workspace name sanitization is available for notifications and accessibility labels.

---

## Phase 2: User Story 1 - Desktop User Preferences via GSettings [US1] 🎯 MVP

**Goal**: Implement `SettingsManager` in GJS with dynamic `changed::` signal listeners, clean signal teardown, headless fallback, invalid icon style fallback, and CSS presentation styling.

**Independent Test**: Read and mutate keys using `SettingsManager` in headless GJS tests (`gjs -m extension/tests/test_settings.js`), asserting default values, listener dispatch, invalid string fallback to `'symbolic'`, and clean signal disconnection.

### Tests for User Story 1
- [X] T129 [P] [US1] [T063] Write automated GJS unit test suite in `extension/tests/test_settings.js` testing `SettingsManager`:
  - Asserting default values (`dwell-duration-seconds: 10`, `enable-desktop-notifications: true`, `notify-on-waiting: true`, `notify-on-error: true`, `indicator-icon-style: 'symbolic'`).
  - Asserting fallback mechanism when `gschemas.compiled` is absent.
  - Asserting fallback boundary safety (fallback activates strictly on missing schema file and does not swallow syntax or unexpected runtime errors).
  - Asserting deterministic fallback to `'symbolic'` when invalid style string (e.g. `'neon'`) is read.
  - Asserting dynamic `changed::` signal listener dispatch and `disconnectAll()` cleanup.
  *Traceability*: FR-006, FR-007, FR-008, FR-009; Acceptance Scenarios 1.1, 1.2, 1.4, 1.5; CHK-MGR-004, CHK-MGR-005, CHK-MGR-006, CHK-MGR-007.
  *Verification*: `gjs -m extension/tests/test_settings.js`.

### Implementation for User Story 1
- [X] T130 [US1] [T063] Implement `SettingsManager` class in `extension/settings.js` using modern GNOME 45+ ESM APIs (`this.getSettings()`), providing typed accessors (`getIconStyle()`, `getEnableNotifications()`, `getNotifyOnWaiting()`, `getNotifyOnError()`, `getDwellDurationSeconds()`), tracking all signal listener IDs for clean `disconnectAll()` teardown, implementing headless in-memory fallback for missing `gschemas.compiled`, and enforcing deterministic fallback to `'symbolic'` on invalid icon style values (depends on T129).
  *Traceability*: FR-006, FR-007, FR-008, FR-009; CHK-MGR-001, CHK-MGR-002, CHK-MGR-003, CHK-MGR-004, CHK-MGR-005, CHK-MGR-006.
  *Verification*: Passes T129 test suite.

- [X] T131 [P] [US1] [T063] Add `.watchai-icon-colored` and `.watchai-icon-symbolic` styling classes to `extension/stylesheet.css` to permit dynamic toggling between full-color state accents and standard monochrome symbolic presentation without altering base icon sizing or geometry.
  *Traceability*: FR-006; Acceptance Scenario 1.1; CHK-ICON-001, CHK-ICON-005.
  *Verification*: CSS inspection and T133 tests.

- [X] T132 [US1] [T063] Update `WatchAIIndicator` in `extension/indicator.js` to accept `SettingsManager`, apply `.watchai-icon-colored` or `.watchai-icon-symbolic` based on `settings.getIconStyle()`, and re-apply icon style immediately upon `changed::indicator-icon-style` signal emission without extension restart (depends on T130, T131).
  *Traceability*: FR-008; Acceptance Scenario 1.1; CHK-ICON-002, CHK-ICON-003, CHK-ICON-004.
  *Verification*: Passes T133 test suite.

- [X] T133 [US1] [T063] Update `extension/tests/test_indicator.js` to verify indicator CSS class generation under both `'symbolic'` and `'colored'` configuration modes (depends on T132).
  *Traceability*: CHK-ICON-002, CHK-ICON-003, CHK-ICON-004; CHK-REG-005.
  *Verification*: `gjs -m extension/tests/test_indicator.js`.

**Checkpoint**: User Story 1 provides full user preference management via GSettings and dynamic icon styling.

---

## Phase 3: User Story 2 - Actionable Desktop Notifications & Cooldown Semantics [US2]

**Goal**: Implement `NotificationManager` with edge-triggered alert detection entering `WAITING` or `ERROR`, an invariant 5.0-second per-session cooldown with immediate suppression (no queueing / no replay), per-session memory cleanup, and zero-leakage privacy.

**Independent Test**: Dispatch simulated session lifecycle events in headless GJS (`gjs -m extension/tests/test_notifications.js`), asserting transition edge gating, identical state suppression, 5.0s cooldown enforcement, immediate suppression without queueing, per-session isolation, and memory cleanup.

### Tests for User Story 2
- [X] T134 [P] [US2] [T064] Write automated GJS unit test suite in `extension/tests/test_notifications.js` testing `NotificationManager`:
  - Asserting transition edge detection: transitions into `WAITING` and `ERROR` notify; transitions into `WORKING`, `SUCCESS`, `STARTING`, `CANCELLED`, `UNKNOWN`, or `IDLE` never notify.
  - Asserting identical state suppression: `WAITING` $\rightarrow$ `WAITING` and `ERROR` $\rightarrow$ `ERROR` heartbeats emit zero notifications.
  - Asserting master toggle gating: `enable-desktop-notifications: false` silences all notifications.
  - Asserting granular toggle gating: `notify-on-waiting` and `notify-on-error` selectively gate alerts.
  - Asserting 5.0-second per-session cooldown: transition at $T=2\text{s}$ is suppressed immediately; transition at $T=6\text{s}$ notifies normally and resets cooldown.
  - Asserting immediate suppression / no-queue policy: suppressed alerts are discarded and NOT replayed when the 5.0-second cooldown expires.
  - Asserting per-session cooldown isolation: alert in session $A$ does not throttle session $B$.
  - Asserting session cleanup: `cleanupSession(sessionId)` purges tracking state from memory.
  - Asserting `sanitizeProjectName()` strips newlines, control characters, markup tags, caps at 32 chars, and defaults to `"workspace"`.
  - Asserting Zero-Leakage Privacy: notification title matches `WatchAI: <Provider> (<Sanitized Project>)` and body strictly uses approved static templates without prompts, arguments, diffs, or secrets.
  *Traceability*: FR-010, FR-011, FR-012, FR-013, FR-014, FR-015, FR-016, FR-017, FR-018, FR-019; Acceptance Scenarios 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8, 2.9, 2.10; CHK-NOTIF-001 through CHK-NOTIF-012; CHK-PRIV-001 through CHK-PRIV-005.
  *Verification*: `gjs -m extension/tests/test_notifications.js`.

### Implementation for User Story 2
- [X] T135 [US2] [T064] Implement `NotificationManager` class in `extension/notifications.js` taking `SettingsManager` and optional injectable `notifyFn` (defaults to `Main.notify`), tracking previous states in `lastStates` map, tracking cooldown timestamps in `lastNotifiedAt` map, enforcing 5.0-second per-session cooldown with immediate suppression (no queueing / no replay), formatting titles with `sanitizeProjectName()`, using generic static notification bodies, and providing `cleanupSession(sessionId)` to purge tracking memory (depends on T128, T134).
  *Traceability*: FR-010 through FR-019; CHK-NOTIF-001 through CHK-NOTIF-011; CHK-PRIV-001 through CHK-PRIV-004.
  *Verification*: Passes T134 test suite.

- [X] T136 [US2] [T064] Wire `NotificationManager` into `WatchAIIndicator` in `extension/indicator.js`, forwarding `onSessionUpdated` events from D-Bus client to `NotificationManager.handleSessionUpdated(session)` (depends on T135).
  *Traceability*: FR-010, FR-011; CHK-NOTIF-001.
  *Verification*: Verified in T134 and T142.

- [X] T137 [US2] [T064] Wire session removal cleanup in `WatchAIIndicator` in `extension/indicator.js`, forwarding `onSessionRemoved` events from D-Bus client to `NotificationManager.cleanupSession(sessionId)` to purge cooldown tracking records from memory (depends on T135, T136).
  *Traceability*: FR-015; Acceptance Scenario 2.8; CHK-NOTIF-010.
  *Verification*: Verified in T134 and T142.

**Checkpoint**: User Story 2 delivers robust, rate-limited, privacy-compliant desktop notifications.

---

## Phase 4: User Story 3 - Full Screen-Reader & AT-SPI Accessibility [US3]

**Goal**: Standardize AT-SPI roles, accessible names, descriptions, and counter behavior across the top-bar indicator, popover menu, and session cards.

**Independent Test**: Verify AT-SPI mappings in headless GJS (`gjs -m extension/tests/test_accessibility.js`), asserting accessible names for all 8 states, dynamic multi-session counter expansion, offline announcement, `MENU`/`PANEL` roles, and count badge exclusion when active count $\le 1$.

### Tests for User Story 3
- [X] T138 [P] [US3] [T065] Write automated GJS unit test suite in `extension/tests/test_accessibility.js` verifying:
  - Accessible names across all 8 lifecycle states (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`).
  - Accessible description: `"Click to open agent session popover menu"`.
  - Dynamic multi-session counter announcement (`" (${activeCount} active sessions)"` when `activeCount > 1`).
  - Offline accessible name: `"WatchAI daemon offline"`.
  - Popover menu role `Atk.Role.MENU` and accessible name `"WatchAI Agent Sessions"`.
  - Session card role `Atk.Role.PANEL`, structured accessible name (`"${providerDisplayName}, ${sanitizedProjectName}, state ${currentState}, duration ${formattedDuration}"`), and accessible description (`"Process ID ${processId}, active tool ${activeToolCategory}"`).
  - Count badge exclusion from accessibility tree when `activeCount <= 1`.
  *Traceability*: FR-020, FR-021, FR-022, FR-023, FR-024, FR-025, FR-026, FR-027; Acceptance Scenarios 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7; CHK-A11Y-001 through CHK-A11Y-010.
  *Verification*: `gjs -m extension/tests/test_accessibility.js`.

### Implementation for User Story 3
- [X] T139 [US3] [T065] Update `WatchAIIndicator` in `extension/indicator.js` to set accessible role `Atk.Role.TOGGLE_BUTTON` (or `PUSH_BUTTON`), set accessible description to `"Click to open agent session popover menu"`, ensure accessible names cover all 8 states with multi-session count expansion (`activeCount > 1`), maintain offline accessible name `"WatchAI daemon offline"`, and exclude the count badge from the AT-SPI accessibility tree when `activeCount <= 1` (depends on T138).
  *Traceability*: FR-020, FR-021, FR-022, FR-023, FR-027; CHK-A11Y-001, CHK-A11Y-002, CHK-A11Y-003, CHK-A11Y-004, CHK-A11Y-005, CHK-A11Y-009.
  *Verification*: Passes T138 test suite.

- [X] T140 [US3] [T065] Update `WatchAISessionPopover` and `WatchAISessionCard` in `extension/popover.js` to set popover accessible role `Atk.Role.MENU` with accessible name `"WatchAI Agent Sessions"`, set session card accessible role `Atk.Role.PANEL` with structured accessible name (`"${providerDisplayName}, ${sanitizedProjectName}, state ${currentState}, duration ${formattedDuration}"`), and set accessible description (`"Process ID ${processId}, active tool ${activeToolCategory}"`) when PID or tool category is present, utilizing `sanitizeProjectName()` for workspace labels (depends on T128, T138).
  *Traceability*: FR-024, FR-025, FR-026; CHK-A11Y-006, CHK-A11Y-007, CHK-A11Y-008.
  *Verification*: Passes T138 test suite.

- [X] T141 [US3] [T065] Update existing tests in `extension/tests/test_popover.js` and `extension/tests/test_indicator.js` to assert the updated accessibility roles, descriptions, and sanitized project names without regressing existing card sorting or liveness behavior (depends on T139, T140).
  *Traceability*: CHK-REG-005, CHK-REG-006.
  *Verification*: `gjs -m extension/tests/test_popover.js` and `gjs -m extension/tests/test_indicator.js`.

**Checkpoint**: User Story 3 delivers comprehensive, HIG-compliant AT-SPI screen-reader accessibility.

---

## Phase 5: Extension Lifecycle Wiring, Verification & Governance Polish

**Purpose**: Wire settings and notifications into the top-level extension entry point, verify clean resource teardown, execute the full test suite, and synchronize documentation.

- [X] T142 Update `WatchAIExtension` in `extension/extension.js` to instantiate `SettingsManager` in `enable()`, pass it to `WatchAIIndicator`, and cleanly call `destroy()` on `SettingsManager` in `disable()` to disconnect all GSettings signal handlers and prevent memory leaks (depends on T130, T132, T136, T137).
  *Traceability*: FR-007, FR-009; Acceptance Scenario 1.4; CHK-CLN-001, CHK-CLN-003.
  *Verification*: GJS enable/disable cycle tests.

- [X] T143 Execute all automated GJS test suites in `extension/tests/` to verify zero regressions across the entire extension:
  - `gjs -m extension/tests/test_settings.js` (T129)
  - `gjs -m extension/tests/test_notifications.js` (T134)
  - `gjs -m extension/tests/test_accessibility.js` (T138)
  - `gjs -m extension/tests/test_indicator.js` (T133)
  - `gjs -m extension/tests/test_popover.js` (T141)
  - `gjs -m extension/tests/test_reconnect.js` (existing regression suite) (depends on T142).
  *Traceability*: CHK-REG-004, CHK-REG-005, CHK-REG-006.
  *Verification*: All 6 test suites exit with code 0.

- [X] T144 [P] Run Rust workspace tests (`cargo test --workspace`) to verify zero regressions across all daemon, adapter, core domain, IPC, and integration test crates (68 tests total).
  *Traceability*: CHK-REG-007.
  *Verification*: `cargo test --workspace` exits with code 0.

- [X] T145 [P] Update `docs/architecture.md` to document the Phase 10 desktop integration layer:
  - Document the GSettings schema structure, preference keys, and informational `dwell-duration-seconds` boundary.
  - Document the notification transition state matrix, 5.0-second per-session cooldown, and immediate suppression policy.
  - Document the AT-SPI accessibility mappings across all top-bar and popover widgets.
  *Traceability*: CHK-DOC-001.
  *Verification*: Architecture documentation review.

- [X] T146 Verify governance and specification invariants:
  - Confirm task `T022` in `specs/001-agent-status-indicator/tasks.md:74` remains strictly discovery-gated and unchecked `[ ]`.
  - Confirm D-Bus IPC contract `(sssssssus)` and interface `org.freedesktop.WatchAI` are 100% unchanged.
  - Confirm daemon/core lifecycle and dwell semantics are 100% unchanged.
  - Synchronize canonical tasks `T062`, `T063`, `T064`, and `T065` in `specs/001-agent-status-indicator/tasks.md` to completed `[x]` ONLY after all verification gates pass (depends on T143, T144).
  *Traceability*: CHK-REG-001, CHK-REG-002, CHK-REG-003, CHK-DOC-002.
  *Verification*: Git diff inspection of `specs/001-agent-status-indicator/tasks.md`.

---

## Phase 10 Task Dependency & Order Flow

```
T126 (Schema XML) ──► T127 (glib-compile-schemas validation)
         │
         ▼
T128 (sanitizeProjectName helper)
         │
         ├────────────────────────────────────────┐
         ▼                                        ▼
T129 (test_settings.js)                  T134 (test_notifications.js)
         │                                        │
         ▼                                        ▼
T130 (SettingsManager)                   T135 (NotificationManager)
         │                                        │
         ├───────────────────┬────────────────────┘
         ▼                   ▼
T131 (stylesheet.css)   T136 (Wire notifications to indicator)
         │                   │
         ▼                   ▼
T132 (indicator.js)     T137 (Wire session removal cleanup)
         │                   │
         ▼                   ▼
T133 (test_indicator)   T138 (test_accessibility.js)
                             │
                             ▼
                        T139 (indicator a11y)
                             │
                             ▼
                        T140 (popover a11y)
                             │
                             ▼
                        T141 (test_popover)
                             │
                             ▼
                        T142 (extension.js lifecycle wiring)
                             │
                             ▼
                        T143 (All 6 GJS test suites pass)
                             │
                             ├────────────────────┐
                             ▼                    ▼
                        T144 (cargo test)    T145 (docs/architecture.md)
                             │                    │
                             └──────────┬─────────┘
                                        ▼
                                   T146 (Governance & task sync)
```
