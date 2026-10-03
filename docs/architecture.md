# WatchAI Architecture Overview

WatchAI is a Linux desktop system designed to monitor local AI coding agents (Claude Code, OpenAI Codex CLI, OpenCode, and others) and present their real-time lifecycle states in the GNOME Shell top bar.

## Four-Tier Layered Architecture

1. **Provider Adapters (`crates/watchai-adapters`)**:
   - Detects active local agent sessions via non-invasive `/proc` scanning and opt-in event hooks.
   - Normalizes vendor-specific telemetry into generic `SessionLifecycleEvent` records.
   - Strictly enforces data sanitization: no prompts, code, tokens, or credentials enter the core domain.
   - Decoupled from the daemon via `AdapterRegistry`.

2. **Core Domain & State Machine (`crates/watchai-core`)**:
   - Implements the 8-state deterministic finite-state machine (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`).
   - Resolves multi-session priority aggregation ($\text{ERROR} > \text{WAITING} > \text{WORKING} > \dots$).
   - Performs hybrid process liveness tracking (`/proc` PID checks + adaptive silence timeouts).
   - Manages in-memory volatile session storage (zero session data on disk).

3. **Local IPC Layer (`crates/watchai-ipc`)**:
   - Exposes the `org.freedesktop.WatchAI` interface on the D-Bus user session bus.
   - Provides methods for aggregate and individual session inspection.
   - Broadcasts asynchronous signals (`AggregateStateChanged`, `SessionAdded`, `SessionUpdated`, `SessionRemoved`).

4. **GNOME Shell Extension (`extension/`)**:
   - ESM JavaScript extension for GNOME Shell (45, 46, 47).
   - Top-bar indicator button with symbolic icons, color styles, and AT-SPI accessibility descriptions.
   - Interactive popover menu detailing active sessions.
   - Strictly non-blocking: communicates with the daemon via asynchronous `Gio.DBusProxy`.
