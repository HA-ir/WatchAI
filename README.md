# WatchAI

**WatchAI** is an open-source, local-first Linux desktop application that monitors local AI coding agents (Claude Code, OpenAI Codex CLI, OpenCode) and displays their real-time activity and lifecycle states via a GNOME Shell top-bar indicator and popover menu.

---

## Features

- **Multi-Agent Monitoring**: Automatically detects active coding agent sessions across your local system.
- **Multi-Provider Telemetry**: Native lifecycle hooks and transparent execution wrapping (`watchai-daemon wrap`) for Claude Code, OpenCode, and OpenAI Codex CLI.
- **Real-Time Lifecycle Tracking**: Displays canonical agent states (`IDLE`, `STARTING`, `WORKING`, `WAITING`, `SUCCESS`, `ERROR`, `CANCELLED`, `UNKNOWN`) in real time with a 60-second completion dwell duration.
- **GNOME Shell Integration**: Native top-bar status icon with multi-session counter and interactive popover displaying active sessions, duration, sanitized workspace project names, PIDs, and active tool categories.
- **Configurable Desktop Notifications**: Native desktop notifications when agents enter `WAITING` (for approval or input) or `ERROR` (crashed or failed), with strict 5-second per-session cooldown and zero-leakage privacy.
- **Zero-Leakage Privacy**: Operates 100% locally on your machine. Never collects, transmits, or logs user prompts, source code, git diffs, tool parameters, or API keys.
- **EGO Distribution Packaging**: Built-in support for generating compliant `.shell-extension.zip` bundles for extensions.gnome.org (`ninja -C build pack-extension` or `./install.sh --pack`).
- **Continuous Integration**: Automated CI pipeline running unit tests, Clippy, GJS test suites, D-Bus session integration tests, and packaging verification on GitHub Actions.

---

## Agent Integration & Telemetry

WatchAI supports three methods to monitor AI coding agents. See the comprehensive [Agent Integration & Telemetry Guide](docs/agent-integration-guide.md) for full architectural details and event specifications.

### 1. Claude Code CLI (`claude`) — Native Hooks
Claude Code features built-in JSON lifecycle hooks. WatchAI can automatically register with your Claude settings:
```bash
# Register activity hooks in ~/.claude/settings.json
watchai-daemon install-hooks
```
*(To uninstall: `watchai-daemon uninstall-hooks`)*

### 2. OpenCode & OpenAI Codex CLI — Transparent Execution Wrapper (⭐ Recommended)
For OpenCode and Codex CLI, the **most accurate, 0ms-latency approach** is the transparent execution wrapper. It immediately catches command execution, maps active turns to `WORKING`, and applies the full **60-second green `SUCCESS` dwell** on clean exit (or red `ERROR` on failure).

Add these aliases to your `~/.bashrc` or `~/.zshrc`:
```bash
# OpenCode alias
alias opencode="watchai-daemon wrap --provider opencode -- opencode"

# OpenAI Codex CLI alias
alias codex="watchai-daemon wrap --provider codex-cli -- codex"
```

### 3. Passive `/proc` Discovery (Zero Configuration)
If you prefer not to configure aliases or hooks, WatchAI automatically scans `/proc` every 2 seconds for running `claude`, `opencode`, and `codex` instances, tracking PIDs and active workspaces in `IDLE` state out of the box.

---

## Supported Environments

- **Operating System**: Linux (Kernel 5.8 or later recommended for `/proc` monitoring).
- **Desktop Environment**: GNOME Shell 45, 46, and 47 (Wayland and X11 sessions).
- **Session Architecture**: `systemd` user manager (`systemd --user`) and D-Bus session bus.
- **Supported AI Agent Providers**:
  - **Claude Code CLI** (`claude`)
  - **OpenAI Codex CLI** (`codex`)
  - **OpenCode** (`opencode`)

---

## Prerequisites & Dependencies

To build and run WatchAI from source, ensure the following packages are installed:

### Debian / Ubuntu (24.04+)
```bash
sudo apt update
sudo apt install meson ninja-build cargo rustc libglib2.0-dev libglib2.0-bin gnome-shell
```

### Fedora (40+)
```bash
sudo dnf install meson ninja-build cargo rust glib2-devel gnome-shell
```

### Arch Linux
```bash
sudo pacman -S meson ninja cargo rust glib2 gnome-shell
```

---

## Building and Installation

WatchAI is designed to install into your unprivileged user directory (`$HOME/.local`) without requiring `sudo` or root privileges.

### Quick Install (Recommended)

Run the included local installer to configure, compile, install, reload systemd, and activate the GNOME Shell extension in one command:

```bash
git clone https://github.com/HA-ir/WatchAI.git
cd WatchAI

./install.sh
```

To test the installation steps beforehand without modifying your system, run:
```bash
./install.sh --dry-run
```

---

### Manual Installation (Advanced)

If you prefer to drive Meson and systemd manually:

#### 1. Configure the Build Directory
```bash
meson setup build --prefix=$HOME/.local
```

#### 2. Compile the Components
```bash
ninja -C build
```
*Note: Meson orchestrates Cargo behind the scenes to compile the production daemon (`watchai-daemon`) in release mode using an isolated target directory (`<builddir>/cargo-target`), leaving the repository source tree untouched.*

#### 3. Install
```bash
ninja -C build install
```

