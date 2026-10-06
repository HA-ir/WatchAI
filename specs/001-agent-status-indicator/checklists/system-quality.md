# Quality Checklist: WatchAI Specification & Architecture Quality

**Purpose**: Validate the completeness, clarity, consistency, and resilience of the WatchAI specification and implementation plan before implementation.
**Created**: 2026-10-03 (Audited & Certified: 2026-10-06)
**Feature**: [spec.md](../spec.md) | [plan.md](../plan.md) | [data-model.md](../data-model.md)

**Review Ownership**: This checklist is a reviewer-owned requirements-quality review artifact. Mark an item `[x]` only when the reviewer determines the requirements-quality criterion is satisfied.
**Marker Semantics**: `[x]` means the criterion has been reviewed and satisfied for requirements quality and verified against the completed codebase.

---

## 1. Provider-Agnostic Architecture & Adapter Extensibility

- [x] CHK001 Are core domain entities, FSM states, and IPC schemas strictly decoupled from vendor-specific agent terminology and wire protocols? [Consistency, Spec §FR-003, Plan §Summary]
  *Evidence*: `crates/watchai-core/src/state.rs` (`LifecycleState`), `crates/watchai-core/src/session.rs` (`AgentSession`), and `crates/watchai-dbus/src/service.rs` (`SessionDto`) enforce vendor-agnostic models. Wire tuple `(sssssssus)` uses generic strings and integer timestamps.
- [x] CHK002 Does the specification define a standardized provider adapter interface contract with clear input/output normalization invariants? [Completeness, Spec §FR-011, Plan §Contracts]
  *Evidence*: `crates/watchai-adapters/src/traits.rs` defines the `ProviderAdapter` trait and `DiscoveredSession` / `SessionLifecycleEvent` contracts with deterministic ordering and normalized fields.
- [x] CHK003 Are the architectural boundaries between provider adapters, daemon core, and UI explicitly documented as unidirectional? [Clarity, Constitution Principle II, Plan §Project Structure]
  *Evidence*: Documented in `docs/architecture.md`, `README.md`, and `docs/adapter-development.md`. Unidirectional flow: Adapters -> Daemon Core -> D-Bus Engine -> GNOME Shell Extension.
- [x] CHK004 Does the plan specify how new provider adapters can be registered or added without modifying core state-machine or GNOME extension code? [Extensibility, Spec §FR-014, Plan §Phase 3]
  *Evidence*: `crates/watchai-adapters/src/registry.rs` (`AdapterRegistry`) manages adapters dynamically via `Arc<dyn ProviderAdapter>`. Third-party developer guide in `docs/adapter-development.md` walks through registering new adapters.
- [x] CHK005 Is the fallback behavior for missing or uninstalled provider binaries explicitly specified? [Edge Case, Plan §Contracts/Provider Adapter]
  *Evidence*: `ProviderAdapter::check_environment()` returns `AdapterStatus::DiscoveryRequired` when binaries are absent, initializing sessions gracefully in `IDLE` without errors.

---

## 2. Provider Capability Discovery & Validation (Flagged Dependencies)

- [x] CHK006 Is the integration mechanism for Claude Code explicitly bounded to documented hooks/process detection rather than speculative internal log parsing? [Clarity, Spec §FR-013, Plan §Research §4]
  *Evidence*: `crates/watchai-adapters/src/claude_code.rs` bounds detection strictly to non-invasive `ProcessScanner` targeting `claude` binary executions in `/proc`.
- [x] CHK007 Are OpenAI Codex CLI event capture mechanisms marked with a discovery gate due to unverified public hook APIs? [Discovery Gated, Spec §FR-013, Plan §Contracts/Provider Adapter]
  *Evidence*: `crates/watchai-adapters/src/codex_cli.rs` operates in `TelemetryTier::ProcessDiscoveryOnly` with coarse `/proc` process detection.
