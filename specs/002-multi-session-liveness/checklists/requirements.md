# Specification Quality Checklist: Multi-Session Aggregation & Liveness

**Purpose**: Validate specification completeness and quality before proceeding to planning  
**Created**: 2026-10-03  
**Feature**: [spec.md](../spec.md)  

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- All 16 requirements quality criteria passed (16/16).
- Zero `[NEEDS CLARIFICATION]` markers remain.
- Clarification session on 2026-10-03 resolved 5 critical semantic and lifecycle ambiguities:
  1. Decoupled aggregate dwell (10s top-bar reset) from session retention (60s popover lifetime).
  2. Liveness vs activity boundary: uninstrumented `/proc` processes are `IDLE` (`DISCOVERY_REQUIRED`); `WORKING`/`WAITING` require telemetry.
  3. `process_start_time` is internal `u64` (jiffies), unexposed over D-Bus; crash confirmed after 2 consecutive failed reads.
  4. `active_session_count` represents non-terminal sessions plus terminal sessions currently inside their 10s dwell window.
  5. Daemon restart recovery via immediate `/proc` discovery sweep using deterministic surrogate keys without false error transitions.
- Task T022 (Claude Code opt-in hook receiver) remains explicitly pending and discovery-gated.
- Zero lines of Rust/GJS source code modified.
