# Implementation Checklist: Phase 10 — Configuration, Notifications & Accessibility Polish

**Purpose**: Implementation-oriented verification checklist traceable to FR requirements, acceptance scenarios, canonical tasks T062–T065, and Phase 10 implementation plan.
**Created**: 2026-10-06
**Feature**: [spec.md](../spec.md) | [plan.md](../plan.md) | [requirements.md](requirements.md)

**Review Ownership**: Reviewer-owned implementation quality and verification gate artifact.
**Marker Semantics**:
- `[ ]` Open verification item (must be verified before Phase 10 implementation is considered complete).
- `[x]` Verified complete with automated test and/or inspection evidence.

---

## 1. GSettings Schema Correctness (T062, FR-001–FR-006)

*Validates XML schema syntax, key naming, types, constraints, and compile verification.*

- [x] `CHK-SET-001` XML schema file exists at `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml` with schema ID `org.gnome.shell.extensions.watchai` and path `/org/gnome/shell/extensions/watchai/`. [FR-001]
- [x] `CHK-SET-002` Key `dwell-duration-seconds` is defined as `type="u"`, default `10`, with valid range `1` to `60` enforced via `<range min="1" max="60"/>`. [FR-002]
- [x] `CHK-SET-003` Key `enable-desktop-notifications` is defined as `type="b"` with default `true`. [FR-003]
- [x] `CHK-SET-004` Key `notify-on-waiting` is defined as `type="b"` with default `true`. [FR-004]
- [x] `CHK-SET-005` Key `notify-on-error` is defined as `type="b"` with default `true`. [FR-005]
- [x] `CHK-SET-006` Key `indicator-icon-style` is defined as `type="s"` with `<choices><choice value="symbolic"/><choice value="colored"/></choices>` and default `'symbolic'`. [FR-006]
- [x] `CHK-SET-007` Schema compiles cleanly without warnings or errors via `/usr/bin/glib-compile-schemas --strict --dry-run extension/schemas/`. [Validation]

---

## 2. SettingsManager Implementation & Fallback (T063, FR-007–FR-009)

*Validates GNOME 45+ ESM settings binding, dynamic listeners, fallback ergonomics, and teardown.*

- [x] `CHK-MGR-001` `SettingsManager` class implemented in `extension/settings.js` using modern GNOME 45+ ESM APIs (`this.getSettings()`). [FR-007]
- [x] `CHK-MGR-002` Dynamic `changed::` signal listeners registered for all 5 preference keys, immediately notifying subscribers when preferences change. [FR-008]
- [x] `CHK-MGR-003` All signal listener IDs are tracked internally and explicitly disconnected in `disconnectAll()` / `destroy()`. [FR-009]
- [x] `CHK-MGR-004` Headless/uncompiled-schema fallback: when `getSettings()` throws (e.g. `gschemas.compiled` missing in headless GJS test runner), safely activates in-memory fallback settings object with standard defaults (`symbolic`, 10s dwell, true toggles) and logs a non-fatal warning. [FR-007, Scenario 1.5]
- [x] `CHK-MGR-005` Fallback boundary safety: fallback activates ONLY when schema lookup fails due to missing schema file; does NOT mask syntax errors, corrupt schemas, or unrelated runtime exceptions. [Architecture Constraint 4]
- [x] `CHK-MGR-006` Runtime invalid icon style handling: if `get_string('indicator-icon-style')` returns an unexpected or unlisted string, deterministically falls back to `'symbolic'`. [FR-006, Scenario 1.2]
- [x] `CHK-MGR-007` Automated GJS test suite in `extension/tests/test_settings.js` verifies default values, typed getters, dynamic listener dispatch, invalid string fallback, and clean disconnects. [Test Evidence]

---

## 3. NotificationManager & Cooldown Semantics (T064, FR-010–FR-015)

*Validates edge-triggered alert detection, 5.0s per-session cooldown, immediate suppression, and cleanup.*

