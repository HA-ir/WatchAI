# End-to-End Test Suite & Mock Verification Harness

WatchAI provides an isolated, deterministic, and 100% headless End-to-End (E2E) testing and simulation framework. It enables comprehensive verification of daemon bootstrap, D-Bus IPC methods and signals, multi-session priority aggregation, and ungraceful crash detection without requiring real proprietary AI coding agent binaries or active display servers.

---

## 1. Architectural Overview

```
┌────────────────────────────────────────────────────────┐
│                      E2E Test                          │
│        (scenario1, scenario2, scenario3, scenario4)    │
└───────────────┬────────────────────────┬───────────────┘
                │                        │
       Runs CLI │                        │ zbus 4.4 IPC
                ▼                        ▼
┌───────────────────────────────┐ ┌──────────────────────┐
│         watchai-mock          │ │ Ephemeral D-Bus Bus  │
│  - session start/transition   │ │ (dbus-daemon session)│
│  - worker run/kill            │ └──────────┬───────────┘
└───────────────┬───────────────┘            │
                │ NDJSON over                │ org.freedesktop.WatchAI
                │ WATCHAI_TEST_SOCKET        │ (sssssssus)
                ▼                            ▼
┌────────────────────────────────────────────────────────┐
│                     watchai-daemon                     │
│  - Real production binary                              │
│  - Test socket listener (active ONLY when env var set) │
│  - Volatile SessionRegistry & core aggregation engine  │
│  - Non-invasive /proc scanner & liveness loop          │
└────────────────────────────────────────────────────────┘
```

### Key Architectural Invariants

1. **Production Purity**: When `WATCHAI_TEST_SOCKET` is unset (normal production and systemd user services), the daemon binds zero test sockets and exposes zero test injection endpoints.
2. **Bounded Socket Ingestion**: The Unix-domain stream listener enforces a strict **64 KiB line limit** to protect against unbounded memory allocation. Malformed JSON frames are safely logged and discarded without interrupting stream processing.
3. **Full D-Bus Isolation**: Every test scenario spawns its own ephemeral `dbus-daemon --session --print-address --nofork` instance, guaranteeing zero interference with the host desktop bus.
4. **Leak-Proof Child Cleanup (`ChildGuard`)**: All child processes (`watchai-daemon`, `dbus-daemon`, mock workers) are managed via Linux process groups (`setpgid(0, 0)`). RAII `Drop` sends `SIGTERM` followed by bounded `SIGKILL` and reaps child processes via `wait()`.
5. **Authentic 10-Second Completion Dwell**: Scenario 2 evaluates the authentic, unmodified production 10-second completion dwell window (`COMPLETION_DWELL_SECONDS = 10`) via bounded polling without artificial test overrides.
6. **Zero-Leakage Privacy**: Mock events and assertions strictly adhere to the metadata-only boundary. No prompts, diffs, tool parameters, or credentials enter the mock protocol.
7. **Test-Scoped `/proc` Isolation (`WATCHAI_PROC_ROOT`)**: For isolated E2E test runs, `WATCHAI_PROC_ROOT` may be configured by test fixtures to point the daemon's discovery scanner and liveness reader to an isolated directory, preventing active host AI coding agent processes from contaminating test runs. When unset or empty (normal production and systemd user execution), the daemon operates exclusively against standard Linux `/proc` without configuration changes.

---

## 2. `watchai-mock` CLI Reference

The `watchai-mock` binary is located at `crates/watchai-mock` (compiled to `target/debug/watchai-mock`).

### Global Flags
- `--socket <PATH>`: Absolute path to the daemon's test Unix domain socket (defaults to `WATCHAI_TEST_SOCKET` environment variable).
- `-h`, `--help`: Displays usage instructions.

### Subcommands

#### `session start`
Registers a new mock session with initial state:
```bash
watchai-mock session start \
  --id "sess-1" \
  --provider "claude-code" \
  --project "/path/to/project" \
  --state "Starting" \
  [--pid 12345]
```