This installs:
- The production daemon binary to `$HOME/.local/bin/watchai-daemon`.
- The GNOME Shell extension to `$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/`.
- Pre-compiled GSettings schemas to `$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/schemas/`.
- System GSettings schemas to `$HOME/.local/share/glib-2.0/schemas/`.
- The systemd user service unit to `$HOME/.local/share/systemd/user/watchai.service`.

#### 4. Reload and Start the Daemon Service
```bash
systemctl --user daemon-reload
systemctl --user enable --now watchai.service
```

#### 5. Enable the GNOME Shell Extension
```bash
gnome-extensions enable watchai@gnome.org
```

### 3. Shell `$PATH` Notice (Manual Terminal Execution)
The `watchai.service` systemd unit executes the daemon using its absolute path (`$HOME/.local/bin/watchai-daemon`) and does **not** depend on your shell `$PATH`.

However, if you wish to run `watchai-daemon` directly from your terminal, ensure `$HOME/.local/bin` is in your `$PATH`. Add the following to your `~/.bashrc` or `~/.zshrc` if not already present:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

---

### Extension Packaging (extensions.gnome.org)

To generate a distribution bundle ready for upload to [extensions.gnome.org (EGO)](https://extensions.gnome.org/):

```bash
# Using the installer
./install.sh --pack

# Or using Ninja
ninja -C build pack-extension
```

This compiles GSettings schemas and produces an audited distribution archive at `build/watchai@gnome.org.shell-extension.zip`.

---

## Configuration & Preferences

WatchAI stores user preferences in GSettings under the schema `org.gnome.shell.extensions.watchai`.

| Setting Key | Type | Default | Description |
| :--- | :---: | :---: | :--- |
| `dwell-duration-seconds` | uint32 | `60` | Extension-exposed completion dwell preference in seconds (range 1–60, default: 60). Note: The background daemon/core remains authoritative for actual lifecycle dwell timing. |
| `enable-desktop-notifications` | boolean | `true` | Master switch to enable or disable desktop notifications for agent lifecycle transitions. |
| `notify-on-waiting` | boolean | `true` | Dispatch a desktop notification when an agent enters `WAITING` requiring user interaction or approval. |
| `notify-on-error` | boolean | `true` | Dispatch a desktop notification when an agent enters `ERROR` or crashes. |
| `indicator-icon-style` | string enum | `'symbolic'` | Top-bar indicator visual style: `'symbolic'` (monochrome desktop theme) or `'colored'` (state-colored accents). |

### Notification Behavior & Privacy
WatchAI notifications follow strict runtime rules implemented in `extension/notifications.js`:
- **Edge-Triggered Only**: Notifications are dispatched strictly when a session transitions *into* `WAITING` or `ERROR`.
- **Heartbeat & Duplicate Suppression**: Repeated updates or heartbeats in the same state never trigger notifications.
- **Per-Session Cooldown**: A strict 5.0-second cooldown is enforced per session. Events occurring inside the cooldown window are dropped immediately with no queuing and no replaying.
- **Zero-Leakage Privacy**: Notification bodies use generic static text (`"Agent is waiting for user input or approval."` or `"Agent encountered an error or crashed."`). Notification titles display only the sanitized project folder basename (`WatchAI: <ProviderName> (<ProjectName>)`). Prompts, tool arguments, code, secrets, and full directory paths are never included.

### Modifying Settings via CLI

You can inspect and update preferences using `gsettings`:

```bash
# Point to schema directory for user-local terminal queries
export GSETTINGS_SCHEMA_DIR="$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/schemas"

# Inspect current dwell duration
gsettings get org.gnome.shell.extensions.watchai dwell-duration-seconds

# Adjust dwell duration to 15 seconds
gsettings set org.gnome.shell.extensions.watchai dwell-duration-seconds 15

# Switch indicator to colored accent style
gsettings set org.gnome.shell.extensions.watchai indicator-icon-style 'colored'

# Disable notifications on error
gsettings set org.gnome.shell.extensions.watchai notify-on-error false

# Master notification toggle
gsettings set org.gnome.shell.extensions.watchai enable-desktop-notifications false
```

---

## Verification & Troubleshooting

### Inspecting D-Bus Communication
WatchAI exposes its status on the user session bus at `org.freedesktop.WatchAI`:

```bash
# Introspect available D-Bus methods and properties
busctl --user introspect org.freedesktop.WatchAI /org/freedesktop/WatchAI

# Query active sessions directly
busctl --user call org.freedesktop.WatchAI /org/freedesktop/WatchAI org.freedesktop.WatchAI GetSessions

# Query aggregate state directly
busctl --user call org.freedesktop.WatchAI /org/freedesktop/WatchAI org.freedesktop.WatchAI GetAggregateState
```

### Viewing Daemon Logs
Inspect daemon activity, discovery events, and liveness checks:
```bash
journalctl --user -u watchai.service -f
```

### Uninstallation

To cleanly remove all installed files, run:
```bash
./uninstall.sh
```
*(Alternatively, you can run `./install.sh --uninstall` or `ninja -C build uninstall`).*

---

## Privacy & Security Guarantees

WatchAI adheres to a strict **Zero-Leakage Privacy Policy**:
1. **Passive Process Introspection**: Agent detection relies strictly on `/proc` process table scanning and local environment discovery.
2. **No Data Snooping**: WatchAI never reads, captures, or transmits user prompts, LLM conversation logs, source code diffs, command-line arguments containing tokens, or credentials.
3. **No External Network Calls**: The daemon and GNOME extension make zero outbound network connections. All IPC occurs locally via standard Linux D-Bus.

---

## License

WatchAI is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.