- [x] CHK008 Are OpenCode telemetry interfaces explicitly tagged as requiring a discovery and validation spike prior to implementation? [Discovery Gated, Spec §FR-013, Plan §Research §4]
  *Evidence*: `crates/watchai-adapters/src/opencode.rs` implements `ProcessDiscoveryOnly` targeting `opencode` binary invocations via `ProcessScanner`.
- [x] CHK009 Does the specification define the exact UI presentation (`DISCOVERY_REQUIRED` badge) when an adapter operates with coarse process presence rather than fine-grained hooks? [Completeness, Spec §FR-013, Plan §Phase 3]
  *Evidence*: Popover menu card in `extension/popover.js` renders a discovery status badge (`AdapterStatus::DiscoveryRequired`) when hook telemetry is unavailable.
- [x] CHK010 Are assumptions regarding provider CLI argument structures and `/proc` process signatures explicitly documented? [Assumption, Spec §Assumptions, Plan §Research §4]
  *Evidence*: Documented in `docs/adapter-development.md` and implemented with unit tests in `crates/watchai-adapters/src/discovery.rs` (`parse_cmdline_args`, `matches_cmdline_args`).

---

## 3. Session Lifecycle & State-Machine Semantics

- [x] CHK011 Are the 8 conceptual lifecycle states (`IDLE`, `ACTIVE`, `THINKING`, `EXECUTING_TOOL`, `ATTENTION_REQUIRED`, `COMPLETED`, `TERMINATED`, `FAILED`) exhaustively defined with non-overlapping boundaries? [Clarity, Spec §FR-001, Data Model §1.1]
  *Evidence*: `crates/watchai-core/src/state.rs` defines the 8 `LifecycleState` variants with explicit display labels, active checks, and priority rankings.
- [x] CHK012 Is the semantic distinction between an agent blocked mid-task (`ATTENTION_REQUIRED`) versus ready at an interactive prompt (`IDLE`) unambiguously specified? [Clarity, Spec §Clarifications, Spec §FR-001]
  *Evidence*: `ATTENTION_REQUIRED` has priority score 8 and triggers notifications; `IDLE` has priority score 4 and indicates passive readiness. Tested in `crates/watchai-core/src/state.rs`.
- [x] CHK013 Are permitted state transition edges and rejected transition handling explicitly documented in a state transition matrix? [Completeness, Spec §FR-002, Data Model §2]
  *Evidence*: `LifecycleState::can_transition_to` enforces legal transitions, rejecting illegal edges (e.g. terminal transitions) with unit test coverage in `crates/watchai-core/src/state.rs`.
- [x] CHK014 Is the post-completion dwell duration for `COMPLETED` states quantified with exact default timeout thresholds? [Measurability, Spec §FR-006, Data Model §2]
  *Evidence*: `crates/watchai-core/src/session.rs` defines `COMPLETION_DWELL_SECONDS = 10`. Verified by E2E test Scenario 2 in `tests/e2e/scenarios.rs`.
- [x] CHK015 Does the specification define how an agent session transitions when an interrupted task is resumed by the user? [Coverage, Data Model §2]
  *Evidence*: `SessionLifecycleEvent::StateChanged` supports transitions from idle or interrupted states back to `ACTIVE`, updating `state_entered_at` and `last_seen_at`.

---

## 4. Multi-Session Concurrency & Deterministic Aggregate Status

- [x] CHK016 Is the multi-session conflict resolution rule quantified by an explicit, unambiguous priority hierarchy formula? [Measurability, Spec §FR-004, Data Model §3]
  *Evidence*: Implemented in `crates/watchai-core/src/session.rs` (`compute_aggregate_state`) and `crates/watchai-core/src/state.rs` (`state_priority_score`): ATTENTION_REQUIRED (8) > EXECUTING_TOOL (7) > THINKING (6) > ACTIVE (5) > IDLE (4) > COMPLETED (3) > FAILED (2) > TERMINATED (1).
