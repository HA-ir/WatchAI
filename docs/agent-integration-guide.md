# WatchAI Agent Integration & Telemetry Guide

WatchAI monitors local AI coding agents running on your machine and provides real-time desktop status in the GNOME top bar.

Depending on the agent you use and your precision requirements, WatchAI supports three distinct monitoring methods:

1. **Passive `/proc` Process Discovery** (Zero configuration, baseline liveness)
2. **Transparent Execution Wrapper (`watchai-daemon wrap`)** (Highest accuracy for OpenCode & Codex CLI)
3. **Event Hook Forwarder (`watchai-daemon hook`)** (Native for Claude Code, customizable for scripts)

---

## Comparison Matrix: Accuracy & Trade-offs

| Metric / Capability | 1. Passive `/proc` Discovery | 2. Transparent Wrapper (`wrap`) ⭐ | 3. Hook Forwarder (`hook`) |
| :--- | :---: | :---: | :---: |
| **Accuracy** | Baseline (Process alive only) | **100% Deterministic** | **100% Deterministic** |
| **Latency** | ~2 seconds polling cycle | **0 ms (Instantaneous)** | **0 ms (Instantaneous)** |
| **Active States** | `IDLE` | `WORKING` $\to$ `SUCCESS` / `ERROR` | `WORKING` / `WAITING` / `SUCCESS` / `ERROR` |
| **Tool Breakdown** | None | None | Granular (Bash, Read, Edit, Search) |
| **Turn Completion Dwell**| None | **Full 60s dwell on exit code** | **Full 60s dwell on turn completion** |
| **Configuration** | **Zero configuration** | **Single shell alias** | Native settings or script pipe |
| **Recommended For** | Fallback / any process | **OpenCode, OpenAI Codex CLI** | **Claude Code** |

---

## Method 1: Passive `/proc` Process Discovery (Zero Config)

### How It Works
The WatchAI background daemon scans `/proc` every 2 seconds for known process names (`claude`, `codex`, `codex-cli`, `opencode`) and script runners (`python3 opencode.py`, `node codex.js`).

- **Workspace Path**: Resolved non-invasively via the symlink target of `/proc/<PID>/cwd`.
- **Liveness & Safety**: Process start times from `/proc/<PID>/stat` are tracked to prevent PID reuse errors and ghost sessions.
- **State Constraint**: Per the WatchAI Epistemic Boundary (Constitution Principle III), passive `/proc` discovery strictly initializes sessions in `IDLE`. The daemon cannot guess whether the model is actively computing tokens without authoritative events.

### Usage
No setup needed. Simply run your agent in any terminal:
```bash
claude
# or
opencode
# or
codex
```
The session will appear in the top-bar popover under its detected provider name.

---

## Method 2: Transparent Execution Wrapper (Recommended for OpenCode & Codex CLI)

### Why It's the Most Accurate for OpenCode & Codex
CLI tools like OpenCode and Codex CLI execute discrete turns or commands from the terminal. The `watchai-daemon wrap` command transparently wraps your agent invocation:

1. **Immediate PID Capture (0ms)**: Launches the child process directly and emits a `UserPromptSubmit` event, immediately changing the top-bar indicator into the active blue `WORKING` spinner.
2. **Deterministic Exit Codes**:
   - Exit code `0` $\to$ emits `Stop` (`SUCCESS` state, dwells green for 60 seconds).
   - Exit code `!= 0` $\to$ emits `StopFailure` (`ERROR` state, dwells red for 60 seconds).
3. **Transparent I/O**: Passes standard input, output, error streams, terminal ANSI colors, and cursor navigation with zero performance penalty.

### Setup Guide

Add shell aliases to your `~/.bashrc` or `~/.zshrc`:

#### For OpenCode:
```bash
alias opencode="watchai-daemon wrap --provider opencode -- opencode"
```

#### For OpenAI Codex CLI:
```bash
alias codex="watchai-daemon wrap --provider codex-cli -- codex"
```

