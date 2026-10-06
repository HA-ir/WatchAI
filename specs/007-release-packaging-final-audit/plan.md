# Implementation Plan: Phase 12 — Release Packaging, Installation Documentation & Final System-Quality Audit

**Branch**: `012-release-packaging-final-audit` | **Date**: 2026-10-06 | **Spec**: [specs/007-release-packaging-final-audit/spec.md](spec.md)

---

## 1. Summary & Objectives

Phase 12 delivers the final production release packaging, user-facing operations manual, third-party adapter development guide, and system-wide quality certification for WatchAI (Tasks `T071`–`T074` from `specs/001-agent-status-indicator/tasks.md`).

It establishes:
1. **Meson Build, Install & Clean Uninstall Packaging (`T071`)**:
   - A root `meson.build` orchestrating Cargo compilation of the production daemon binary (`watchai-daemon`) in release mode.
   - Isolated Cargo target directory (`<builddir>/cargo-target`) preventing any pollution of the top-level `<repository>/target/` directory.
   - Dual-location GSettings schema installation (extension-local `<extension_dir>/schemas/` with pre-compiled `gschemas.compiled` plus `@datadir@/glib-2.0/schemas/`), ensuring out-of-the-box user-local preference resolution in GNOME Shell without requiring manual `$XDG_DATA_DIRS` exports.
   - Configurable `systemd/watchai.service.in` template resolving `@bindir@/watchai-daemon` dynamically to the absolute installed binary path.
   - Production purity boundary: `watchai-mock`, test sockets, and test harnesses are strictly excluded from installation.
   - Secure file permissions: executable binaries installed as `0755`; non-executable assets installed non-writable by group/world (`0644`).
   - Clean, idempotent uninstallation (`ninja -C build uninstall`) leaving zero orphaned files.
   - Automated packaging verification script validating setup, build, install, permissions, uninstall, and reinstall in an isolated temporary prefix.
2. **Comprehensive User Installation & Operation Manual (`T072`)**:
   - Master `README.md` replacing the placeholder `# WatchAI` with complete instructions for prerequisites, user-local unprivileged installation (`--prefix=$HOME/.local`), systemd service management, GNOME extension enabling, GSettings preferences CLI commands, shell `$PATH` guidance, and troubleshooting.
3. **Third-Party AI Provider Adapter Integration Guide (`T073`)**:
   - Authoritative developer documentation in `docs/adapter-development.md` detailing the `ProviderAdapter` trait, process discovery heuristics, false-positive rejection rules, lifecycle state normalization, sanitized tool categories, liveness monitoring, and zero-leakage privacy invariants.
4. **Final System-Quality Checklist Audit (`T074`)**:
   - Exhaustive evaluation and marking of all 59 quality criteria in `specs/001-agent-status-indicator/checklists/system-quality.md` (`CHK001`–`CHK059`), certifying that WatchAI complies with the project Constitution and production readiness criteria.
5. **Strict Governance & Contract Invariants**:
   - Public D-Bus wire protocol `(sssssssus)` and interface `org.freedesktop.WatchAI` remain 100% immutable.
   - Task `T022` remains strictly discovery-gated and unchecked `[ ]`.
   - Zero elevated root/sudo privileges required; all runtime and installation workflows default to unprivileged user-local paths.

---

## 2. Technical Context & Packaging Architecture

### 2.1 The Cargo-Meson Build Integration Architecture

```
┌────────────────────────────────────────────────────────┐
│                      meson setup                       │
│    meson setup build --prefix=$HOME/.local             │
└───────────────┬────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────┐
│                 build-aux/cargo-build.py               │
│  - Sets --target-dir <builddir>/cargo-target           │
│  - Runs: cargo build --release -p watchai-daemon       │
│  - Copies target binary directly to @OUTPUT@           │
│  - Leaves <repo>/target/ completely untouched          │
└───────────────┬────────────────────────────────────────┘
                │
                ▼
┌────────────────────────────────────────────────────────┐
│                      ninja install                     │
│  1. Binary: $prefix/bin/watchai-daemon (0755)          │
│  2. Extension: $datadir/gnome-shell/extensions/        │
│     watchai@gnome.org/ (0644)                          │
│  3. Local Schema: .../watchai@gnome.org/schemas/       │
│     - org.gnome.shell.extensions.watchai.gschema.xml   │
│     - gschemas.compiled (0644)                         │
│  4. System Schema: $datadir/glib-2.0/schemas/ (0644)   │
│  5. Systemd: $datadir/systemd/user/watchai.service     │
│     (ExecStart=$prefix/bin/watchai-daemon) (0644)      │
│  6. watchai-mock EXCLUDED (production purity)          │
└────────────────────────────────────────────────────────┘
```

