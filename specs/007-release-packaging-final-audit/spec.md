# Feature Specification: Phase 12 — Release Packaging, Installation Documentation & Final System-Quality Audit

**Feature Directory**: `specs/007-release-packaging-final-audit`
**Feature Branch**: `012-release-packaging-final-audit`
**Created**: 2026-10-06
**Status**: Draft (Specification Phase - Remediated)
**Input**: Phase 12 — Release Packaging, Installation Documentation & Final System-Quality Audit (Canonical Tasks T071–T074)

---

## 1. Executive Summary & Context

WatchAI has successfully implemented and verified all core functional layers across Phases 1 through 11:
- Multi-provider process discovery and capability modeling (Claude Code, OpenAI Codex CLI, OpenCode).
- Deterministic finite-state machine (FSM) lifecycle tracking with terminal state immunity.
- Real-time multi-session priority aggregation ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$) and contextual recency tie-breaking.
- Process liveness verification and ungraceful crash recovery via `/proc` inspection and 2-check failure hysteresis within 5 seconds.
- User-space D-Bus service (`org.freedesktop.WatchAI`, object `/org/freedesktop/WatchAI`) with immutable wire contract `(sssssssus)`.
- Systemd user service integration (`Type=notify`, `BusName=org.freedesktop.WatchAI`).
- Modern GNOME Shell 45+ ESM top-bar indicator, interactive popover session cards, GSettings user preferences, transition-gated desktop notifications (with 5.0s per-session cooldown), and AT-SPI screen-reader accessibility.
- End-to-end headless integration verification suite and mock simulation harness (`watchai-mock`).

**Phase 12** delivers the final release packaging, end-user documentation, adapter extensibility guide, and system-wide quality certification:
1. **Meson Build, Install & Uninstall Workflow (`T071`)**: Production-grade Meson build definition orchestrating Cargo compilation of `watchai-daemon` with isolated build directories, dual-location GSettings schema installation (extension-local plus system datadir) ensuring out-of-the-box user-local preference resolution, GNOME Shell extension installation, metadata alignment, systemd user service template configuration, secure file permissions, and complete uninstall/reinstall targets (`ninja uninstall`) supporting user-local unprivileged installation (`--prefix=$HOME/.local`) without requiring `root` or `sudo`.
2. **Comprehensive User Installation & Operation Guide (`T072`)**: Complete `README.md` covering prerequisites, dependencies, user-local build and install commands, systemd service management, extension enabling, GSettings configuration, shell PATH guidance, supported agent providers, privacy guarantees, D-Bus verification, and clean uninstallation.
3. **Provider Adapter Integration Guide (`T073`)**: Authoritative developer documentation in `docs/adapter-development.md` establishing the exact contracts, discovery heuristics, process matching rules, lifecycle event streaming, sanitized tool category mapping, zero-leakage privacy invariants, and testing standards required to introduce new AI agent providers.
4. **Final System-Quality Checklist Audit (`T074`)**: Systematic audit and formal evaluation of all 59 quality items in `specs/001-agent-status-indicator/checklists/system-quality.md` (`CHK001`–`CHK059`), certifying that WatchAI complies with the project Constitution and production readiness criteria.

---

## 2. User Scenarios & Testing *(mandatory)*

### User Story 1 - Production Meson Build, User-Local Install, and Clean Uninstall Workflow (Priority: P1) 🎯 MVP

As a Linux desktop user or system packager, I want to build and install WatchAI using standard Meson and Ninja build commands into my user environment (`~/.local`) without requiring root/sudo privileges, have my GSettings preferences work immediately without editing shell environment variables, and cleanly uninstall it when desired, so that I can easily deploy and manage WatchAI on any modern Linux distribution.

**Why this priority**: Packaging and installation is the fundamental entry point for users and packagers. Without a working build and install system, the software cannot be deployed or distributed to end users.

