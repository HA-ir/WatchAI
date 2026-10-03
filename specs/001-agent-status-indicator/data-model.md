# Data Model: Baseline Agent Monitoring & GNOME Shell Indicator

**Feature**: Baseline Agent Monitoring & GNOME Shell Indicator (`001-agent-status-indicator`)  
**Date**: 2026-10-03  
**Status**: Draft  

---

## 1. Domain Entities & Schemas

### 1.1 `LifecycleState` (Enum)
Defines the finite set of valid operational states for any observed agent session:

```text
IDLE       - No active task processing; session is at prompt or unmonitored
STARTING   - Process initialized, bootstrapping or loading model context
WORKING    - Actively processing, executing tools, or generating code
WAITING    - Blocked mid-task awaiting user permission, input, or confirmation
SUCCESS    - Task completed successfully (dwells before returning to IDLE)
ERROR      - Terminated due to error, crash, or unexpected failure
CANCELLED  - Aborted or interrupted by user action
UNKNOWN    - State unverified or telemetry dropped/unobservable
```

---

### 1.2 `AgentSession`
Represents an individual agent process operating within a workspace.

| Field | Type | Description | Privacy / Security Rule |
| :--- | :--- | :--- | :--- |
| `session_id` | `String` | Stable identifier: agent-provided UUID for event streams, or deterministic `SHA256(PID + process_start_time + canonical_path)[0..16]` for unmanaged `/proc` discoveries. | Non-sensitive. |
| `provider_id` | `String` (Slug) | Canonical provider identifier (e.g., `claude-code`, `codex-cli`, `opencode`). | Non-sensitive. |
| `provider_display_name` | `String` | Human-readable provider name (e.g., "Claude Code"). | Non-sensitive. |
| `project_path` | `String` (Filesystem Path) | Absolute path to the monitored project root. | Stripped to basename for standard IPC display; full path kept locally in memory. |
| `project_name` | `String` | Display name of the workspace folder (basename of `project_path`). | Non-sensitive. |
| `current_state` | `LifecycleState` | Current active state in the FSM. | Non-sensitive. |
| `started_at` | `DateTime<Utc>` (ISO 8601) | Timestamp when the session was first discovered. | Non-sensitive. |
| `state_entered_at` | `DateTime<Utc>` (ISO 8601) | Timestamp when the session transitioned into `current_state`. | Non-sensitive. |
| `last_seen_at` | `DateTime<Utc>` (ISO 8601) | Timestamp of the most recent heartbeat, telemetry event, or process check. | Non-sensitive. |
| `process_id` | `Option<u32>` | OS Process ID (PID) of the agent process, if safely discoverable. | Non-sensitive; verified via `/proc`. |
| `active_tool_category` | `Option<String>` | High-level category (e.g., `FileRead`, `ShellExecution`, `ModelThinking`). | **STRICT SANITIZATION**: No tool arguments, file paths, command parameters, or prompt contents allowed. |
| `adapter_status` | `AdapterStatus` | Discovery status (`ACTIVE`, `DISCOVERY_REQUIRED`, `UNAVAILABLE`). | Non-sensitive. |

---

### 1.3 `AggregateState`
Represents the combined desktop-level status exposed to the GNOME Shell top-bar indicator.

| Field | Type | Description |
| :--- | :--- | :--- |
| `state` | `LifecycleState` | The single computed aggregate state representing all active sessions. |
| `active_session_count` | `u32` | Number of currently observed non-terminal sessions. |
| `waiting_session_count` | `u32` | Number of sessions currently blocked in `WAITING`. |
| `error_session_count` | `u32` | Number of sessions currently in `ERROR`. |
| `updated_at` | `DateTime<Utc>` | Timestamp of the latest calculation. |

---

### 1.4 `SessionLifecycleEvent`
Normalized event emitted by provider adapters to trigger state transitions in the engine.

| Field | Type | Description |
| :--- | :--- | :--- |
| `event_id` | `String` (UUIDv4) | Unique event identifier. |
| `session_id` | `String` | Target session identifier. |
| `sequence_number` | `u64` | Monotonically increasing sequence number per session (drops out-of-order events). |
| `timestamp` | `DateTime<Utc>` | Event emission timestamp. |
| `target_state` | `LifecycleState` | Desired state transition. |
| `tool_category` | `Option<String>` | Optional sanitized tool category tag. |
| `error_summary` | `Option<String>` | Generic error category (e.g., `ProcessExitedNonZero`, `ConnectionRefused`). |

