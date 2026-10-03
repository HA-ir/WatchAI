# Quality Checklist: WatchAI Specification & Architecture Quality

**Purpose**: Validate the completeness, clarity, consistency, and resilience of the WatchAI specification and implementation plan before implementation.  
**Created**: 2026-10-03  
**Feature**: [spec.md](../spec.md) | [plan.md](../plan.md) | [data-model.md](../data-model.md)  

**Review Ownership**: This checklist is a reviewer-owned requirements-quality review artifact. Mark an item `[x]` only when the reviewer determines the requirements-quality criterion is satisfied.  
**Marker Semantics**: `[x]` means the criterion has been reviewed and satisfied for requirements quality. It does not mean implementation work is complete.  

---

## 1. Provider-Agnostic Architecture & Adapter Extensibility

- [ ] CHK001 Are core domain entities, FSM states, and IPC schemas strictly decoupled from vendor-specific agent terminology and wire protocols? [Consistency, Spec §FR-003, Plan §Summary]
- [ ] CHK002 Does the specification define a standardized provider adapter interface contract with clear input/output normalization invariants? [Completeness, Spec §FR-011, Plan §Contracts]
- [ ] CHK003 Are the architectural boundaries between provider adapters, daemon core, and UI explicitly documented as unidirectional? [Clarity, Constitution Principle II, Plan §Project Structure]
- [ ] CHK004 Does the plan specify how new provider adapters can be registered or added without modifying core state-machine or GNOME extension code? [Extensibility, Spec §FR-014, Plan §Phase 3]
- [ ] CHK005 Is the fallback behavior for missing or uninstalled provider binaries explicitly specified? [Edge Case, Plan §Contracts/Provider Adapter]

---

## 2. Provider Capability Discovery & Validation (Flagged Dependencies)

- [ ] CHK006 Is the integration mechanism for Claude Code explicitly bounded to documented hooks/process detection rather than speculative internal log parsing? [Clarity, Spec §FR-013, Plan §Research §4]
- [ ] CHK007 Are OpenAI Codex CLI event capture mechanisms marked with a discovery gate due to unverified public hook APIs? [Discovery Gated, Spec §FR-013, Plan §Contracts/Provider Adapter]
- [ ] CHK008 Are OpenCode telemetry interfaces explicitly tagged as requiring a discovery and validation spike prior to implementation? [Discovery Gated, Spec §FR-013, Plan §Research §4]
- [ ] CHK009 Does the specification define the exact UI presentation (`DISCOVERY_REQUIRED` badge) when an adapter operates with coarse process presence rather than fine-grained hooks? [Completeness, Spec §FR-013, Plan §Phase 3]
- [ ] CHK010 Are assumptions regarding provider CLI argument structures and `/proc` process signatures explicitly documented? [Assumption, Spec §Assumptions, Plan §Research §4]

---

## 3. Session Lifecycle & State-Machine Semantics