### 2.2 Dual-Tiered GSettings Schema Resolution Strategy

GNOME Shell extensions running in modern GNOME (45, 46, 47) execute `extension.getSettings()` in `extension/settings.js`.
- On standard Linux desktop distributions (Ubuntu, Debian, Fedora), `$HOME/.local/share` is NOT present in `$XDG_DATA_DIRS` by default (which contains `/usr/share/ubuntu:/usr/share/gnome:...`).
- If a schema is installed solely to `$HOME/.local/share/glib-2.0/schemas/`, GNOME Shell cannot find it, and falls back to in-memory defaults.
- However, GNOME's `Extension` base class automatically inspects the extension's private directory: `<extension_dir>/schemas/gschemas.compiled`.
- **Strategy**:
  1. Compile and install `gschemas.compiled` and the XML schema into `$datadir/gnome-shell/extensions/watchai@gnome.org/schemas/`. This guarantees GNOME Shell resolves the schema out-of-the-box in user-local installations without requiring users to export `$XDG_DATA_DIRS`.
  2. Also install the XML schema into `$datadir/glib-2.0/schemas/` with post-install compilation so that system-wide installations and CLI `gsettings` queries find it.
  3. Meson tracks both installations, ensuring `ninja uninstall` removes schema files from both directories without leaving residual files.

### 2.3 Dynamic systemd User Unit Generation

- Template file: `systemd/watchai.service.in` containing `ExecStart=@bindir@/watchai-daemon`.
- Meson's `configure_file(input: 'systemd/watchai.service.in', output: 'watchai.service', configuration: conf_data)` substitutes `@bindir@` with the configured binary directory (e.g. `/home/user/.local/bin` or `/usr/bin`).
- Unit is installed to `@datadir@/systemd/user/watchai.service` (or `@prefix@/lib/systemd/user/`).
- Using the absolute path guarantees that systemd user manager executes the daemon regardless of the user's interactive shell `$PATH`.

---

## 3. Constitution Check

| Principle | Status | Verification & Architectural Evidence |
| :--- | :---: | :--- |
| **I. Provider-Agnostic Architecture** | **PASS** | `docs/adapter-development.md` formalizes the `ProviderAdapter` trait and Tiered Discovery Model, enabling arbitrary new agents without modifying core FSM or UI code. |
| **II. Strict Layered Separation** | **PASS** | Meson installs each layer to its standard desktop location; UI connects exclusively over D-Bus; zero direct agent detection in UI. |
| **III. Explicit State-Machine Modeling** | **PASS** | Canonical 8-state FSM, priority scoring, completion dwell (10s), and terminal immunity are verified in the final quality checklist (`CHK011`–`CHK020`). |
| **IV. Local-First & Zero-Leakage Privacy** | **PASS** | Absolute ban on prompts, code, tokens, or credentials documented in `README.md`, `docs/adapter-development.md`, and certified in quality checklist (`CHK045`–`CHK049`). |
| **V. GNOME Extension Stability** | **PASS** | Dual schema packaging ensures non-blocking schema resolution; GJS memory and reconnection resilience certified in quality audit (`CHK036`–`CHK040`). |
| **VI. Determinism & Quality Gates** | **PASS** | Automated packaging verification exercises setup, build, install, permissions, uninstall, and reinstall in an isolated temporary prefix. |
| **VII. Contract Stability & Documentation** | **PASS** | D-Bus interface `org.freedesktop.WatchAI` and signature `(sssssssus)` remain immutable; Task `T022` remains discovery-gated and unchecked `[ ]`. |

---

## 4. Affected & New Files

### New Files to Create

1. **`meson.build`**:
   - Root project configuration (`project('watchai', version: '0.1.0', meson_version: '>=0.60.0')`).
   - Declares options, detects tools (`cargo`, `glib-compile-schemas`), invokes `cargo-build.py`, configures GSettings dual-installation, generates `watchai.service`, and installs extension assets.