---

## 2. State Machine Transition Matrix

Every session FSM must validate incoming events against the permitted transition graph. Invalid transitions are rejected.

| Current State | Permitted Next States | Trigger / Condition |
| :--- | :--- | :--- |
| *(None / Initial)* | `STARTING`, `IDLE`, `WORKING` | Session discovery or `SessionStart` event. |
| `STARTING` | `WORKING`, `IDLE`, `ERROR`, `CANCELLED` | Agent boots up and starts work, or sits at prompt. |
| `WORKING` | `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN` | Requests permission (`WAITING`), finishes task (`SUCCESS`), crashes (`ERROR`), aborted (`CANCELLED`), or drops telemetry (`UNKNOWN`). |
| `WAITING` | `WORKING`, `CANCELLED`, `ERROR`, `UNKNOWN` | User grants/denies permission and execution resumes (`WORKING`), user aborts, or process dies. |
| `SUCCESS` | `WORKING`, `IDLE`, `CANCELLED` | New prompt submitted (`WORKING`) or dwell timer (10s) expires transitioning to `IDLE`. |
| `ERROR` | `STARTING`, `WORKING`, `IDLE`, *(Pruned)* | User restarts task or retention window expires (pruned from memory). |
| `CANCELLED` | `STARTING`, `WORKING`, `IDLE`, *(Pruned)* | User resumes task or retention window expires. |
| `UNKNOWN` | `WORKING`, `WAITING`, `IDLE`, `ERROR`, *(Pruned)* | Liveness restored by heartbeat or process verification (`/proc`), or timeout transitions to `ERROR`. |

---

## 3. Aggregate State Computation Algorithm

The aggregate state exposed to GNOME Shell is computed deterministically from the set $S$ of all tracked sessions:

$$\text{Priority Hierarchy: } \text{ERROR} > \text{WAITING} > \text{WORKING} > \text{STARTING} > \text{CANCELLED} > \text{SUCCESS} > \text{UNKNOWN} > \text{IDLE}$$

### Algorithm Rules:
1. If $S$ is empty $\rightarrow \text{IDLE}$.
2. If $\exists s \in S \text{ such that } s.\text{state} = \text{ERROR} \rightarrow \text{ERROR}$.
3. Else if $\exists s \in S \text{ such that } s.\text{state} = \text{WAITING} \rightarrow \text{WAITING}$.
4. Else if $\exists s \in S \text{ such that } s.\text{state} = \text{WORKING} \rightarrow \text{WORKING}$.
5. Else if $\exists s \in S \text{ such that } s.\text{state} = \text{STARTING} \rightarrow \text{STARTING}$.
6. Else if $\exists s \in S \text{ such that } s.\text{state} = \text{CANCELLED} \rightarrow \text{CANCELLED}$ (within dwell window).
7. Else if $\exists s \in S \text{ such that } s.\text{state} = \text{SUCCESS} \rightarrow \text{SUCCESS}$ (within dwell window).
8. Else if $\exists s \in S \text{ such that } s.\text{state} = \text{UNKNOWN} \rightarrow \text{UNKNOWN}$.
9. Else $\rightarrow \text{IDLE}$.

---

## 4. Data Sanitization & Whitelist Invariant

To satisfy the zero-leakage privacy requirement, the following schema boundary is strictly enforced across all domain entities and IPC boundaries:

```text
ALLOWED FIELDS:
  session_id, provider_id, provider_display_name, project_name,
  current_state, started_at, state_entered_at, last_seen_at, process_id,
  active_tool_category (enum string: [FileRead, FileWrite, ShellExecution, Search, ModelThinking]),
  adapter_status

EXCLUDED (PROHIBITED) FIELDS:
  prompts, queries, completions, code snippets, git diffs, file contents,
  shell arguments, command stdout/stderr, API keys, bearer tokens,
  user email addresses, environment variable maps.
```