**Independent Test**: Can be independently verified on a clean machine by executing:
```bash
meson setup build --prefix=$HOME/.local
ninja -C build
ninja -C build install
```
verifying that `watchai-daemon` is installed in `$HOME/.local/bin`, the extension in `$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/`, GSettings schema compiled both in `$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/schemas/` and `$HOME/.local/share/glib-2.0/schemas/`, systemd unit in `$HOME/.local/share/systemd/user/` referencing the absolute daemon path, that `watchai-mock` is strictly excluded, that installed assets are not group/world writable, and that `ninja -C build uninstall` removes all installed assets cleanly without orphaned files.

**Acceptance Scenarios**:
1. **Given** a clean checkout of WatchAI with Meson, Ninja, and Cargo installed, **When** `meson setup build --prefix=$HOME/.local` and `ninja -C build` are executed, **Then** Meson invokes Cargo to build the production daemon binary in release mode inside an isolated target directory in the Meson build tree without polluting `<repository>/target/`, and materializes the executable at the exact Meson target output path.
2. **Given** a successful build in `build/`, **When** `ninja -C build install` is executed, **Then** the production binary `watchai-daemon` is installed to `$prefix/bin/watchai-daemon` with executable permissions, the extension directory is installed to `$datadir/gnome-shell/extensions/watchai@gnome.org/`, the GSettings schema is installed and compiled both within the extension's `schemas/` directory and `$datadir/glib-2.0/schemas/`, and the systemd user service unit is installed with `@bindir@` resolved to the absolute installed path `$prefix/bin`.
3. **Given** a successful user-local installation under `$HOME/.local`, **When** GNOME Shell loads the extension, **Then** `extension.getSettings()` successfully resolves schema `org.gnome.shell.extensions.watchai` from the extension's local `schemas/` directory without requiring the user to manually modify `$XDG_DATA_DIRS`.
4. **Given** a successful build and install, **When** examining the installed files, **Then** the development/test binary `watchai-mock`, test sockets, and test harnesses are strictly EXCLUDED from production installation paths.
5. **Given** installed production files, **When** examining file permissions, **Then** executable binaries are executable (`0755` or equivalent) and non-executable assets (JS, CSS, metadata, XML schemas, compiled schema databases, systemd units) are not group-writable or world-writable (`0644` or equivalent).
6. **Given** WatchAI is installed via Meson, **When** `ninja -C build uninstall` is executed, **Then** all files installed by Meson are removed, compiled schema files are removed from the extension schemas directory and updated in glib schemas, and no orphaned WatchAI files remain in the target prefix.
7. **Given** an uninstalled state, **When** `ninja -C build install` is executed again, **Then** reinstallation is clean, idempotent, and leaves zero duplicate or corrupted assets.
8. **Given** a system with missing build dependencies (e.g. missing `cargo` or `glib-compile-schemas`), **When** `meson setup build` is executed, **Then** Meson fails early with clear, actionable diagnostic messages indicating the missing requirement.

---

### User Story 2 - Comprehensive End-User Documentation, Configuration & Troubleshooting Guide (Priority: P2)

As an AI engineer or Linux desktop user, I want clear, accurate, and comprehensive documentation in `README.md` explaining how to build, install, enable, configure, verify, and uninstall WatchAI, so that I can get the indicator running on my desktop quickly and diagnose any environment issues without guesswork.

**Why this priority**: Documentation is the primary user interface for setup and operations. Accurate documentation prevents user frustration, reduces setup errors, and establishes transparent privacy and capability expectations.

**Independent Test**: Can be independently verified by following the instructions in `README.md` verbatim on a fresh Ubuntu 24.04 or Fedora 40 GNOME desktop, confirming every listed command succeeds as documented.