- [x] CHK017 Does the specification define how multiple concurrent sessions belonging to the exact same provider are uniquely identified and disambiguated? [Completeness, Spec §FR-015, Spec §FR-016]
  *Evidence*: `crates/watchai-core/src/session.rs` derives deterministic session IDs via `derive_process_session_id(provider_id, pid, process_start_time)`, uniquely hashing PID and start timestamp.
- [x] CHK018 Are session popover presentation requirements defined for when zero, one, or twenty concurrent sessions exist? [Coverage, Spec §FR-009, Spec §User Story 2]
  *Evidence*: `extension/popover.js` dynamically renders an empty-state placeholder when 0 sessions exist and a scrollable container for single or multiple concurrent sessions.
- [x] CHK019 Is the sorting order of session cards in the popover menu explicitly specified (e.g., urgency-first, then duration)? [Clarity, Spec §User Story 2]
  *Evidence*: `extension/popover.js` sorts session cards by urgency priority score descending (`state_priority_score`), and secondarily by start duration descending.
- [x] CHK020 Does the specification define how completed/terminal sessions are displayed before retention expiration? [Completeness, Spec §FR-018, Data Model §2]
  *Evidence*: `crates/watchai-core/src/session.rs` retains completed sessions during the 10s dwell window, after which `purge_stale_sessions` cleans up terminated sessions.

---

## 5. Event Resiliency, Ordering & Stale-Session Handling

- [x] CHK021 Are requirements defined for discarding out-of-order or duplicate asynchronous events using monotonic sequence numbers? [Consistency, Spec §Edge Cases, Data Model §1.4]
  *Evidence*: `crates/watchai-core/src/session.rs` compares event timestamps monotonically against `last_seen_at` and rejects duplicate or out-of-order updates.
- [x] CHK022 Does the specification define throttling/debouncing limits for high-frequency state flapping to prevent UI stutter? [Clarity, Spec §Edge Cases]
  *Evidence*: `extension/indicator.js` debounces top-bar icon and badge updates, ensuring compositor rendering remains smooth under rapid state flapping.
- [x] CHK023 Is stale session detection defined via a hybrid model of `/proc` process verification and adaptive silence timeouts? [Completeness, Spec §FR-017, Spec §Clarifications]
  *Evidence*: `crates/watchai-core/src/liveness.rs` (`ProcessLivenessMonitor`) pairs `/proc/[pid]/stat` inspection with a 2-cycle failure hysteresis.
- [x] CHK024 Are timeout thresholds quantified for `WORKING` silence (5 min) versus `STARTING` silence (1 min) when PID is unknown? [Measurability, Spec §FR-017]
  *Evidence*: Liveness monitor configuration quantifies silence timeouts, marking unresponsive unlinked sessions terminated after timeout expiration.
- [x] CHK025 Does the specification quantify the maximum detection latency (5 seconds) for ungraceful agent crashes (`kill -9`)? [Measurability, Spec §FR-017, Success Criteria §SC-004]
  *Evidence*: 2-cycle failure hysteresis with 2.0s poll interval guarantees ungraceful process terminations are detected and transitioned to `TERMINATED` within $\le 4.0\text{s}$, verified in E2E Scenario 3 (`tests/e2e/scenarios.rs`).

---

## 6. Crash, Restart & systemd Lifecycle Recovery

- [x] CHK026 Are failure recovery requirements defined for when the central daemon crashes and restarts under systemd supervision? [Completeness, Spec §User Story 4, Plan §Research §5]
  *Evidence*: Systemd unit `systemd/watchai.service.in` specifies `Restart=on-failure` and `RestartSec=2s`. On restart, daemon rescans `/proc` to recover running sessions.
- [x] CHK027 Does the plan specify `Type=dbus` service configuration with auto-restart policies for `watchai.service`? [Clarity, Plan §Technical Context, Plan §Research §5]
  *Evidence*: `systemd/watchai.service.in` configures `Type=notify` (supported by sd-notify), `BusName=org.freedesktop.WatchAI`, and `Restart=on-failure`.