- [x] `CHK-NOTIF-001` `NotificationManager` class implemented in `extension/notifications.js` tracking state transitions per session ($S_{t-1} \neq S_t$) using an in-memory `lastStates` map. [FR-012]
- [x] `CHK-NOTIF-002` Edge-triggered notification: only transitions INTO `WAITING` or `ERROR` trigger notification evaluation. Transitions into `WORKING`, `SUCCESS`, `STARTING`, `CANCELLED`, `UNKNOWN`, or `IDLE` never emit notifications. [FR-010, FR-011]
- [x] `CHK-NOTIF-003` Identical state suppression: repeated events with unchanged state (`WAITING` $\rightarrow$ `WAITING` or `ERROR` $\rightarrow$ `ERROR`) emit zero notifications. [FR-012, Scenario 2.6]
- [x] `CHK-NOTIF-004` Master toggle gating: when `enable-desktop-notifications` is `false`, zero notifications are dispatched regardless of state transitions. [FR-010, FR-011, Scenario 2.9]
- [x] `CHK-NOTIF-005` Granular toggle gating: `notify-on-waiting` selectively gates `WAITING` alerts; `notify-on-error` selectively gates `ERROR` alerts. [FR-010, FR-011]
- [x] `CHK-NOTIF-006` 5.0-second per-session cooldown: if transition into `WAITING` or `ERROR` occurs within $< 5.0\text{s}$ of previous notification for that session, notification is suppressed immediately. [FR-013, Scenario 2.2, Scenario 2.4]
- [x] `CHK-NOTIF-007` Immediate suppression / No-queue policy: suppressed notifications are discarded immediately and are NOT queued or replayed when the 5.0-second cooldown expires. [FR-013, Scenario 2.2]
- [x] `CHK-NOTIF-008` Cooldown reset: transition into `WAITING` or `ERROR` occurring $\ge 5.0\text{s}$ after previous notification notifies normally and resets the session's cooldown timestamp. [FR-013, Scenario 2.3, Scenario 2.5]
- [x] `CHK-NOTIF-009` Per-session cooldown isolation: an alert in session $A$ does not throttle or delay notification for session $B$. [FR-014, Scenario 2.7]
- [x] `CHK-NOTIF-010` Cooldown memory cleanup: `cleanupSession(sessionId)` (triggered by `onSessionRemoved`) purges tracking records from `lastStates` and `lastNotifiedAt`. [FR-015, Scenario 2.8]
- [x] `CHK-NOTIF-011` Injectable notification sink (`notifyFn`, defaults to `Main.notify`) enables 100% deterministic assertion in headless GJS test suites. [Architecture Constraint 7]
- [x] `CHK-NOTIF-012` Automated GJS test suite in `extension/tests/test_notifications.js` verifies transition detection, identical state suppression, 5.0s cooldown, no-replay, per-session isolation, and session cleanup. [Test Evidence]

---

## 4. Privacy & Project-Name Sanitization (FR-016–FR-019)

*Validates zero-leakage privacy invariants and defensive metadata sanitization.*

- [x] `CHK-PRIV-001` Zero-Leakage Privacy Invariant: dispatched notifications and accessibility labels strictly contain ZERO prompt text, tool arguments, source code diffs, command lines, credentials, tokens, or full filesystem paths. [FR-019, Scenario 2.10]
- [x] `CHK-PRIV-002` Notification title standardized to `WatchAI: <Provider Display Name> (<Sanitized Project Name>)`. [FR-016]
- [x] `CHK-PRIV-003` Notification body strictly uses approved static templates: `"Agent is waiting for user input or approval."` for `WAITING` and `"Agent encountered an error or crashed."` for `ERROR`. [FR-018]
- [x] `CHK-PRIV-004` Helper `sanitizeProjectName()` implemented in `extension/utils.js`:
  - Strips newlines (`\n`), carriage returns (`\r`), tabs (`\t`), and non-printable control characters.
  - Strips HTML/XML/Pango markup tags (`<`, `>`, `&`).
  - Truncates length to maximum 32 characters.
  - Falls back to `"workspace"` if input is null, empty, or consists solely of whitespace. [FR-017]
- [x] `CHK-PRIV-005` Automated tests in `extension/tests/test_notifications.js` assert privacy compliance and adversarial sanitization against pathological strings (newlines, tags, overly long paths). [Test Evidence]

---

## 5. AT-SPI Accessibility & Screen Reader Polish (T065, FR-020–FR-027)

*Validates AT-SPI roles, accessible names, descriptions, and dynamic counter updates.*

- [x] `CHK-A11Y-001` Verified GNOME Shell / GJS AT-SPI roles:
  - Top-bar indicator button: `Atk.Role.TOGGLE_BUTTON` / `PUSH_BUTTON` (inherited natively by `PanelMenu.Button`).
  - Popover menu container: `Atk.Role.MENU`.
  - Session cards: `Atk.Role.PANEL`. [FR-020, FR-024, FR-025]
- [x] `CHK-A11Y-002` Top-bar indicator button accessible name covers all 8 lifecycle states:
  `IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`. [FR-020, Scenario 3.1]
- [x] `CHK-A11Y-003` Top-bar indicator button accessible name dynamically appends `" (${activeCount} active sessions)"` when `activeCount > 1`. [FR-021, Scenario 3.2]
- [x] `CHK-A11Y-004` Offline accessible name reports `"WatchAI daemon offline"`. [FR-022, Scenario 3.3]
- [x] `CHK-A11Y-005` Top-bar indicator button sets accessible description to `"Click to open agent session popover menu"`. [FR-023]
- [x] `CHK-A11Y-006` Popover menu sets accessible name to `"WatchAI Agent Sessions"` and role `Atk.Role.MENU`. [FR-024, Scenario 3.4]
- [x] `CHK-A11Y-007` Session card sets accessible role `Atk.Role.PANEL` and structured accessible name:
  `"${providerDisplayName}, ${sanitizedProjectName}, state ${currentState}, duration ${formattedDuration}"`. [FR-025, Scenario 3.5]
