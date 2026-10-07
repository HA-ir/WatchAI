# OpenAI Codex CLI Provider Discovery & Telemetry Guide

WatchAI provides native integration with OpenAI Codex CLI through both non-invasive process discovery and opt-in event telemetry.

## 1. Process Discovery (Baseline)

The WatchAI daemon performs periodic sweeps (every 2 seconds) of `/proc` looking for running Codex processes.
- **Process names detected**: `codex`, `codex-cli`, script runners executing `codex.js`.
- **Initial State**: Strictly initializes in `IDLE` (`DiscoveryRequired` or `Active`).
- **Project Directory**: Extracted non-invasively by reading `/proc/[PID]/cwd`.
- **Liveness & PID Reuse**: Monitored via `/proc/[PID]/stat` start-time validation.

## 2. Event Telemetry (Opt-In)

To elevate OpenAI Codex CLI to authoritative real-time telemetry (`WORKING`, `WAITING`, `SUCCESS`, `ERROR` with 60s completion dwell):

### Option A: Transparent Execution Wrapper (Recommended)

Wrap Codex CLI invocations using the `watchai-daemon wrap` command:

```bash
# Direct command invocation
watchai-daemon wrap --provider codex-cli -- codex run

# Shell alias (add to ~/.bashrc or ~/.zshrc)
alias codex="watchai-daemon wrap --provider codex-cli -- codex"
```

#### Wrapper Lifecycle:
1. Spawns `codex` with full terminal pass-through.
2. Emits `UserPromptSubmit` -> transitions session to `WORKING`.
3. Monitors process exit code:
   - Code `0` -> emits `Stop` (`SUCCESS` state, dwells for 60 seconds).
   - Code `!= 0` -> emits `StopFailure` (`ERROR` state, dwells for 60 seconds).

### Option B: Script / Hook Forwarder

If integrating via external orchestration or wrappers, pipe JSON payloads directly into:

```bash
watchai-daemon hook --provider codex-cli
```

Set `CODEX_PID=$$` in the script environment to correlate telemetry to the active process.

#### Supported Hook Events:
| Hook Event | WatchAI State | Description |
| :--- | :--- | :--- |
| `TurnStart` / `CommandStart` | `WORKING` | Turn started, model actively executing |
| `ToolExecute` / `ToolStart` | `WORKING` | Tool execution with category extraction |
| `PermissionRequest` / `ConfirmAction` | `WAITING` | Waiting for user confirmation |
| `ToolFailure` / `PostToolUseFailure` | `ERROR` | Tool execution failed |
| `Stop` / `Done` / `TurnEnd` | `SUCCESS` | Turn completed successfully (60s dwell) |
| `StopFailure` | `ERROR` | Turn completed with failure (60s dwell) |
| `SessionEnd` | `SUCCESS` / `ERROR` | Process termination |