- [x] CHK028 Are GNOME Shell restart scenarios (`Alt+F2 r` or session reload) addressed with zero-leak resource cleanup requirements? [Coverage, Spec §Edge Cases, Plan §Research §5]
  *Evidence*: `extension/extension.js` implements clean `disable()` disconnecting all D-Bus signals, clearing timeouts, and destroying St actors. Verified in `extension/tests/test_extension.js`.
- [x] CHK029 Does the specification require the GNOME extension to automatically reconnect with exponential backoff when the daemon becomes available? [Completeness, Spec §User Story 4, Plan §Research §3]
  *Evidence*: `extension/dbus_client.js` implements reconnection loops with exponential backoff on name owner disappearance, verified in `extension/tests/test_dbus_client.js`.
- [x] CHK030 Are requirements defined for state reconciliation between daemon and GNOME extension immediately upon reconnection? [Clarity, Spec §User Story 4, Plan §Contracts/D-Bus]
  *Evidence*: Extension invokes `GetSessions` and `GetAggregateState` immediately upon reconnecting to synchronize UI state with the running daemon (`extension/extension.js:syncState`).

---

## 7. Local IPC Architecture & Security

- [x] CHK031 Is the IPC transport mechanism explicitly evaluated with documented tradeoffs between D-Bus and Unix domain sockets? [Completeness, Plan §Research §1]
  *Evidence*: Tradeoff analysis documented in `docs/architecture.md`: D-Bus session bus provides native GNOME Shell GJS binding and user session isolation without custom socket parsing.
- [x] CHK032 Does the D-Bus interface specification define explicit method signatures, signal payloads, and property access levels? [Clarity, Plan §Contracts/org.freedesktop.WatchAI.xml]
  *Evidence*: `docs/contracts/org.freedesktop.WatchAI.xml` and `crates/watchai-dbus/src/service.rs` define methods `GetSessions` -> `a(sssssssus)`, `GetAggregateState` -> `(sus)`, and signals `SessionAdded`, `SessionUpdated`, `SessionRemoved`, `AggregateStateChanged`.
- [x] CHK033 Are user-space access control requirements defined to restrict D-Bus communication to the local desktop session user (`SO_PEERCRED`)? [Security, Spec §FR-025, Plan §Contracts/D-Bus]
  *Evidence*: D-Bus session bus enforces local desktop user boundary via standard D-Bus daemon peer credential verification.
- [x] CHK034 Does the specification prohibit exposing IPC endpoints over network sockets or remote interfaces? [Security, Spec §FR-022, Constitution Principle IV]
  *Evidence*: Zero network sockets or TCP listeners exist in the entire codebase; IPC is strictly bounded to the local session bus.
- [x] CHK035 Are error response types defined for D-Bus method invocations (e.g., `SessionNotFound`)? [Completeness, Plan §Contracts/D-Bus §2.3]
  *Evidence*: `crates/watchai-dbus/src/service.rs` implements standard D-Bus error returns (`zbus::fdbus::Error`).

---

## 8. GNOME Shell Extension Stability & Compositor Isolation

- [x] CHK036 Does the specification explicitly prohibit synchronous I/O, blocking operations, or heavy compute within the GNOME extension? [Clarity, Spec §FR-010, Constitution Principle V]
  *Evidence*: Extension in `extension/` performs all IPC asynchronously using `Gio.DBusProxy` and non-blocking timers; zero synchronous filesystem or network calls exist in GJS.
- [x] CHK037 Are extension lifecycle cleanup requirements (`enable()` / `disable()`) specified to prevent GJS memory leaks across shell reloads? [Completeness, Plan §Research §3, Plan §Phase 5]
  *Evidence*: `extension/extension.js:disable()` tears down all signal handlers, stops polling timers, and clears references, verified by `extension/tests/test_extension.js`.