2. **`build-aux/cargo-build.py`**:
   - Python helper script invoked by Meson `custom_target` to run `cargo build --release --manifest-path ... --target-dir <builddir>/cargo-target -p watchai-daemon` and copy the resulting binary to `@OUTPUT@`.
3. **`systemd/watchai.service.in`**:
   - Parameterized systemd user service unit template substituting `ExecStart=@bindir@/watchai-daemon`.
4. **`docs/adapter-development.md`**:
   - Comprehensive third-party developer integration guide for AI coding agent adapters.
5. **`tests/packaging/verify_packaging.py`**:
   - Automated packaging test runner executing Meson setup, build, install into an isolated temporary directory, inspecting binaries, checking permissions, validating service and schema files, verifying absence of `watchai-mock`, testing clean uninstallation, and confirming reinstallation idempotency.

### Files to Modify

1. **`extension/metadata.json`**:
   - Update line 10 to point to the authoritative upstream URL: `"https://github.com/HA-ir/WatchAI"`.
2. **`README.md`**:
   - Replace placeholder with comprehensive user installation, configuration, operational, verification, and uninstallation documentation.
3. **`specs/001-agent-status-indicator/checklists/system-quality.md`**:
   - Audit and mark all 59 quality items (`CHK001`–`CHK059`) as verified `[x]` with concrete implementation and test citations.
4. **`specs/001-agent-status-indicator/tasks.md`**:
   - Synchronize Phase 12 canonical tasks `T071`, `T072`, `T073`, `T074` to completed `[x]`.

---

## 5. Detailed Implementation Tasks & Work Breakdown

### Phase 1: Build Automation & Cargo Integration (`T071`)

#### Task 1.1: Create `build-aux/cargo-build.py` Helper Script
- **File**: `build-aux/cargo-build.py`
- **Responsibility**: Accepts source directory, build directory, output file path, and package name. Invokes `cargo build --release --manifest-path <source>/Cargo.toml --package <package> --target-dir <builddir>/cargo-target`. Copies `<builddir>/cargo-target/release/<package>` to the exact output path expected by Meson (`@OUTPUT@`).
- **Dependencies**: None.
- **Verification**: Run directly via Python and verify binary materialization at output path and zero modifications to `<repo>/target/`.

#### Task 1.2: Create Template `systemd/watchai.service.in`
- **File**: `systemd/watchai.service.in`
- **Responsibility**: Declares systemd user unit with `Type=notify`, `BusName=org.freedesktop.WatchAI`, and `ExecStart=@bindir@/watchai-daemon`.
- **Dependencies**: None.
- **Verification**: Text inspection against existing `systemd/watchai.service`.

#### Task 1.3: Update Extension Repository Metadata
- **File**: `extension/metadata.json`
- **Responsibility**: Update `"url": "https://github.com/watchai/watchai"` to `"url": "https://github.com/HA-ir/WatchAI"`.
- **Dependencies**: None.
- **Verification**: JSON syntax check and text inspection.

#### Task 1.4: Implement Root `meson.build`
- **File**: `meson.build`
- **Responsibility**:
  - Define project `watchai` with Meson version `>=0.60.0`.
  - Locate `cargo` and `glib-compile-schemas` using `find_program(..., required: true)`.
  - Define `custom_target` for `watchai-daemon` using `build-aux/cargo-build.py`, installing to `get_option('bindir')` with mode `rwxr-xr-x`.
  - Define `configure_file` generating `watchai.service` from `systemd/watchai.service.in`, installing to `get_option('datadir') / 'systemd' / 'user'` with mode `rw-r--r--`.
  - Install GNOME Shell extension files from `extension/` to `get_option('datadir') / 'gnome-shell' / 'extensions' / 'watchai@gnome.org'` with mode `rw-r--r--`. Exclude `extension/tests/`.
  - Install schema XML to `extension_dir / 'schemas'` and compile `gschemas.compiled` at build time to install into `extension_dir / 'schemas'` with mode `rw-r--r--`.
  - Install schema XML to `get_option('datadir') / 'glib-2.0' / 'schemas'` with mode `rw-r--r--` and register `gnome.post_install(glib_compile_schemas: true)`.