**Acceptance Scenarios**:
1. **Given** a user reading `README.md`, **When** reviewing system prerequisites, **Then** the documentation accurately specifies supported Linux environments (kernel 5.8+, GNOME Shell 45/46/47 on Wayland/X11, systemd user session, D-Bus session bus).
2. **Given** a user following installation instructions, **When** executing the documented commands, **Then** the user-local installation workflow (`--prefix=$HOME/.local`) succeeds without prompting for `sudo` or `root` credentials.
3. **Given** a user executing commands from a terminal, **When** reading the shell environment section, **Then** `README.md` clearly explains that while the systemd user service resolves `watchai-daemon` via its absolute path independently of shell `PATH`, invoking `watchai-daemon` directly from a terminal requires `$HOME/.local/bin` in the user's `$PATH`.
4. **Given** WatchAI is installed, **When** reading the configuration section, **Then** the document explains all GSettings keys (`dwell-duration-seconds`, `enable-desktop-notifications`, `notify-on-waiting`, `notify-on-error`, `indicator-icon-style`) with default values and exact `gsettings` CLI command examples (including `GSETTINGS_SCHEMA_DIR` notes for user-local CLI querying).
5. **Given** WatchAI is running, **When** reading the verification and troubleshooting section, **Then** `README.md` provides exact commands to inspect D-Bus state (`busctl --user introspect org.freedesktop.WatchAI /org/freedesktop/WatchAI`) and view systemd logs (`journalctl --user -u watchai.service -f`).
6. **Given** a user wishing to remove WatchAI, **When** following the uninstallation section, **Then** instructions cover stopping the service, disabling the extension, and running `ninja -C build uninstall`.

---

### User Story 3 - Third-Party AI Provider Adapter Integration Guide (Priority: P3)

As a contributor or external developer adding support for a new AI coding agent (e.g., Aider, Cursor, Goose, Devin CLI), I want an authoritative guide in `docs/adapter-development.md` detailing the adapter contract, process discovery rules, lifecycle event normalization, tool category mapping, and privacy boundaries, so that I can implement a robust, non-invasive adapter that integrates cleanly with the WatchAI core engine without modifying core state-machine code.

**Why this priority**: Extensibility (Constitution Principle I) requires that new AI agent providers can be added seamlessly by the community following established architectural invariants.

**Independent Test**: Can be independently verified by reviewing the guide against existing adapters (`claude-code`, `codex-cli`, `opencode`) and validating that a developer can follow its steps to implement a mock fourth adapter conforming to all trait and testing requirements.