#### `session transition`
Transitions an existing session to a target lifecycle state:
```bash
watchai-mock session transition \
  --id "sess-1" \
  --state "Working" \
  [--tool "ShellExecution"]
```
*Note*: Tool categories are normalized to canonical SCREAMING_SNAKE_CASE identifiers (`SHELL_EXECUTION`, `FILE_READ`, `FILE_WRITE`, `SEARCH`, `MODEL_THINKING`). Convenient aliases like `Bash` or `Shell` are accepted by the CLI.

#### `session heartbeat`
Confirms session activity and refreshes `last_seen_at` without changing state:
```bash
watchai-mock session heartbeat --id "sess-1"
```

#### `session terminate`
Signals session termination with an optional exit code:
```bash
watchai-mock session terminate --id "sess-1" [--exit-code 0]
```
- `exit_code: Some(0)` $\rightarrow$ `SUCCESS`
- `exit_code: Some(N > 0)` $\rightarrow$ `ERROR`
- `exit_code: None` $\rightarrow$ `CANCELLED`

#### `worker run`
Spawns a realistic sleeper worker script matching `ProcessScanner` command-line rules for `/proc` discovery:
```bash
watchai-mock worker run \
  --provider "claude-code" \
  --project "/tmp/my-project" \
  --hold-seconds 60
```
Outputs: `Worker spawned: pid=<PID>, provider=..., project=...`

#### `worker kill`
Terminates a spawned worker process:
```bash
watchai-mock worker kill --pid <PID> [--signal "SIGKILL"]
```

---

## 3. Running E2E Verification Tests

Ensure `watchai-daemon` and `watchai-mock` are compiled first:
```bash
cargo build -p watchai-daemon -p watchai-mock
```

### Running All 4 E2E Test Suites
```bash
cargo test -p watchai-integration-tests --test 'scenario*'
```

### Running Individual Scenarios
```bash
# Scenario 1: Daemon bootstrap, readiness, D-Bus name claim within 2.0s, clean SIGTERM shutdown
cargo test -p watchai-integration-tests --test scenario1_startup_test

# Scenario 2: Complete lifecycle progression and authentic 10-second completion dwell
cargo test -p watchai-integration-tests --test scenario2_lifecycle_test

# Scenario 3: Multi-session priority aggregation (ERROR > WAITING > WORKING) and recency tie-breaking
cargo test -p watchai-integration-tests --test scenario3_aggregation_test

# Scenario 4: Real /proc discovery, abrupt SIGKILL termination, and 2-check failure hysteresis (2.0s-4.0s)
cargo test -p watchai-integration-tests --test scenario4_crash_test
```

---

## 4. Troubleshooting & Diagnostics

If an integration test fails, the test fixture automatically captures and prints daemon standard output and error streams.

### Inspecting Captured Logs
Diagnostics are dumped in the test output with clear demarcations:
```
=== WATCHAI-DAEMON STDOUT DIAGNOSTICS ===
... daemon stdout ...
=== WATCHAI-DAEMON STDERR DIAGNOSTICS ===
... daemon stderr ...
=========================================
```

### Common Failure Modes & Solutions

1. **`dbus-daemon` not found in PATH**:
   - Ensure `dbus-daemon` is installed on your Linux system (`sudo apt install dbus` or `sudo dnf install dbus-daemon`).
2. **Missing Binary in `target/debug`**:
   - Run `cargo build -p watchai-daemon -p watchai-mock` prior to executing integration tests.
   - Alternatively, pass custom binary paths via `WATCHAI_DAEMON_BIN` and `WATCHAI_MOCK_BIN`.
3. **Zombie or Orphaned Process Warnings**:
   - The test harness uses Linux process groups (`setpgid(0, 0)`) and sends `SIGTERM`/`SIGKILL` on Drop. If manually aborting tests with `Ctrl+C`, run `pkill -f watchai-daemon` or `pkill -f claude.py` if necessary.