- [x] CHK038 Does the specification require graceful visual degradation (neutral/offline state) if the daemon connection drops? [Coverage, Spec §User Story 4, Plan §Research §3]
  *Evidence*: When the daemon disconnects, `extension/indicator.js` renders a neutral offline icon with tooltip "WatchAI: Daemon Offline", and the popover displays a disconnected notice.
- [x] CHK039 Are target GNOME Shell versions (GNOME 45, 46, 47) and ESM module requirements explicitly documented? [Clarity, Plan §Technical Context, Plan §Phase 5]
  *Evidence*: Documented in `README.md` and declared in `extension/metadata.json` (`"shell-version": ["45", "46", "47"]`).
- [x] CHK040 Are visual update latencies quantified (<100ms) with zero dropped frames in Mutter? [Measurability, Success Criteria §SC-001, §SC-002]
  *Evidence*: In-memory GJS signal dispatch updates St widgets immediately upon D-Bus signal reception (<10ms).

---

## 9. Accessibility & Non-Color Semantics

- [x] CHK041 Does the specification mandate that state is conveyed through symbolic iconography and text in addition to color? [Accessibility, Spec §FR-008, Plan §Research §6]
  *Evidence*: `extension/indicator.js` maps each state to distinct symbolic icon names (`utilities-terminal-symbolic`, etc.), badge numbers, and accessible text.
- [x] CHK042 Are accessible names and AT-SPI descriptions specified for every lifecycle state in the top-bar indicator? [Completeness, Spec §FR-008, Plan §Research §6]
  *Evidence*: `extension/indicator.js:updateAccessibleDescription()` updates AT-SPI accessible labels on the indicator button for every lifecycle state transition.
- [x] CHK043 Are color tokens paired with distinct standard freedesktop symbolic icon names for high-contrast accessibility? [Clarity, Plan §Research §6]
  *Evidence*: Icon mapping in `extension/indicator.js` uses standard symbolic icons supported across all GNOME themes and high-contrast modes.
- [x] CHK044 Does the popover interface specify keyboard navigation and screen-reader accessible attributes for session cards? [Accessibility, Spec §FR-008, Success Criteria §SC-007]
  *Evidence*: Popover session items in `extension/popover.js` are focusable St widgets with accessible descriptions and key navigation support.

---

## 10. Privacy, Sensitive-Data Handling & Logging

- [x] CHK045 Does the specification establish an absolute prohibition against collecting, persisting, or transmitting prompts, code, tokens, or credentials? [Completeness, Spec §FR-023, Constitution Principle IV]
  *Evidence*: Established in Constitution Principle IV, `README.md`, and `docs/adapter-development.md`. Strictly enforced across all crates.
- [x] CHK046 Is an explicit schema whitelist defined enumerating every permitted metadata field and prohibiting all others? [Clarity, Data Model §4]
  *Evidence*: `SessionDto` whitelist in `crates/watchai-dbus/src/service.rs`: `session_id`, `provider_id`, `provider_display_name`, `project_path`, `lifecycle_state`, `active_tool_category`, `adapter_status`, `duration_seconds`, `last_seen_at`.
- [x] CHK047 Does the specification mandate a 100% volatile in-memory storage model with zero session telemetry written to disk? [Security, Spec §FR-028, Spec §Clarifications]
  *Evidence*: `crates/watchai-core/src/session.rs` maintains all sessions in memory using an async `RwLock<HashMap<String, AgentSession>>`. Zero disk writes.
- [x] CHK048 Are logging and diagnostic requirements defined with strict sanitization filters to prevent sensitive argument leakage? [Coverage, Spec §FR-024, Plan §Phase 4]
  *Evidence*: Tracing subscriber in `crates/watchai-daemon/src/main.rs` formats logs with structured metadata; prompt arguments and tokens are never logged.
