use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::Duration;
use watchai_adapters::claude_code::ClaudeTelemetryPayload;

use crate::telemetry_socket::resolve_telemetry_socket_path;

/// Maximum bytes read from standard input by the forwarder (64 KiB).
pub const MAX_HOOK_STDIN_BYTES: u64 = 64 * 1024;

/// Executes the ultra-fast CLI hook forwarder sub-command.
///
/// Designed to execute in sub-millisecond time and fail open (exit 0) on any error
/// or missing daemon connection to ensure Claude Code is never blocked or disrupted.
pub fn run_hook_forwarder() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Check CLAUDE_PID from environment. If absent or invalid, exit 0 cleanly.
    let pid_str = match std::env::var("CLAUDE_PID") {
        Ok(val) => val,
        Err(_) => return Ok(()),
    };
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) if p > 0 => p,
        _ => return Ok(()),
    };

    // 2. Read stdin up to MAX_HOOK_STDIN_BYTES
    let mut stdin_buf = Vec::with_capacity(2048);
    let stdin = std::io::stdin();
    let mut handle = stdin.take(MAX_HOOK_STDIN_BYTES);
    if handle.read_to_end(&mut stdin_buf).is_err() || stdin_buf.is_empty() {
        return Ok(());
    }

    // 3. Parse JSON from Claude Code hook
    let val: serde_json::Value = match serde_json::from_slice(&stdin_buf) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    let hook_event = match val.get("hook_event_name").and_then(|v| v.as_str()) {
        Some(e) if !e.is_empty() => e.to_string(),
        _ => return Ok(()),
    };

    let claude_session_id = val
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let cwd = val
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| std::env::var("CLAUDE_PROJECT_DIR").ok())
        .or_else(|| std::env::var("PWD").ok());
    let tool_name = val
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let exit_reason = val
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let payload = ClaudeTelemetryPayload {
        pid,
        claude_session_id,
        cwd,
        hook_event,
        tool_name,
        exit_reason,
    };

    // 4. Connect to production socket with short timeout and write line
    let socket_path = resolve_telemetry_socket_path();
    if !socket_path.exists() {
        // Daemon not running; exit 0 without failing or blocking
        return Ok(());
    }

    if let Ok(stream) = UnixStream::connect(&socket_path) {
        stream
            .set_write_timeout(Some(Duration::from_millis(100)))
            .ok();
        let mut writer = stream;
        use std::io::Write;
        if let Ok(json_line) = serde_json::to_string(&payload) {
            let _ = writeln!(writer, "{}", json_line);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forwarder_returns_ok_when_claude_pid_unset() {
        std::env::remove_var("CLAUDE_PID");
        assert!(run_hook_forwarder().is_ok());
    }

    #[test]
    fn test_forwarder_returns_ok_when_daemon_socket_missing() {
        std::env::set_var("CLAUDE_PID", "12345");
        std::env::set_var(
            "WATCHAI_TELEMETRY_SOCKET",
            "/tmp/non_existent_watchai_socket_12345.sock",
        );
        assert!(run_hook_forwarder().is_ok());
        std::env::remove_var("CLAUDE_PID");
        std::env::remove_var("WATCHAI_TELEMETRY_SOCKET");
    }
}
