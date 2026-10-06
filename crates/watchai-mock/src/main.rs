//! WatchAI mock CLI simulator for testing and validation.

use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use watchai_core::session::SessionLifecycleEvent;
use watchai_mock::cli::{parse_args, Command, SessionCommand, WorkerCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let options = match parse_args(args) {
        Ok(opts) => opts,
        Err(e) => {
            if e != "Help requested" {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return Ok(());
        }
    };

    match options.command {
        Command::Session(session_cmd) => {
            execute_session_command(options.socket_path, session_cmd).await?;
        }
        Command::Worker(worker_cmd) => {
            execute_worker_command(worker_cmd).await?;
        }
    }

    Ok(())
}

async fn execute_session_command(
    socket_path_opt: Option<PathBuf>,
    cmd: SessionCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = socket_path_opt.ok_or(
        "No test socket specified. Pass --socket <path> or set WATCHAI_TEST_SOCKET environment variable.",
    )?;

    let now = Utc::now();
    let event = match cmd {
        SessionCommand::Start {
            id,
            provider,
            display_name,
            project,
            state,
            pid,
        } => SessionLifecycleEvent::SessionRegistered {
            session_id: id,
            provider_id: provider,
            provider_display_name: display_name,
            project_path: project,
            process_id: pid,
            initial_state: state,
            timestamp: now,
        },
        SessionCommand::Transition { id, state, tool } => SessionLifecycleEvent::StateTransition {
            session_id: id,
            new_state: state,
            tool_category: tool,
            timestamp: now,
        },
        SessionCommand::Heartbeat { id } => SessionLifecycleEvent::Heartbeat {
            session_id: id,
            timestamp: now,
        },
        SessionCommand::Terminate { id, exit_code } => SessionLifecycleEvent::SessionTerminated {
            session_id: id,
            exit_code,
            timestamp: now,
        },
    };

    let mut stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Failed to connect to test socket at {:?}: {}",
                socket_path, e
            );
            std::process::exit(1);
        }
    };

    let mut json = serde_json::to_string(&event)?;
    json.push('\n');

    stream.write_all(json.as_bytes()).await?;
    stream.flush().await?;

    println!(
        "Transmitted mock event successfully: {}",
        event.session_id()
    );
    Ok(())
}

async fn execute_worker_command(cmd: WorkerCommand) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        WorkerCommand::Run {
            provider,
            project,
            hold_seconds,
        } => {
            let script_name = match provider.to_lowercase().as_str() {
                "claude" | "claude-code" => "claude.py",
                "codex" | "codex-cli" => "codex.py",
                "opencode" => "opencode.py",
                _ => "claude.py",
            };

            fs::create_dir_all(&project)?;
            let script_path = project.join(script_name);

            // Sleeper script compatible with ProcessScanner script runner pattern
            let python_code = "import time, sys\ntry:\n    time.sleep(float(sys.argv[1]) if len(sys.argv) > 1 else 60.0)\nexcept:\n    pass\n";
            fs::write(&script_path, python_code)?;

            let child = std::process::Command::new("python3")
                .arg(&script_path)
                .arg(hold_seconds.to_string())
                .current_dir(&project)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?;

            let pid = child.id();
            if let Ok(proc_root) = std::env::var("WATCHAI_PROC_ROOT") {
                let target = format!("/proc/{}", pid);
                let link = std::path::Path::new(&proc_root).join(pid.to_string());
                let _ = std::os::unix::fs::symlink(target, link);
            }
            println!(
                "Worker spawned: pid={}, provider={}, project={:?}",
                pid, provider, project
            );
        }
        WorkerCommand::Kill { pid, signal } => {
            let sig = match signal.to_uppercase().as_str() {
                "SIGTERM" | "TERM" | "15" => nix::sys::signal::Signal::SIGTERM,
                "SIGKILL" | "KILL" | "9" => nix::sys::signal::Signal::SIGKILL,
                "SIGINT" | "INT" | "2" => nix::sys::signal::Signal::SIGINT,
                other => {
                    eprintln!("Unsupported signal '{}', defaulting to SIGKILL", other);
                    nix::sys::signal::Signal::SIGKILL
                }
            };

            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), sig)?;
            println!("Sent {} to PID {}", sig, pid);
        }
    }

    Ok(())
}
