use std::io::Write;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::time::Duration;
use watchai_adapters::claude_code::ProviderTelemetryPayload;

use crate::telemetry_socket::resolve_telemetry_socket_path;

/// Dispatches a telemetry payload to the WatchAI daemon's Unix socket.
///
/// Designed to fail open and never panic or block if the daemon is offline.
pub fn send_telemetry_payload(payload: &ProviderTelemetryPayload) {
    let socket_path = resolve_telemetry_socket_path();
    if !socket_path.exists() {
        return;
    }

    if let Ok(stream) = UnixStream::connect(&socket_path) {
        let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
        let mut writer = stream;
        if let Ok(json_line) = serde_json::to_string(payload) {
            let _ = writeln!(writer, "{}", json_line);
        }
    }
}

/// Executes a child process with transparent standard I/O and telemetry tracking.
pub fn run_wrapper(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut provider_id = "opencode".to_string();
    let mut custom_cwd: Option<PathBuf> = None;
    let mut cmd_parts: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--provider" => {
                if i + 1 < args.len() {
                    provider_id = args[i + 1].clone();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--cwd" => {
                if i + 1 < args.len() {
                    custom_cwd = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--" => {
                cmd_parts.extend_from_slice(&args[i + 1..]);
                break;
            }
            other => {
                if !other.starts_with('-') {
                    cmd_parts.extend_from_slice(&args[i..]);
                    break;
                }
                i += 1;
            }
        }
    }

    if cmd_parts.is_empty() {
        eprintln!(
            "Usage: watchai-daemon wrap [--provider <id>] [--cwd <path>] -- <command> [args...]"
        );
        std::process::exit(1);
    }

    let command_name = &cmd_parts[0];
    let command_args = &cmd_parts[1..];

    let effective_cwd = custom_cwd
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let cwd_str = effective_cwd.to_string_lossy().to_string();

    let mut command = Command::new(command_name);
    command.args(command_args);
    command.current_dir(&effective_cwd);

    // Spawn child process with inherited standard streams
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "watchai-daemon wrap: failed to execute '{}': {}",
                command_name, e
            );
            std::process::exit(127);
        }
    };

    let child_pid = child.id();

    // 1. Emit Start telemetry event (Working state)
    let start_payload = ProviderTelemetryPayload {
        pid: child_pid,
        provider_id: Some(provider_id.clone()),
        claude_session_id: None,
        cwd: Some(cwd_str.clone()),
        hook_event: "UserPromptSubmit".to_string(),
        tool_name: None,
        exit_reason: None,
    };
    send_telemetry_payload(&start_payload);

    // 2. Wait for process completion
    let status: ExitStatus = child.wait().unwrap_or_else(|_| ExitStatusExt::from_raw(1));

    // 3. Emit Exit telemetry event (Success or Error state)
    let (hook_event, exit_reason) = if status.success() {
        ("Stop".to_string(), None)
    } else {
        ("StopFailure".to_string(), Some("error".to_string()))
    };

    let exit_payload = ProviderTelemetryPayload {
        pid: child_pid,
        provider_id: Some(provider_id),
        claude_session_id: None,
        cwd: Some(cwd_str),
        hook_event,
        tool_name: None,
        exit_reason,
    };
    send_telemetry_payload(&exit_payload);

    // 4. Exit with child process exit code
    if let Some(code) = status.code() {
        std::process::exit(code);
    } else if let Some(sig) = status.signal() {
        std::process::exit(128 + sig);
    } else {
        std::process::exit(1);
    }
}
