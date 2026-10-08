# OpenCode Provider Discovery & Telemetry Guide

WatchAI provides native integration with OpenCode through both non-invasive process discovery and opt-in event telemetry.

## 1. Process Discovery (Baseline)

By default, the WatchAI daemon performs periodic sweeps (every 2 seconds) of `/proc` looking for running `opencode` processes.
- **Process names detected**: `opencode`, script runners executing `opencode.py`.
- **Initial State**: Always initializes in `IDLE` (`AdapterStatus::Active` or `DiscoveryRequired`).
- **Project Directory**: Extracted non-invasively by reading the target of `/proc/[PID]/cwd`.
- **Liveness**: Monitored via `/proc/[PID]/stat` start-time validation to prevent PID reuse anomalies.

## 2. Event Telemetry (Opt-In)

To elevate OpenCode from passive process discovery to active execution monitoring with real-time tool state (`FileRead`, `FileWrite`, `ShellExecution`) and turn completion dwell:

### Option A: Transparent Execution Wrapper (Recommended)

Wrap your interactive or CLI invocations using the `watchai-daemon wrap` command:

```bash
# Direct command invocation
watchai-daemon wrap --provider opencode -- opencode

# Shell alias (add to ~/.bashrc or ~/.zshrc)
alias opencode="watchai-daemon wrap --provider opencode -- opencode"
```

#### Wrapper Lifecycle:
1. Spawns `opencode` with transparent stdio inheritance.
2. Emits `UserPromptSubmit` -> transitions session to `WORKING`.
3. Monitors process exit code:
   - Exit code `0` -> emits `Stop` (`SUCCESS` state, dwells for 60 seconds).
   - Exit code `!= 0` -> emits `StopFailure` (`ERROR` state, dwells for 60 seconds).

### Option B: OpenCode Hook Integration

If using an OpenCode configuration or plugin that supports lifecycle hooks, forward JSON events to:

```bash
watchai-daemon hook --provider opencode
```

Set `OPENCODE_PID=$$` in your hook script to correlate events with the parent process.

#### Supported Hook Events:
| Hook Event | WatchAI State | Description |
| :--- | :--- | :--- |
| `UserPromptSubmit` / `TurnStart` | `WORKING` | Prompt submitted, turn started |
| `ToolExecute` / `PreToolUse` | `WORKING` | Tool execution with category extraction |
| `PromptUser` / `WaitingInput` | `WAITING` | Waiting for user approval/input |
| `ToolFailure` / `ToolError` | `ERROR` | Tool execution failure |
| `Stop` / `Done` / `TurnComplete` | `SUCCESS` | Turn completed successfully (60s dwell) |
| `StopFailure` | `ERROR` | Turn completed with failure (60s dwell) |
| `SessionEnd` | `SUCCESS` / `ERROR` | Clean exit or error exit |
