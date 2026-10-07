use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

pub const HOOK_EVENTS_TO_REGISTER: &[&str] = &[
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Returns the standard default user settings path for Claude Code: `~/.claude/settings.json`.
pub fn default_claude_settings_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".claude").join("settings.json")
    } else {
        PathBuf::from(".claude").join("settings.json")
    }
}

/// Returns the default path to the installed watchai-daemon binary: `~/.local/bin/watchai-daemon`.
pub fn default_watchai_daemon_bin() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("bin")
            .join("watchai-daemon")
    } else {
        PathBuf::from("watchai-daemon")
    }
}

/// Summary report of hook installation operation.
#[derive(Debug, PartialEq, Eq)]
pub struct HookInstallReport {
    pub backup_path: Option<PathBuf>,
    pub events_registered: Vec<String>,
    pub events_already_present: Vec<String>,
}

/// Summary report of hook uninstallation operation.
#[derive(Debug, PartialEq, Eq)]
pub struct HookUninstallReport {
    pub backup_path: Option<PathBuf>,
    pub events_removed: Vec<String>,
}

/// Check if a specific matcher entry in Claude settings points to watchai-daemon hook.
fn is_watchai_hook_entry(matcher_entry: &serde_json::Value, daemon_bin_str: &str) -> bool {
    if let Some(hooks_arr) = matcher_entry.get("hooks").and_then(|h| h.as_array()) {
        for hook_obj in hooks_arr {
            let cmd = hook_obj
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let args = hook_obj
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|x| x.as_str()).collect::<Vec<&str>>())
                .unwrap_or_default();

            if (cmd == daemon_bin_str
                || cmd.ends_with("/watchai-daemon")
                || cmd == "watchai-daemon")
                && args.contains(&"hook")
            {
                return true;
            }
        }
    }
    false
}

/// Safely and idempotently merges WatchAI activity telemetry hooks into Claude Code settings.
/// Creates a timestamped backup before modifying. Preserves all other user settings and hooks.
pub fn install_claude_hooks(
    settings_path: &Path,
    daemon_bin_path: &Path,
) -> Result<HookInstallReport, Box<dyn std::error::Error>> {
    let daemon_bin_str = daemon_bin_path.to_string_lossy().to_string();

    // 1. Read existing settings or initialize empty structure
    let mut root: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(settings_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }
        serde_json::json!({})
    };

    if !root.is_object() {
        root = serde_json::json!({});
    }

    // 2. Create timestamped backup if file exists
    let backup_path = if settings_path.exists() {
        let timestamp = chrono::Utc::now().timestamp();
        let bpath = settings_path.with_extension(format!("json.bak.{}", timestamp));
        fs::copy(settings_path, &bpath)?;
        info!("Created backup of Claude settings at {:?}", bpath);
        Some(bpath)
    } else {
        None
    };

    // 3. Ensure "hooks" object exists
    let hooks_obj = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("Settings 'hooks' entry is not a JSON object")?;

    let mut registered = Vec::new();
    let mut already_present = Vec::new();

    // 4. Register each target hook event idempotently
    for &event_name in HOOK_EVENTS_TO_REGISTER {
        let matchers_arr = hooks_obj
            .entry(event_name)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("Hooks entry for '{}' is not a JSON array", event_name))?;

        let existing_entry = matchers_arr
            .iter_mut()
            .find(|m| is_watchai_hook_entry(m, &daemon_bin_str));

        if let Some(entry) = existing_entry {
            // Claude Code documentation: "The matcher is a string: ... or empty to match all."
            // Ensure matcher is empty string so events with empty queries (Stop, UserPromptSubmit, SessionEnd) match.
            if entry.get("matcher").and_then(|m| m.as_str()) != Some("") {
                entry["matcher"] = serde_json::json!("");
            }
            already_present.push(event_name.to_string());
        } else {
            let new_hook = serde_json::json!({
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": daemon_bin_str,
                        "args": ["hook"],
                        "timeout": 5
                    }
                ]
            });
            matchers_arr.push(new_hook);
            registered.push(event_name.to_string());
        }
    }

    // 5. Atomic write: write to temp file then rename
    let pretty = serde_json::to_string_pretty(&root)?;
    let tmp_file = settings_path.with_extension("tmp");
    fs::write(&tmp_file, pretty)?;
    fs::rename(&tmp_file, settings_path)?;

    Ok(HookInstallReport {
        backup_path,
        events_registered: registered,
        events_already_present: already_present,
    })
}