- **Dependencies**: Tasks 1.1, 1.2, 1.3.
- **Verification**: `meson setup` and `ninja` syntax inspection.

---

### Phase 2: Packaging Verification Automation (`T071`)

#### Task 2.1: Implement Automated Packaging Verification Runner
- **File**: `tests/packaging/verify_packaging.py`
- **Responsibility**: Automate end-to-end packaging verification in an isolated temporary directory:
  1. Executes `meson setup <builddir> --prefix=<test_prefix>`.
  2. Executes `ninja -C <builddir>`.
  3. Executes `ninja -C <builddir> install`.
  4. Asserts `<test_prefix>/bin/watchai-daemon` exists and is executable (`0755`).
  5. Asserts `<test_prefix>/bin/watchai-mock` is strictly ABSENT.
  6. Asserts `<test_prefix>/share/gnome-shell/extensions/watchai@gnome.org/metadata.json` exists with correct URL.
  7. Asserts `<test_prefix>/share/gnome-shell/extensions/watchai@gnome.org/schemas/gschemas.compiled` exists.
  8. Asserts `<test_prefix>/share/systemd/user/watchai.service` contains `ExecStart=<test_prefix>/bin/watchai-daemon`.
  9. Asserts installed non-executable assets are not group/world writable.
  10. Executes `ninja -C <builddir> uninstall`.
  11. Asserts all installed WatchAI assets are removed.
  12. Executes `ninja -C <builddir> install` again and confirms reinstallation is clean and idempotent.
- **Dependencies**: Tasks 1.4.
- **Verification**: Execute `python3 tests/packaging/verify_packaging.py` and assert exit code 0.

---

### Phase 3: Comprehensive User Documentation (`T072`)

#### Task 3.1: Author Comprehensive `README.md`
- **File**: `README.md`
- **Responsibility**: Replace placeholder with complete end-user documentation:
  - Project overview and supported AI agent providers.
  - Prerequisites and supported Linux desktop environments (kernel 5.8+, GNOME Shell 45/46/47 on Wayland/X11, systemd user session, D-Bus session bus).
  - Build dependencies (`meson`, `ninja`, `rustc`/`cargo`, `glib-2.0`).
  - Unprivileged user-local build and installation commands (`meson setup build --prefix=$HOME/.local`, `ninja -C build install`).
  - Shell `$PATH` guidance for manual terminal invocation of `watchai-daemon`.
  - Service lifecycle management (`systemctl --user daemon-reload`, `systemctl --user enable --now watchai.service`).
  - Extension management (`gnome-extensions enable watchai@gnome.org`).
  - User configuration via GSettings (all keys, default values, `gsettings` CLI commands, and `GSETTINGS_SCHEMA_DIR` notes).
  - Verification commands (`busctl --user introspect ...`, `journalctl --user -u watchai.service -f`).
  - Troubleshooting tips and clean uninstallation (`ninja -C build uninstall`).
  - Local-first privacy guarantees.
- **Dependencies**: Tasks 1.4.
- **Verification**: Markdown formatting and link validation.

---

### Phase 4: Third-Party Provider Adapter Integration Guide (`T073`)

#### Task 4.1: Author `docs/adapter-development.md`
- **File**: `docs/adapter-development.md`
- **Responsibility**: Authoritative guide for third-party developers implementing new AI agent adapters:
  - System architecture and layered decoupling (Adapters $\rightarrow$ Daemon $\rightarrow$ D-Bus $\rightarrow$ Extension).
  - The `ProviderAdapter` trait contract and required methods.
  - Process scanning heuristics (`ProcessScanner`), null-delimited `/proc/[pid]/cmdline` parsing, and cwd inspection.
  - Mandatory false-positive rejection rules (filtering shells, grep, git, pytest).
  - Deterministic session ID derivation using `derive_process_session_id`.
  - Lifecycle state normalization (canonical 8 states and legal transitions).
  - Sanitized `ToolCategory` mapping (`FILE_READ`, `FILE_WRITE`, `SHELL_EXECUTION`, `SEARCH`, `MODEL_THINKING`).
  - Event streaming via `EventSink` (`SessionLifecycleEvent`).
  - Liveness monitoring and 2-cycle failure hysteresis.
  - Zero-Leakage Privacy invariant (strict prohibition on prompts, code, diffs, tool parameters, tokens, credentials).
  - Step-by-step tutorial on registering a new adapter in `AdapterRegistry`.
  - Unit and integration testing standards.