**Acceptance Scenarios**:
1. **Given** a developer reading `docs/adapter-development.md`, **When** reviewing the architectural boundary, **Then** the guide clearly distinguishes process discovery (inspecting `/proc/[pid]/cmdline` and cwd) from telemetry observation, state inference, and session registration.
2. **Given** an adapter implementation, **When** implementing process discovery, **Then** the guide specifies mandatory false-positive rejection rules (ignoring shells, grep, git, pytest) and deterministic session ID derivation using `derive_process_session_id`.
3. **Given** an adapter normalizing agent events, **When** mapping lifecycle states, **Then** the guide documents the canonical 8-state FSM (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`) and valid state transition edges.
4. **Given** an adapter reporting tool activity, **When** assigning tool categories, **Then** the guide documents the allowed `ToolCategory` variants (`FileRead`, `FileWrite`, `ShellExecution`, `Search`, `ModelThinking`) and their SCREAMING_SNAKE_CASE IPC representations.
5. **Given** any adapter design, **When** reviewing privacy standards, **Then** the guide strictly reiterates the Zero-Leakage Privacy invariant: absolute prohibition against collecting, caching, logging, or transmitting prompts, code, diffs, tool arguments, tokens, or credentials.

---

### User Story 4 - Final Comprehensive System-Quality Checklist Audit (Priority: P4)

As the project lead and repository maintainer, I want a complete, rigorous evaluation of all 59 quality criteria in `specs/001-agent-status-indicator/checklists/system-quality.md`, so that I can certify that WatchAI meets every architectural, security, state-machine, IPC, accessibility, privacy, and reliability standard before tagging v1.0.0.

**Why this priority**: Ensures total quality governance and traceability across all 11 preceding implementation phases, proving that no shortcuts or technical debt were left unverified.

**Independent Test**: Can be independently verified by checking that all 59 items in `specs/001-agent-status-indicator/checklists/system-quality.md` (`CHK001`–`CHK059`) are audited, marked `[x]`, and supported by concrete test and code citations.

**Acceptance Scenarios**:
1. **Given** `specs/001-agent-status-indicator/checklists/system-quality.md`, **When** the audit is performed, **Then** every item across all 12 sections is verified with reference to automated tests, source code modules, and IPC contracts.
2. **Given** Constitution Principle IV (Zero-Leakage Privacy), **When** audited against `CHK045`–`CHK049`, **Then** volatile in-memory storage, prompt redaction filters, metadata-only IPC, and path sanitization are verified with automated test evidence.
3. **Given** Constitution Principle V (GNOME Extension Stability), **When** audited against `CHK036`–`CHK040`, **Then** non-blocking GJS execution, automated reconnection backoff, and clean `disable()` lifecycle unregistration are certified.
4. **Given** the completed audit, **When** canonical roadmap task `T074` is reviewed, **Then** all 59 quality checklist items are marked complete `[x]`.

---

### Edge Cases & Packaging Boundaries

- **User-Local vs System-Wide Installation**:
  - *Scenario*: User runs `meson setup build --prefix=$HOME/.local` (no sudo) vs system packager runs `meson setup build --prefix=/usr` (system packaging).
  - *Handling*: Meson configuration dynamically derives paths using Meson built-ins (`get_option('bindir')`, `get_option('datadir')`). In user-local mode, systemd user services land in `$datadir/systemd/user/` (recognized by systemd 240+) and extension in `$datadir/gnome-shell/extensions/watchai@gnome.org/`.
- **GSettings Schema Discovery on Ubuntu/Debian User-Local Prefixes**:
  - *Scenario*: Standard Ubuntu/Debian desktop sessions set `$XDG_DATA_DIRS` to `/usr/share/ubuntu:/usr/share/gnome:/usr/local/share/:/usr/share/:/var/lib/snapd/desktop`, omitting `~/.local/share`. If schemas are only placed in `~/.local/share/glib-2.0/schemas/`, GNOME Shell's `extension.getSettings()` fails to locate the schema.
  - *Handling*: Install the schema XML and compile `gschemas.compiled` directly within the extension's local `schemas/` directory (`$datadir/gnome-shell/extensions/watchai@gnome.org/schemas/`). GNOME Shell's `Extension` base class automatically discovers local extension schemas. Additionally, install the schema into `$datadir/glib-2.0/schemas/` for system-wide or `$XDG_DATA_DIRS`-configured environments.
- **Cargo Target Directory Isolation**:
  - *Scenario*: Running `cargo build` directly from Meson without an isolated target dir pollutes the top-level `<repository>/target/` directory and creates lock/artifact collisions with local developer debug builds.
  - *Handling*: Meson build targets configure Cargo with `--target-dir` set inside the Meson build tree (e.g. `<builddir>/cargo-target`), materializing the release binary at the exact Meson `custom_target` output path without touching the repository's root `target/`.
- **systemd Unit Path Resolution**:
  - *Scenario*: `systemd/watchai.service` currently hardcodes `/usr/bin/watchai-daemon`.
  - *Handling*: Introduce `systemd/watchai.service.in` where `ExecStart=@bindir@/watchai-daemon` is substituted at configure time via Meson `configure_file()`, ensuring the service points to the exact installed executable path (e.g. `/home/user/.local/bin/watchai-daemon` or `/usr/bin/watchai-daemon`). Absolute path execution guarantees the systemd user service does not depend on the user's interactive shell `$PATH`.
- **Production Purity (`watchai-mock` & Test Sockets)**:
  - *Scenario*: Cargo builds both `watchai-daemon` and `watchai-mock` in the workspace.
  - *Handling*: Meson install targets install ONLY `watchai-daemon`. `watchai-mock` is strictly a development/test binary and is never copied to `$bindir`. No test sockets (`WATCHAI_TEST_SOCKET`) are created or enabled by the installation.
- **Test-Scoped `/proc` Isolation (`WATCHAI_PROC_ROOT`)**:
  - *Scenario*: `WATCHAI_PROC_ROOT` was introduced for E2E test isolation.
  - *Handling*: The production installation and systemd unit NEVER set or reference `WATCHAI_PROC_ROOT`. In production, WatchAI defaults strictly to standard Linux `/proc`. It is documented strictly as an internal test/E2E isolation mechanism, not a user configuration setting.
- **Pre-existing or Active Service on Reinstall**:
  - *Scenario*: User installs over an existing installation while `watchai.service` is actively running.
  - *Handling*: File overwrites are atomic; documentation instructs user to run `systemctl --user daemon-reload` and `systemctl --user restart watchai.service`. Reinstallation is clean and idempotent.

---

## 3. Requirements *(mandatory)*

### Functional Requirements

#### Pillar 1: Meson Build, Install, Packaging & Verification (`T071`)
- **FR-001**: Project MUST provide a root `meson.build` file supporting Meson version 0.60.0 or later.
- **FR-002**: Meson build MUST compile the production daemon binary `watchai-daemon` in release mode (`--release -p watchai-daemon`) with Cargo artifacts isolated inside the Meson build directory (e.g. via `--target-dir` pointing inside the build tree), ensuring the top-level `<repository>/target/` directory is NOT polluted by Meson builds.
- **FR-003**: The compiled release binary `watchai-daemon` MUST be materialized at the exact Meson `custom_target()` output path expected by Meson, and installed to `@bindir@` (e.g. `$prefix/bin/watchai-daemon`) with executable permissions (`0755` or equivalent).
- **FR-004**: Meson build MUST strictly EXCLUDE `watchai-mock` and test-only helper infrastructure from production installation targets.
- **FR-005**: Meson MUST install all production GNOME Shell extension files from `extension/` (`extension.js`, `indicator.js`, `popover.js`, `settings.js`, `notifications.js`, `dbus_client.js`, `utils.js`, `metadata.json`, `stylesheet.css`) to `@datadir@/gnome-shell/extensions/watchai@gnome.org/`. Test files (`extension/tests/`) MUST NOT be installed.
- **FR-006**: Meson MUST install the GSettings XML schema `extension/schemas/org.gnome.shell.extensions.watchai.gschema.xml` into the extension's local schemas directory (`@datadir@/gnome-shell/extensions/watchai@gnome.org/schemas/`) AND compile the schema database (`gschemas.compiled`) within that directory. Additionally, the schema XML MUST be installed to `@datadir@/glib-2.0/schemas/` and compiled post-install where supported.
- **FR-007**: Meson MUST generate `watchai.service` from a template `systemd/watchai.service.in` substituting `@bindir@` with the configured absolute binary directory, and install it to `@datadir@/systemd/user/watchai.service` (or `@prefix@/lib/systemd/user/watchai.service`).
- **FR-008**: Installed non-executable package assets (extension JavaScript, CSS, metadata, GSettings XML, compiled schema databases, systemd service units) MUST NOT be installed group-writable or world-writable (`0644` or equivalent non-writable mode).
- **FR-009**: `extension/metadata.json` MUST be updated to specify the authoritative upstream repository URL `"https://github.com/HA-ir/WatchAI"`, replacing the placeholder `"https://github.com/watchai/watchai"`.
- **FR-010**: Meson project MUST support clean, idempotent uninstallation (`ninja -C build uninstall`) that removes all files installed by Meson, removes compiled schema files from the extension schemas directory, and leaves no orphaned WatchAI assets in the target prefix.
- **FR-011**: Project MUST provide automated packaging verification that validates the full build, install, artifact inspection, permission checking, uninstallation, and reinstallation sequence within an isolated temporary prefix.

#### Pillar 2: README Documentation (`T072`)
- **FR-012**: System MUST provide a comprehensive, complete `README.md` in the repository root replacing the current placeholder `# WatchAI`.
- **FR-013**: `README.md` MUST document supported Linux desktop environments (kernel 5.8+, GNOME Shell 45, 46, and 47 on Wayland/X11, systemd user session, D-Bus session bus).
- **FR-014**: `README.md` MUST provide step-by-step build and installation commands for unprivileged user-local installation (`--prefix=$HOME/.local`) without requiring `sudo` or `root`.
- **FR-015**: `README.md` MUST explain shell `$PATH` requirements for manual terminal execution of `watchai-daemon` while clarifying that the systemd user service executes via an absolute binary path independently of shell `$PATH`.
- **FR-016**: `README.md` MUST provide clear service lifecycle instructions: reloading systemd daemon (`systemctl --user daemon-reload`), starting/enabling `watchai.service`, and enabling the GNOME Shell extension (`gnome-extensions enable watchai@gnome.org`).
- **FR-017**: `README.md` MUST document all available GSettings configuration keys, default values, and CLI adjustment commands (`gsettings set org.gnome.shell.extensions.watchai ...`), including instructions on using `GSETTINGS_SCHEMA_DIR` when querying user-local schemas from a terminal.
- **FR-018**: `README.md` MUST document verification and troubleshooting workflows: D-Bus inspection via `busctl`, systemd logging via `journalctl --user -u watchai.service`, and clean uninstallation via `ninja -C build uninstall`.

#### Pillar 3: Adapter Development Guide (`T073`)
- **FR-019**: System MUST provide a comprehensive third-party developer guide in `docs/adapter-development.md`.
- **FR-020**: The guide MUST document the `ProviderAdapter` trait contract and its methods (`provider_id`, `display_name`, `capabilities`, `check_environment`, `discover_sessions`).
- **FR-021**: The guide MUST document process discovery heuristics via `/proc/[pid]/cmdline` and cwd inspection, including mandatory false-positive rejection rules and deterministic session ID generation via `derive_process_session_id`.
- **FR-022**: The guide MUST document lifecycle event normalization into `SessionLifecycleEvent`, canonical 8-state FSM transitions, and sanitized `ToolCategory` assignment.
- **FR-023**: The guide MUST document liveness monitoring expectations (2-cycle failure hysteresis, PID reuse defense via `process_start_time`), testing requirements, and the Zero-Leakage Privacy invariant.

#### Pillar 4: Final System-Quality Checklist Audit (`T074`)
- **FR-024**: All 59 requirements quality items in `specs/001-agent-status-indicator/checklists/system-quality.md` (`CHK001`–`CHK059`) MUST be systematically evaluated against the completed codebase.
- **FR-025**: The checklist MUST record concrete verification evidence (file paths, test suites, contracts) for every evaluated item.
- **FR-026**: Every quality item confirming compliance MUST be updated from `[ ]` to `[x]`.
- **FR-027**: Canonical roadmap task `T074` in `specs/001-agent-status-indicator/tasks.md` MUST be synchronized to `[x]`.
- **FR-028**: Task `T022` in `specs/001-agent-status-indicator/tasks.md:74` MUST remain strictly discovery-gated and unchecked `[ ]`.

---

### Key Entities & Packaging Deliverables

1. **`meson.build`**: Root build configuration defining project `watchai` (version 0.1.0), options, isolated Cargo target, schema installation, and installation directories.
2. **`systemd/watchai.service.in`**: Configurable systemd user service unit template substituting `@bindir@` with the configured absolute binary path.
3. **`extension/metadata.json`**: Upstream-aligned GNOME extension manifest with corrected URL.
4. **`README.md`**: Master user-facing installation, configuration, operational, and troubleshooting documentation.
5. **`docs/adapter-development.md`**: Developer and contributor integration guide for AI coding agent provider adapters.
6. **`specs/001-agent-status-indicator/checklists/system-quality.md`**: Audited quality certification checklist with all 59 verified items.

---

## 4. Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A fresh checkout of WatchAI builds, installs, and uninstalls cleanly via Meson and Ninja without warnings or errors:
  ```bash
  meson setup build --prefix=$HOME/.local
  ninja -C build
  ninja -C build install
  ninja -C build uninstall
  ```
- **SC-002**: 100% of installed production artifacts land within the user prefix without requiring `sudo` or `root` permissions.
- **SC-003**: `watchai-mock` and test socket infrastructure are 100% absent from all installed directories after `ninja install`.
- **SC-004**: When installed to `$HOME/.local`, GNOME Shell resolves the `org.gnome.shell.extensions.watchai` GSettings schema from `<extension_dir>/schemas/` without requiring manual `$XDG_DATA_DIRS` exports.
- **SC-005**: 100% of installed files satisfy security permissions: `watchai-daemon` is executable (`0755`), and non-executable assets are not group-writable or world-writable (`0644`).
- **SC-006**: 100% of documented commands in `README.md` execute successfully on supported Linux distributions (Ubuntu 24.04+, Fedora 40+).
- **SC-007**: 100% of the 59 quality items in `specs/001-agent-status-indicator/checklists/system-quality.md` (`CHK001`–`CHK059`) are audited, marked `[x]`, and backed by verifiable implementation evidence.
- **SC-008**: Existing verification baselines remain 100% passing with zero regressions:
  - 110 Rust unit, integration, and E2E tests (`cargo test --workspace`).
  - 6 GJS test suites (`gjs -m extension/tests/test_*.js`).
  - Strict GSettings schema validation (`glib-compile-schemas --strict --dry-run extension/schemas/`).
  - `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`.

---

## 5. Assumptions & Environment Context

- **Build Dependencies**: System provides `meson` (>= 0.60.0), `ninja`, `rustc`/`cargo` (>= 1.75), `glib-2.0` (with `glib-compile-schemas`), and `python3`.
- **Target OS**: Linux desktop running systemd user manager (`systemd --user`) and D-Bus session bus.
- **Desktop Environment**: GNOME Shell 45, 46, or 47 on Wayland or X11.
- **User-Space Principle**: WatchAI is designed as an unprivileged, local-first application; all installation and runtime procedures default to user-local paths (`$HOME/.local`).
- **Contract Stability**: Public D-Bus wire protocol `(sssssssus)` and interface `org.freedesktop.WatchAI` remain 100% unchanged.
- **Task T022 Invariant**: Claude Code opt-in hook event telemetry receiver remains discovery-gated and strictly unchecked `[ ]`.

---

## 6. Explicit Out-of-Scope Items

- Creating distribution-specific binary packages (`.deb`, `.rpm`, Arch PKGBUILD, or Flatpak manifests). Meson provides the upstream standard build/install foundation upon which downstream packagers create distribution packages.
- Adding new AI provider adapters (e.g. Aider, Cursor). Phase 12 documents *how* to add them via `docs/adapter-development.md`; implementation of future adapters is post-v1.0.0.
- Implementing Task T022 (remains discovery-gated and unchecked `[ ]`).
- Network-based telemetry or cloud crash reporting.

---

## 7. Traceability Matrix (T071–T074)

| Canonical Task | Roadmap Description | Functional Requirements | Success Criteria | Target Files |
| :--- | :--- | :--- | :--- | :--- |
| **`T071`** | Meson build/install/uninstall configuration & packaging verification | `FR-001` through `FR-011` | `SC-001`, `SC-002`, `SC-003`, `SC-004`, `SC-005` | `meson.build`<br>`systemd/watchai.service.in`<br>`extension/metadata.json`<br>packaging test runner |
| **`T072`** | Comprehensive user documentation in README | `FR-012` through `FR-018` | `SC-006` | `README.md` |
| **`T073`** | Provider adapter integration guide | `FR-019` through `FR-023` | `SC-007` | `docs/adapter-development.md` |
| **`T074`** | Final system-quality checklist audit | `FR-024` through `FR-028` | `SC-007`, `SC-008` | `specs/001-agent-status-indicator/checklists/system-quality.md`<br>`specs/001-agent-status-indicator/tasks.md` |
