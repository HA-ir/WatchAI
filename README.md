# WatchAI

**WatchAI** is an open-source, local-first Linux desktop application that monitors local AI coding agents (Claude Code, OpenAI Codex CLI, OpenCode) and displays their real-time activity and lifecycle states via a GNOME Shell top-bar indicator and popover menu.

---

## Features

- **Multi-Agent Monitoring**: Automatically detects active coding agent sessions across your local system.
- **Real-Time Lifecycle Tracking**: Displays agent states (`ACTIVE`, `THINKING`, `EXECUTING_TOOL`, `ATTENTION_REQUIRED`, `IDLE`, `COMPLETED`, `TERMINATED`, `FAILED`) in real time.
- **GNOME Shell Integration**: Native top-bar status icon with badge counter and interactive popover showing session details, duration, working directories, active tools, and direct focus actions.
- **Configurable Desktop Notifications**: Optional system notifications when agents require human attention (approval/input) or finish their work.
- **Zero-Leakage Privacy**: Operates 100% locally on your machine. Never collects, transmits, or logs user prompts, source code, git diffs, tool parameters, or API keys.

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

### 1. Configure the Build Directory
```bash
git clone https://github.com/HA-ir/WatchAI.git
cd WatchAI

meson setup build --prefix=$HOME/.local
```

### 2. Compile the Components
```bash
ninja -C build
```
*Note: Meson orchestrates Cargo behind the scenes to compile the production daemon (`watchai-daemon`) in release mode using an isolated target directory.*

### 3. Install
```bash
ninja -C build install
```

This installs:
- The production daemon binary to `$HOME/.local/bin/watchai-daemon`.
- The GNOME Shell extension to `$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/`.
- Pre-compiled GSettings schemas to `$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/schemas/`.
- The systemd user service unit to `$HOME/.local/share/systemd/user/watchai.service`.

---

## Starting the Service & Extension

### 1. Reload and Start the Daemon Service
Reload systemd user units and enable the background daemon service:
```bash
systemctl --user daemon-reload
systemctl --user enable --now watchai.service
```

Verify that the daemon is active and running:
```bash
systemctl --user status watchai.service
```

### 2. Enable the GNOME Shell Extension
Enable the indicator extension in GNOME Shell:
```bash
gnome-extensions enable watchai@gnome.org
```
*(If the extension is not immediately recognized, log out and log back in, or restart GNOME Shell in X11 via `Alt+F2` -> `r`).*

### 3. Shell `$PATH` Notice (Manual Terminal Execution)
The `watchai.service` systemd unit executes the daemon using its absolute path (`$HOME/.local/bin/watchai-daemon`) and does **not** depend on your shell `$PATH`.

However, if you wish to run `watchai-daemon` directly from your terminal, ensure `$HOME/.local/bin` is in your `$PATH`. Add the following to your `~/.bashrc` or `~/.zshrc` if not already present:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

---

## Configuration & Preferences

WatchAI stores user preferences in GSettings under the schema `org.gnome.shell.extensions.watchai`.

| Setting Key | Type | Default | Description |
| :--- | :---: | :---: | :--- |
| `notify-on-attention` | boolean | `true` | Send a desktop notification when an agent enters `ATTENTION_REQUIRED`. |
| `notify-on-completion` | boolean | `false` | Send a desktop notification when an agent enters `COMPLETED`. |
| `badge-counter-mode` | enum | `'total-active'` | Top-bar badge counter mode (`'total-active'` or `'attention-only'`). |
| `filter-threshold` | enum | `'all'` | Popover session filter threshold (`'all'`, `'active-only'`, or `'attention-only'`). |

### Modifying Settings via CLI

You can inspect and update preferences using `gsettings`:

```bash
# Set schema directory for user-local query
export GSETTINGS_SCHEMA_DIR="$HOME/.local/share/gnome-shell/extensions/watchai@gnome.org/schemas"

# Enable notifications on task completion
gsettings set org.gnome.shell.extensions.watchai notify-on-completion true

# Change badge counter to display attention-required sessions only
gsettings set org.gnome.shell.extensions.watchai badge-counter-mode 'attention-only'

# Filter popover to active sessions only
gsettings set org.gnome.shell.extensions.watchai filter-threshold 'active-only'

# Read current setting
gsettings get org.gnome.shell.extensions.watchai notify-on-attention
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
```

### Viewing Daemon Logs
Inspect daemon activity, discovery events, and liveness checks:
```bash
journalctl --user -u watchai.service -f
```

### Uninstallation

To cleanly remove all installed files:
```bash
# Stop and disable the service
systemctl --user disable --now watchai.service

# Disable extension
gnome-extensions disable watchai@gnome.org

# Uninstall installed files via Ninja
ninja -C build uninstall

# Reload systemd
systemctl --user daemon-reload
```

---

## Privacy & Security Guarantees

WatchAI adheres to a strict **Zero-Leakage Privacy Policy**:
1. **Passive Process Introspection**: Agent detection relies strictly on `/proc` process table scanning and local environment discovery.
2. **No Data Snooping**: WatchAI never reads, captures, or transmits user prompts, LLM conversation logs, source code diffs, command-line arguments containing tokens, or credentials.
3. **No External Network Calls**: The daemon and GNOME extension make zero outbound network connections. All IPC occurs locally via standard Linux D-Bus.

---

## License

WatchAI is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.