- **Dependencies**: None.
- **Verification**: Review against existing implementations in `crates/watchai-adapters/`.

---

### Phase 5: Final System-Quality Audit & Roadmap Synchronization (`T074`)

#### Task 5.1: Audit All 59 Items in `specs/001-agent-status-indicator/checklists/system-quality.md`
- **File**: `specs/001-agent-status-indicator/checklists/system-quality.md`
- **Responsibility**: Systematically evaluate each of the 59 quality items (`CHK001` through `CHK059`), recording concrete implementation evidence (file paths, test cases, contracts, line references), and mark each verified item complete `[x]`.
- **Dependencies**: Phases 1–4.
- **Verification**: Checklist completeness review (59/59 marked `[x]`).

#### Task 5.2: Execute Full Verification Suite Baseline
- **Responsibility**: Run all automated quality and verification gates:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace` (all 110 Rust unit, integration, and E2E tests)
  - `python3 tests/packaging/verify_packaging.py`
  - All 6 GNOME Shell extension GJS test suites (`gjs -m extension/tests/test_*.js`)
  - Strict GSettings schema validation (`glib-compile-schemas --strict --dry-run extension/schemas/`)
  - `git diff --check`
- **Dependencies**: Tasks 1.4, 2.1, 5.1.
- **Verification**: 100% exit code 0 across all verification suites.

#### Task 5.3: Synchronize Canonical Roadmap Tasks
- **File**: `specs/001-agent-status-indicator/tasks.md`
- **Responsibility**: Mark Phase 12 canonical tasks `T071`, `T072`, `T073`, `T074` as completed `[x]`. Confirm task `T022` remains strictly unchecked `[ ]`.
- **Dependencies**: Task 5.2.
- **Verification**: Git diff inspection of `specs/001-agent-status-indicator/tasks.md`.

---

## 6. Traceability Matrix & Requirements Coverage

| Requirement | Description | Planned Task | Success Criteria |
| :--- | :--- | :--- | :--- |
| **`FR-001`** | Root `meson.build` supporting Meson >= 0.60.0 | Task 1.4 | `SC-001` |
| **`FR-002`** | Isolated Cargo release build in Meson build tree | Tasks 1.1, 1.4 | `SC-001` |
| **`FR-003`** | Materialize daemon at Meson output and install to `@bindir@` (0755) | Tasks 1.1, 1.4 | `SC-001`, `SC-005` |
| **`FR-004`** | Exclude `watchai-mock` and test infrastructure from installation | Task 1.4, Task 2.1 | `SC-003` |
| **`FR-005`** | Install GNOME extension files to `$datadir/gnome-shell/extensions/` (0644) | Task 1.4 | `SC-001`, `SC-005` |
| **`FR-006`** | Dual-location GSettings schema installation and compilation | Task 1.4, Task 2.1 | `SC-004` |
| **`FR-007`** | Generate `watchai.service` from template resolving `@bindir@` (0644) | Tasks 1.2, 1.4 | `SC-001`, `SC-005` |
| **`FR-008`** | Installed non-executable assets not group/world writable (0644) | Task 1.4, Task 2.1 | `SC-005` |
| **`FR-009`** | Update `extension/metadata.json` URL to upstream repository | Task 1.3 | `SC-001` |
| **`FR-010`** | Clean, idempotent uninstallation (`ninja uninstall`) | Task 1.4, Task 2.1 | `SC-001` |
| **`FR-011`** | Automated packaging verification in isolated temporary prefix | Task 2.1 | `SC-001`–`SC-005` |
| **`FR-012`** | Comprehensive `README.md` replacing placeholder | Task 3.1 | `SC-006` |
| **`FR-013`** | Document supported desktop environments (Linux 5.8+, GNOME 45+) | Task 3.1 | `SC-006` |
| **`FR-014`** | Document unprivileged user-local build and install commands | Task 3.1 | `SC-002`, `SC-006` |
| **`FR-015`** | Explain shell `$PATH` guidance for manual CLI invocation | Task 3.1 | `SC-006` |
| **`FR-016`** | Document service lifecycle (`systemctl --user`, `gnome-extensions`) | Task 3.1 | `SC-006` |
| **`FR-017`** | Document GSettings preferences, defaults, and CLI commands | Task 3.1 | `SC-006` |
| **`FR-018`** | Document verification (`busctl`, `journalctl`) and uninstall | Task 3.1 | `SC-006` |
| **`FR-019`** | Comprehensive adapter developer guide in `docs/adapter-development.md` | Task 4.1 | `SC-007` |
| **`FR-020`** | Document `ProviderAdapter` trait methods and capabilities | Task 4.1 | `SC-007` |
| **`FR-021`** | Document `/proc` discovery heuristics and rejection rules | Task 4.1 | `SC-007` |
| **`FR-022`** | Document FSM state normalization and sanitized `ToolCategory` | Task 4.1 | `SC-007` |
| **`FR-023`** | Document liveness monitoring, testing, and Zero-Leakage Privacy | Task 4.1 | `SC-007` |
| **`FR-024`** | Evaluate all 59 items in `system-quality.md` | Task 5.1 | `SC-007` |
| **`FR-025`** | Record concrete verification evidence for every item | Task 5.1 | `SC-007` |
| **`FR-026`** | Update evaluated checklist items to complete `[x]` | Task 5.1 | `SC-007` |
| **`FR-027`** | Synchronize canonical roadmap task `T074` to complete `[x]` | Task 5.3 | `SC-007` |
| **`FR-028`** | Confirm task `T022` remains discovery-gated and unchecked `[ ]` | Task 5.3 | Invariant |

---

## 7. Acceptance Scenarios Coverage

- **User Story 1 Scenarios (1–8)**: Covered by Tasks 1.1, 1.2, 1.4, and verified automatically by Task 2.1 (`verify_packaging.py`).
- **User Story 2 Scenarios (1–6)**: Covered by Task 3.1 (`README.md`) and verified via text and link inspection.
- **User Story 3 Scenarios (1–5)**: Covered by Task 4.1 (`docs/adapter-development.md`) and verified against `watchai-adapters` contracts.
- **User Story 4 Scenarios (1–4)**: Covered by Task 5.1, 5.2, and 5.3, completing the full quality certification.

---

## 8. Risks & Mitigations

| Risk | Impact | Mitigation |
| :--- | :---: | :--- |
| **Cargo Build Directory Pollution** | Cargo creating files in `<repo>/target/` breaks Meson clean builds. | `build-aux/cargo-build.py` enforces `--target-dir <builddir>/cargo-target`. |
| **Missing GSettings in User Prefixes** | Extension fails to load schema on Ubuntu/Debian user-local installs. | Dual-location installation: schema XML and compiled `gschemas.compiled` placed in extension's local `schemas/` folder. |
| **systemd `$PATH` Desynchronization** | Daemon fails to start under systemd if `$HOME/.local/bin` is not in service `$PATH`. | `watchai.service.in` template resolves absolute binary path via `@bindir@/watchai-daemon`. |
| **Accidental Mock Packaging** | Development mock binary installed on user desktops. | Meson install target strictly enumerates `watchai-daemon` and explicitly excludes `watchai-mock`. |
| **Insecure Permissions on Multi-User Desktops** | Other local users able to modify installed scripts or service units. | Meson install targets specify explicit permissions (`0755` for binary, `0644` for data and service files). |
| **Broken Reinstall / Stale Schema Caches** | Reinstalling over an existing version leaves orphaned files. | Meson's tracked installation manifest ensures complete removal on `ninja uninstall` and atomic overwrites on reinstall. |

---

## 9. Verification & Quality Gates Summary

Before declaring Phase 12 implementation complete, the following gates must all pass with exit code 0:
1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace` (all 110 Rust unit, integration, and E2E tests)
4. `python3 tests/packaging/verify_packaging.py` (automated packaging lifecycle verification in an isolated temporary prefix)
5. `gjs -m extension/tests/test_*.js` (all 6 GNOME Shell extension GJS test suites)
6. `glib-compile-schemas --strict --dry-run extension/schemas/`
7. Verification that `specs/001-agent-status-indicator/tasks.md:74` (`T022`) remains strictly unchecked `[ ]`
8. `git diff --check`