- [ ] CHK011 Are the 8 conceptual lifecycle states (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`) exhaustively defined with non-overlapping boundaries? [Clarity, Spec §FR-001, Data Model §1.1]
- [ ] CHK012 Is the semantic distinction between an agent blocked mid-task (`WAITING`) versus ready at an interactive prompt (`IDLE`) unambiguously specified? [Clarity, Spec §Clarifications, Spec §FR-001]
- [ ] CHK013 Are permitted state transition edges and rejected transition handling explicitly documented in a state transition matrix? [Completeness, Spec §FR-002, Data Model §2]
- [ ] CHK014 Is the post-completion dwell duration for `SUCCESS` and `CANCELLED` states quantified with exact default timeout thresholds? [Measurability, Spec §FR-006, Data Model §2]
- [ ] CHK015 Does the specification define how an agent session transitions when an interrupted task is resumed by the user? [Coverage, Data Model §2]

---

## 4. Multi-Session Concurrency & Deterministic Aggregate Status

- [ ] CHK016 Is the multi-session conflict resolution rule quantified by an explicit, unambiguous priority hierarchy formula? [Measurability, Spec §FR-004, Data Model §3]
- [ ] CHK017 Does the specification define how multiple concurrent sessions belonging to the exact same provider are uniquely identified and disambiguated? [Completeness, Spec §FR-015, Spec §FR-016]
- [ ] CHK018 Are session popover presentation requirements defined for when zero, one, or twenty concurrent sessions exist? [Coverage, Spec §FR-009, Spec §User Story 2]
- [ ] CHK019 Is the sorting order of session cards in the popover menu explicitly specified (e.g., urgency-first, then duration)? [Clarity, Spec §User Story 2]
- [ ] CHK020 Does the specification define how completed/terminal sessions are displayed before retention expiration? [Completeness, Spec §FR-018, Data Model §2]

---

## 5. Event Resiliency, Ordering & Stale-Session Handling

- [ ] CHK021 Are requirements defined for discarding out-of-order or duplicate asynchronous events using monotonic sequence numbers? [Consistency, Spec §Edge Cases, Data Model §1.4]
- [ ] CHK022 Does the specification define throttling/debouncing limits for high-frequency state flapping to prevent UI stutter? [Clarity, Spec §Edge Cases]
- [ ] CHK023 Is stale session detection defined via a hybrid model of `/proc` process verification and adaptive silence timeouts? [Completeness, Spec §FR-017, Spec §Clarifications]
- [ ] CHK024 Are timeout thresholds quantified for `WORKING` silence (5 min) versus `STARTING` silence (1 min) when PID is unknown? [Measurability, Spec §FR-017]
- [ ] CHK025 Does the specification quantify the maximum detection latency (5 seconds) for ungraceful agent crashes (`kill -9`)? [Measurability, Spec §FR-017, Success Criteria §SC-004]

---

## 6. Crash, Restart & systemd Lifecycle Recovery

- [ ] CHK026 Are failure recovery requirements defined for when the central daemon crashes and restarts under systemd supervision? [Completeness, Spec §User Story 4, Plan §Research §5]
- [ ] CHK027 Does the plan specify `Type=dbus` service configuration with auto-restart policies for `watchai.service`? [Clarity, Plan §Technical Context, Plan §Research §5]
- [ ] CHK028 Are GNOME Shell restart scenarios (`Alt+F2 r` or session reload) addressed with zero-leak resource cleanup requirements? [Coverage, Spec §Edge Cases, Plan §Research §5]
- [ ] CHK029 Does the specification require the GNOME extension to automatically reconnect with exponential backoff when the daemon becomes available? [Completeness, Spec §User Story 4, Plan §Research §3]
- [ ] CHK030 Are requirements defined for state reconciliation between daemon and GNOME extension immediately upon reconnection? [Clarity, Spec §User Story 4, Plan §Contracts/D-Bus]

---

## 7. Local IPC Architecture & Security

- [ ] CHK031 Is the IPC transport mechanism explicitly evaluated with documented tradeoffs between D-Bus and Unix domain sockets? [Completeness, Plan §Research §1]
- [ ] CHK032 Does the D-Bus interface specification define explicit method signatures, signal payloads, and property access levels? [Clarity, Plan §Contracts/org.freedesktop.WatchAI.xml]
- [ ] CHK033 Are user-space access control requirements defined to restrict D-Bus communication to the local desktop session user (`SO_PEERCRED`)? [Security, Spec §FR-025, Plan §Contracts/D-Bus]
- [ ] CHK034 Does the specification prohibit exposing IPC endpoints over network sockets or remote interfaces? [Security, Spec §FR-022, Constitution Principle IV]
- [ ] CHK035 Are error response types defined for D-Bus method invocations (e.g., `SessionNotFound`)? [Completeness, Plan §Contracts/D-Bus §2.3]

---

## 8. GNOME Shell Extension Stability & Compositor Isolation

- [ ] CHK036 Does the specification explicitly prohibit synchronous I/O, blocking operations, or heavy compute within the GNOME extension? [Clarity, Spec §FR-010, Constitution Principle V]
- [ ] CHK037 Are extension lifecycle cleanup requirements (`enable()` / `disable()`) specified to prevent GJS memory leaks across shell reloads? [Completeness, Plan §Research §3, Plan §Phase 5]
- [ ] CHK038 Does the specification require graceful visual degradation (neutral/offline state) if the daemon connection drops? [Coverage, Spec §User Story 4, Plan §Research §3]
- [ ] CHK039 Are target GNOME Shell versions (GNOME 45, 46, 47) and ESM module requirements explicitly documented? [Clarity, Plan §Technical Context, Plan §Phase 5]
- [ ] CHK040 Are visual update latencies quantified (<100ms) with zero dropped frames in Mutter? [Measurability, Success Criteria §SC-001, §SC-002]

---

## 9. Accessibility & Non-Color Semantics

- [ ] CHK041 Does the specification mandate that state is conveyed through symbolic iconography and text in addition to color? [Accessibility, Spec §FR-008, Plan §Research §6]
- [ ] CHK042 Are accessible names and AT-SPI descriptions specified for every lifecycle state in the top-bar indicator? [Completeness, Spec §FR-008, Plan §Research §6]
- [ ] CHK043 Are color tokens paired with distinct standard freedesktop symbolic icon names for high-contrast accessibility? [Clarity, Plan §Research §6]
- [ ] CHK044 Does the popover interface specify keyboard navigation and screen-reader accessible attributes for session cards? [Accessibility, Spec §FR-008, Success Criteria §SC-007]

---

## 10. Privacy, Sensitive-Data Handling & Logging

- [ ] CHK045 Does the specification establish an absolute prohibition against collecting, persisting, or transmitting prompts, code, tokens, or credentials? [Completeness, Spec §FR-023, Constitution Principle IV]
- [ ] CHK046 Is an explicit schema whitelist defined enumerating every permitted metadata field and prohibiting all others? [Clarity, Data Model §4]
- [ ] CHK047 Does the specification mandate a 100% volatile in-memory storage model with zero session telemetry written to disk? [Security, Spec §FR-028, Spec §Clarifications]
- [ ] CHK048 Are logging and diagnostic requirements defined with strict sanitization filters to prevent sensitive argument leakage? [Coverage, Spec §FR-024, Plan §Phase 4]
- [ ] CHK049 Is the handling of project paths specified to display user-friendly basenames in standard UI rather than full sensitive paths? [Privacy, Data Model §1.2]

---

## 11. Configuration, Installation & Desktop Packaging

- [ ] CHK050 Are configurable user settings (dwell times, notification toggles, top-bar display modes) explicitly listed with default values? [Completeness, Spec §FR-026, Spec §FR-027]
- [ ] CHK051 Does the plan specify standard desktop configuration storage using GSettings (`org.gnome.shell.extensions.watchai`)? [Clarity, Plan §Technical Context, Plan §Phase 5]
- [ ] CHK052 Are installation requirements specified for systemd user service units (`~/.config/systemd/user/` or `/usr/lib/systemd/user/`)? [Completeness, Plan §Research §5]
- [ ] CHK053 Does the plan define clean uninstallation expectations ensuring no orphaned systemd services or GSettings keys remain? [Completeness, Plan §Phase 6]
- [ ] CHK054 Are notification rate-limiting requirements specified to prevent desktop notification spam during rapid transitions? [Edge Case, Spec §Edge Cases, Spec §FR-026]

---

## 12. Test Coverage & Verification Strategy

- [ ] CHK055 Does the plan define unit test requirements for state machine transitions, priority aggregation, and adapter normalization? [Completeness, Plan §Phase 1, Plan §Phase 3]
- [ ] CHK056 Are headless D-Bus contract tests specified using `dbus-run-session` to enable automated CI verification? [Testability, Plan §Technical Context, Plan §Phase 2]
- [ ] CHK057 Does the plan provide a standalone mock test CLI (`watchai-mock`) to simulate agent events without executing real LLM agents? [Testability, Plan §Phase 6, Quickstart §Scenario 2]
- [ ] CHK058 Are end-to-end verification scenarios defined with runnable step-by-step instructions in a quickstart guide? [Completeness, Plan §Quickstart, Quickstart §2]
- [ ] CHK059 Does the plan mandate documentation updates (architecture, IPC contracts, discovery notes) as part of each phase delivery? [Completeness, Plan §Phases 1-6, Constitution Principle VII]

---

## Notes

- Mark items `[x]` only after reviewer evaluation confirms the requirements-quality criterion is satisfied in `spec.md`, `plan.md`, `data-model.md`, or `contracts/`.
- Leave items unchecked (`[ ]`) when requirements require clarification, expansion, or reviewer sign-off.
- Items tagged `[Discovery Gated]` denote provider features (e.g. OpenAI Codex CLI, OpenCode) whose fine-grained event hooks cannot be fully verified until prototype discovery spikes are conducted.
- `/speckit-implement` reads checklist state as a quality gate and must not modify checkbox markers.