#### For Any Custom AI Agent or Script:
```bash
alias myagent="watchai-daemon wrap --provider opencode -- /path/to/my-agent"
```

Reload your shell:
```bash
source ~/.bashrc  # or source ~/.zshrc
```

Now, whenever you run `codex` or `opencode`, WatchAI will track its full lifecycle in real time.

---

## Method 3: Event Hook Forwarder (Native for Claude Code)

For agents with built-in hook systems (such as Claude Code) or custom wrapper scripts, WatchAI provides a sub-millisecond hook forwarder: `watchai-daemon hook`.

### Claude Code Setup (Automated)

Claude Code features native JSON lifecycle hooks. WatchAI can register these automatically:

```bash
# Register WatchAI activity hooks in ~/.claude/settings.json
watchai-daemon install-hooks

# Verify active hook registration
watchai-daemon status-hooks
```

This configures Claude Code to invoke `watchai-daemon hook` on the following events:
- `UserPromptSubmit` $\to$ `WORKING`
- `PreToolUse` $\to$ `WORKING` (displays tool category: Bash, Read, Edit, Search)
- `PostToolUse` $\to$ `WORKING`
- `PermissionRequest` $\to$ `WAITING` (triggers desktop notification)
- `PostToolUseFailure` $\to$ `ERROR`
- `Stop` $\to$ `SUCCESS` (dwells for 60 seconds or until new prompt)
- `SessionEnd` $\to$ `SUCCESS` (clean exit) or `ERROR` (crash)

To remove Claude Code hooks at any time:
```bash
watchai-daemon uninstall-hooks
```

---

### Custom Script / OpenCode / Codex Hook Integration

If you run OpenCode or Codex inside a custom script or pipeline, you can forward JSON events directly into WatchAI:

```bash
# Export the active process ID
export OPENCODE_PID=$$

# Send a TurnStart event
echo '{"hook_event_name": "TurnStart"}' | watchai-daemon hook --provider opencode

# Send a ToolExecute event
echo '{"hook_event_name": "ToolExecute", "tool_name": "read_file"}' | watchai-daemon hook --provider opencode

# Send a WaitingInput event (prompts user for confirmation)
echo '{"hook_event_name": "WaitingInput"}' | watchai-daemon hook --provider opencode

# Send a Done event (triggers 60s green success dwell)
echo '{"hook_event_name": "Done"}' | watchai-daemon hook --provider opencode
```

#### Supported Event Names:
| Event Name | Mapped State | Description |
| :--- | :--- | :--- |
| `TurnStart`, `UserPromptSubmit`, `CommandStart` | `WORKING` | Agent is processing or thinking |
| `ToolExecute`, `PreToolUse`, `ToolStart` | `WORKING` | Tool execution with active category |
| `PromptUser`, `PermissionRequest`, `WaitingInput` | `WAITING` | Waiting for user approval / input |
| `ToolFailure`, `ToolError`, `Error` | `ERROR` | Execution or tool failure |
| `Stop`, `Done`, `Complete`, `TurnComplete` | `SUCCESS` | Clean completion (60s dwell) |
| `StopFailure` | `ERROR` | Failed completion (60s dwell) |
| `SessionEnd` | `SUCCESS` / `ERROR` | Session termination |

---

## 60-Second Completion Dwell Behavior

When an agent finishes a turn or command cleanly, WatchAI holds the top-bar indicator in the **green `SUCCESS`** state for **60 seconds**.

- **Immediate Turn Recovery**: If you submit another prompt or run another command *before* the 60 seconds elapse, WatchAI immediately transitions the session back to `WORKING`.
- **Automatic Idle Settle**: If 60 seconds elapse with no further activity and the process is still running, WatchAI smoothly settles the session to `IDLE`.
- **Clean Process Termination**: When an interactive session exits cleanly from `IDLE` or `SUCCESS`, it terminates cleanly without triggering false "crashed" alarms.