/// Safely removes WatchAI activity telemetry hooks from Claude Code settings.
/// Creates a timestamped backup before modifying. Preserves all other user settings and hooks.
pub fn uninstall_claude_hooks(
    settings_path: &Path,
    daemon_bin_path: &Path,
) -> Result<HookUninstallReport, Box<dyn std::error::Error>> {
    if !settings_path.exists() {
        return Ok(HookUninstallReport {
            backup_path: None,
            events_removed: Vec::new(),
        });
    }

    let daemon_bin_str = daemon_bin_path.to_string_lossy().to_string();
    let content = fs::read_to_string(settings_path)?;
    let mut root: serde_json::Value = serde_json::from_str(&content)?;

    let backup_path = {
        let timestamp = chrono::Utc::now().timestamp();
        let bpath = settings_path.with_extension(format!("json.bak.{}", timestamp));
        fs::copy(settings_path, &bpath)?;
        info!("Created backup of Claude settings at {:?}", bpath);
        Some(bpath)
    };

    let mut removed = Vec::new();

    if let Some(hooks_obj) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for &event_name in HOOK_EVENTS_TO_REGISTER {
            if let Some(matchers_arr) = hooks_obj.get_mut(event_name).and_then(|m| m.as_array_mut())
            {
                let initial_len = matchers_arr.len();
                matchers_arr.retain(|m| !is_watchai_hook_entry(m, &daemon_bin_str));
                if matchers_arr.len() < initial_len {
                    removed.push(event_name.to_string());
                }
            }
        }
    }

    let pretty = serde_json::to_string_pretty(&root)?;
    let tmp_file = settings_path.with_extension("tmp");
    fs::write(&tmp_file, pretty)?;
    fs::rename(&tmp_file, settings_path)?;

    Ok(HookUninstallReport {
        backup_path,
        events_removed: removed,
    })
}

/// Reports whether WatchAI activity telemetry hooks are currently registered in Claude settings.
pub fn status_claude_hooks(settings_path: &Path, daemon_bin_path: &Path) -> Vec<(String, bool)> {
    let daemon_bin_str = daemon_bin_path.to_string_lossy().to_string();
    if !settings_path.exists() {
        return HOOK_EVENTS_TO_REGISTER
            .iter()
            .map(|&e| (e.to_string(), false))
            .collect();
    }

    let content = match fs::read_to_string(settings_path) {
        Ok(c) => c,
        Err(_) => {
            return HOOK_EVENTS_TO_REGISTER
                .iter()
                .map(|&e| (e.to_string(), false))
                .collect();
        }
    };

    let root: serde_json::Value = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => {
            return HOOK_EVENTS_TO_REGISTER
                .iter()
                .map(|&e| (e.to_string(), false))
                .collect();
        }
    };

    let mut statuses = Vec::new();
    let hooks_obj = root.get("hooks").and_then(|h| h.as_object());

    for &event_name in HOOK_EVENTS_TO_REGISTER {
        let is_present = hooks_obj
            .and_then(|h| h.get(event_name))
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|m| is_watchai_hook_entry(m, &daemon_bin_str))
            })
            .unwrap_or(false);
        statuses.push((event_name.to_string(), is_present));
    }

    statuses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_and_uninstall_hooks_idempotency_and_preservation() {
        let temp_dir =
            std::env::temp_dir().join(format!("watchai-hook-install-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let settings_file = temp_dir.join("settings.json");
        let daemon_bin = PathBuf::from("/home/user/.local/bin/watchai-daemon");

        // Initial settings with an existing unrelated hook
        let initial_json = serde_json::json!({
            "model": "test-model",
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "CustomTool",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "/usr/bin/custom-script"
                            }
                        ]
                    }
                ]
            }
        });
        fs::write(
            &settings_file,
            serde_json::to_string_pretty(&initial_json).unwrap(),
        )
        .unwrap();

        // 1. Install hooks
        let report = install_claude_hooks(&settings_file, &daemon_bin).unwrap();
        assert_eq!(report.events_registered.len(), 8);
        assert_eq!(report.events_already_present.len(), 0);

        // Verify status
        let statuses = status_claude_hooks(&settings_file, &daemon_bin);
        assert_eq!(statuses.len(), 8);
        for (_event, present) in statuses {
            assert!(present);
        }

        // 2. Re-install (idempotency check)
        let report_re = install_claude_hooks(&settings_file, &daemon_bin).unwrap();
        assert_eq!(report_re.events_registered.len(), 0);
        assert_eq!(report_re.events_already_present.len(), 8);

        // Verify custom hook was preserved!
        let current: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_file).unwrap()).unwrap();
        let pre_tool = current["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre_tool.len(), 2); // 1 custom + 1 watchai
        assert_eq!(pre_tool[0]["matcher"], "CustomTool");

        // 3. Uninstall hooks
        let un_report = uninstall_claude_hooks(&settings_file, &daemon_bin).unwrap();
        assert_eq!(un_report.events_removed.len(), 8);

        // Verify custom hook is still preserved!
        let after_un: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_file).unwrap()).unwrap();
        let pre_tool_after = after_un["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre_tool_after.len(), 1);
        assert_eq!(pre_tool_after[0]["matcher"], "CustomTool");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
