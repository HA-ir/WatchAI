# Tasks: Phase 12 — Release Packaging, Installation Documentation & Final System-Quality Audit

**Feature**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md) | **Branch**: `012-release-packaging-final-audit`

---

## Task Breakdown

### Phase 1: Build Automation & Cargo Integration (`T071`)

- [x] **Task 1.1**: Create `build-aux/cargo-build.py` helper script compiling `watchai-daemon` with `--target-dir <builddir>/cargo-target` and copying release binary to `@OUTPUT@`.
- [x] **Task 1.2**: Create parameterized `systemd/watchai.service.in` template substituting `ExecStart=@bindir@/watchai-daemon`.
- [x] **Task 1.3**: Update `extension/metadata.json` line 10 to point to upstream repository `"https://github.com/HA-ir/WatchAI"`.
- [x] **Task 1.4**: Implement root `meson.build` orchestrating Cargo build, dual-location schema installation/compilation, systemd unit generation, and extension bundling with secure permissions (`0755`/`0644`).

### Phase 2: Packaging Verification Automation (`T071`)

- [x] **Task 2.1**: Implement `tests/packaging/verify_packaging.py` validating static assets and full setup/build/install/permission/uninstall/reinstall sequence in an isolated prefix.

### Phase 3: Comprehensive User Documentation (`T072`)

- [x] **Task 3.1**: Author master `README.md` documenting prerequisites, user-local unprivileged installation (`--prefix=$HOME/.local`), service lifecycle, extension enabling, GSettings preferences CLI commands, shell `$PATH` guidance, troubleshooting, and privacy guarantees.

### Phase 4: Third-Party Provider Adapter Integration Guide (`T073`)

- [x] **Task 4.1**: Author `docs/adapter-development.md` detailing the `ProviderAdapter` trait, process discovery heuristics, false-positive rejection rules, lifecycle state normalization, sanitized tool categories, liveness monitoring, and zero-leakage privacy invariants.

### Phase 5: Final System-Quality Audit & Roadmap Synchronization (`T074`)

- [x] **Task 5.1**: Evaluate and mark all 59 quality criteria in `specs/001-agent-status-indicator/checklists/system-quality.md` (`CHK001`–`CHK059`) with concrete implementation evidence citations.
- [x] **Task 5.2**: Execute complete verification suite baseline (Cargo fmt/clippy/tests, packaging verification, 6 GJS test suites, schema validation, git diff checks).
- [x] **Task 5.3**: Synchronize canonical roadmap tasks in `specs/001-agent-status-indicator/tasks.md` (`T071`–`T074` to `[x]`, confirming `T022` remains discovery-gated and unchecked `[ ]`).