- [x] `CHK-A11Y-008` Session card sets accessible description if PID or tool present: `"Process ID ${processId}, active tool ${activeToolCategory}"`. [FR-026, Scenario 3.6]
- [x] `CHK-A11Y-009` Top-bar count badge is excluded from screen-reader accessibility tree when `activeCount <= 1` to prevent redundant speech output. [FR-027, Scenario 3.7]
- [x] `CHK-A11Y-010` Automated GJS test suite in `extension/tests/test_accessibility.js` verifies all accessibility mappings, roles, descriptions, and counter updates. [Test Evidence]

---

## 6. Top-Bar Icon Presentation Styling (T063, FR-006, FR-008)

*Validates symbolic vs colored styling modes and dynamic CSS application.*

- [x] `CHK-ICON-001` Stylesheet in `extension/stylesheet.css` defines `.watchai-icon-colored` and `.watchai-icon-symbolic`. [Style Definition]
- [x] `CHK-ICON-002` When `indicator-icon-style` is `'symbolic'` (default), top-bar icon applies symbolic styling without color accents. [Scenario 1.1]
- [x] `CHK-ICON-003` When `indicator-icon-style` is `'colored'`, top-bar icon applies state accent classes corresponding to current aggregate state. [Scenario 1.1]
- [x] `CHK-ICON-004` Changing `indicator-icon-style` in GSettings immediately updates top-bar icon class without extension restart. [Scenario 1.1]
- [x] `CHK-ICON-005` Existing indicator visual styles, icon sizes, and spacing remain un-regressed. [Regression]

---

## 7. Contract Stability & Regression Verification

*Validates backward-compatibility, architectural invariants, and test baselines.*

- [x] `CHK-REG-001` Daemon lifecycle ownership preserved: `watchai-core` remains authoritative for all session states and completion dwell (`COMPLETION_DWELL_SECONDS = 10`). Extension does NOT synthesize client-side dwell delays. [Architecture Constraint 1]
- [x] `CHK-REG-002` D-Bus IPC contract preserved: wire protocol `(sssssssus)` and interface `org.freedesktop.WatchAI` remain 100% unchanged. [Architecture Constraint 2]
- [x] `CHK-REG-003` Discovery-gated task `T022` in `specs/001-agent-status-indicator/tasks.md:74` remains untouched and strictly unchecked `[ ]`. [Constitution Check]
- [x] `CHK-REG-004` Existing reconnect test suite `extension/tests/test_reconnect.js` passes with zero failures (`gjs -m extension/tests/test_reconnect.js`). [Regression]
- [x] `CHK-REG-005` Existing indicator test suite `extension/tests/test_indicator.js` passes with zero failures (`gjs -m extension/tests/test_indicator.js`). [Regression]
- [x] `CHK-REG-006` Existing popover test suite `extension/tests/test_popover.js` passes with zero failures (`gjs -m extension/tests/test_popover.js`). [Regression]
- [x] `CHK-REG-007` Rust workspace test suite passes all 68 tests across all crates with zero regressions (`cargo test --workspace`). [Regression]

---

## 8. Lifecycle & Resource Cleanup

*Validates leak-free disable/enable cycles and timer management.*

- [x] `CHK-CLN-001` When extension is disabled (`disable()`), `SettingsManager.destroy()` is called and all GSettings signal listener IDs are disconnected. [FR-009, Scenario 1.4]
- [x] `CHK-CLN-002` When extension is disabled, `WatchAIIndicator.destroy()` and `WatchAISessionPopover.destroy()` cleanly tear down UI actors and GLib duration timers. [Lifecycle]
- [x] `CHK-CLN-003` Extension enable/disable cycle verified to produce zero memory leaks or orphaned signal handlers across multiple toggle cycles. [Scenario 1.4]

---

## 9. Documentation Synchronization

*Validates documentation accuracy and task list integrity.*

- [x] `CHK-DOC-001` `docs/architecture.md` updated with Phase 10 desktop integration layer (GSettings schema, notification throttling matrix, AT-SPI accessibility mappings). [Documentation]
- [x] `CHK-DOC-002` Canonical tasks `T062`, `T063`, `T064`, `T065` in `specs/001-agent-status-indicator/tasks.md` synchronized to completed `[x]` ONLY after all verification gates pass. [Task Synchronization]

---

## Verification Dependencies & Evidence Map

```
GSettings Schema (T062) ──► SettingsManager (T063) ──► Icon Style Wiring ──┐
                                     │                                    │
Project Name Sanitizer  ──► NotificationManager (T064) ───────────────────┼──► All 6 GJS Test Suites Pass
                                     │                                    │    + Schema Compile Passes
AT-SPI Widget Polish (T065) ─────────┴────────────────────────────────────┘    + Cargo Workspace Passes (68/68)
```