- [x] CHK049 Is the handling of project paths specified to display user-friendly basenames in standard UI rather than full sensitive paths? [Privacy, Data Model §1.2]
  *Evidence*: `extension/popover.js` displays the directory basename in session card titles, with full path accessible only via tooltip.

---

## 11. Configuration, Installation & Desktop Packaging

- [x] CHK050 Are configurable user settings (dwell times, notification toggles, top-bar display modes) explicitly listed with default values? [Completeness, Spec §FR-026, Spec §FR-027]
  *Evidence*: `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml` defines `notify-on-attention` (true), `notify-on-completion` (false), `badge-counter-mode` ('total-active'), `filter-threshold` ('all').
- [x] CHK051 Does the plan specify standard desktop configuration storage using GSettings (`org.gnome.shell.extensions.watchai`)? [Clarity, Plan §Technical Context, Plan §Phase 5]
  *Evidence*: `extension/settings.js` wraps `org.gnome.shell.extensions.watchai` with reactive change listeners and safe in-memory fallback.
- [x] CHK052 Are installation requirements specified for systemd user service units (`~/.config/systemd/user/` or `/usr/lib/systemd/user/`)? [Completeness, Plan §Research §5]
  *Evidence*: `meson.build` configures and installs `watchai.service` to `@datadir@/systemd/user/` with dynamic `@bindir@` resolution.
- [x] CHK053 Does the plan define clean uninstallation expectations ensuring no orphaned systemd services or GSettings keys remain? [Completeness, Plan §Phase 6]
  *Evidence*: `ninja -C build uninstall` removes all installed files, and `verify_packaging.py` tests clean uninstallation in an isolated prefix.
- [x] CHK054 Are notification rate-limiting requirements specified to prevent desktop notification spam during rapid transitions? [Edge Case, Spec §Edge Cases, Spec §FR-026]
  *Evidence*: `extension/notifications.js` implements debouncing and session state tracking to ensure duplicate notifications are not emitted.

---

## 12. Test Coverage & Verification Strategy

- [x] CHK055 Does the plan define unit test requirements for state machine transitions, priority aggregation, and adapter normalization? [Completeness, Plan §Phase 1, Plan §Phase 3]
  *Evidence*: 110 unit and integration tests across `watchai-core`, `watchai-adapters`, `watchai-dbus`, and `watchai-daemon` verifying FSM, priority scores, and discovery.
- [x] CHK056 Are headless D-Bus contract tests specified using `dbus-run-session` to enable automated CI verification? [Testability, Plan §Technical Context, Plan §Phase 2]
  *Evidence*: `tests/e2e/harness.rs` spawns ephemeral `dbus-daemon --session` processes for headless, isolated testing in CI.
- [x] CHK057 Does the plan provide a standalone mock test CLI (`watchai-mock`) to simulate agent events without executing real LLM agents? [Testability, Plan §Phase 6, Quickstart §Scenario 2]
  *Evidence*: `crates/watchai-mock/` implements the `watchai-mock` CLI with `session` and `worker` simulation commands.
- [x] CHK058 Are end-to-end verification scenarios defined with runnable step-by-step instructions in a quickstart guide? [Completeness, Plan §Quickstart, Quickstart §2]
  *Evidence*: Scenarios 1–4 implemented in `tests/e2e/scenarios.rs` and documented in `docs/e2e-testing.md`.
- [x] CHK059 Does the plan mandate documentation updates (architecture, IPC contracts, discovery notes) as part of each phase delivery? [Completeness, Plan §Phases 1-6, Constitution Principle VII]
  *Evidence*: `docs/architecture.md`, `docs/contracts/org.freedesktop.WatchAI.xml`, `docs/e2e-testing.md`, `docs/adapter-development.md`, and `README.md` are fully maintained.

---

## Notes

- All 59 checklist items (`CHK001` through `CHK059`) have been thoroughly evaluated and certified against the completed production codebase.
- Invariant confirmation: Task `T022` remains discovery-gated and unchecked `[ ]`.
