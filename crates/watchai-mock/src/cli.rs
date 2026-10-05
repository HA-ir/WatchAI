use std::path::PathBuf;
use std::str::FromStr;
use watchai_core::session::ToolCategory;
use watchai_core::state::LifecycleState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOptions {
    pub socket_path: Option<PathBuf>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Session(SessionCommand),
    Worker(WorkerCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    Start {
        id: String,
        provider: String,
        display_name: String,
        project: String,
        state: LifecycleState,
        pid: Option<u32>,
    },
    Transition {
        id: String,
        state: LifecycleState,
        tool: Option<ToolCategory>,
    },
    Heartbeat {
        id: String,
    },
    Terminate {
        id: String,
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerCommand {
    Run {
        provider: String,
        project: PathBuf,
        hold_seconds: u64,
    },
    Kill {
        pid: u32,
        signal: String,
    },
}

pub fn parse_tool_category(s: &str) -> Result<ToolCategory, String> {
    match s.trim().to_uppercase().as_str() {
        "BASH" | "SHELL" | "SHELL_EXECUTION" | "SHELLEXECUTION" => Ok(ToolCategory::ShellExecution),
        "READ" | "FILE_READ" | "FILEREAD" => Ok(ToolCategory::FileRead),
        "WRITE" | "FILE_WRITE" | "FILEWRITE" => Ok(ToolCategory::FileWrite),
        "SEARCH" => Ok(ToolCategory::Search),
        "THINKING" | "MODEL_THINKING" | "MODELTHINKING" => Ok(ToolCategory::ModelThinking),
        other => Err(format!("Unrecognized tool category: '{other}'")),
    }
}

pub fn normalize_provider(p: &str) -> (String, String) {
    match p.trim().to_lowercase().as_str() {
        "claude" | "claude-code" => ("claude-code".to_string(), "Claude Code".to_string()),
        "codex" | "codex-cli" => ("codex-cli".to_string(), "Codex CLI".to_string()),
        "opencode" => ("opencode".to_string(), "OpenCode".to_string()),
        other => (other.to_string(), other.to_string()),
    }
}

pub fn print_help() {
    println!(
        r#"watchai-mock - Deterministic simulation CLI for WatchAI E2E verification

USAGE:
    watchai-mock [OPTIONS] <SUBCOMMAND>

SUBCOMMANDS:
    session start       Register a new mock session with initial state
    session transition  Transition an existing session to a new state
    session heartbeat   Emit a heartbeat confirmation for a session
    session terminate   Terminate a session with an optional exit code
    worker run          Spawn a discovery-compliant sleeper worker process
    worker kill         Terminate a worker process with SIGTERM or SIGKILL

OPTIONS:
    --socket <PATH>     Path to WATCHAI_TEST_SOCKET (defaults to env var)
    -h, --help          Print this help message
"#
    );
}

pub fn parse_args(args: Vec<String>) -> Result<CliOptions, String> {
    let mut socket_path = std::env::var("WATCHAI_TEST_SOCKET").ok().map(PathBuf::from);

    let mut filtered_args = Vec::new();
    let mut iter = args.into_iter();

    // Skip argv[0] if provided
    let first = iter.next();
    if let Some(f) = first {
        if !f.starts_with('-') && (f.ends_with("watchai-mock") || f.ends_with("main")) {
            // Program name skipped
        } else {
            filtered_args.push(f);
        }
    }

    while let Some(arg) = iter.next() {
        if arg == "--socket" {
            let val = iter.next().ok_or("Missing value for --socket")?;
            socket_path = Some(PathBuf::from(val));
        } else if let Some(stripped) = arg.strip_prefix("--socket=") {
            socket_path = Some(PathBuf::from(stripped));
        } else {
            filtered_args.push(arg);
        }
    }

    if filtered_args.is_empty()
        || filtered_args
            .iter()
            .any(|a| a == "-h" || a == "--help" || a == "help")
    {
        print_help();
        return Err("Help requested".to_string());
    }

    let subcmd = &filtered_args[0];
    match subcmd.as_str() {
        "session" => {
            if filtered_args.len() < 2 {
                return Err(
                    "Missing session action: start, transition, heartbeat, terminate".to_string(),
                );
            }
            let action = &filtered_args[1];
            let action_args = &filtered_args[2..];
            let cmd = parse_session_command(action, action_args)?;
            Ok(CliOptions {
                socket_path,
                command: Command::Session(cmd),
            })
        }
        "worker" => {
            if filtered_args.len() < 2 {
                return Err("Missing worker action: run, kill".to_string());
            }
            let action = &filtered_args[1];
            let action_args = &filtered_args[2..];
            let cmd = parse_worker_command(action, action_args)?;
            Ok(CliOptions {
                socket_path,
                command: Command::Worker(cmd),
            })
        }
        other => Err(format!("Unrecognized command: '{other}'")),
    }
}

fn parse_session_command(action: &str, args: &[String]) -> Result<SessionCommand, String> {
    let mut id: Option<String> = None;
    let mut provider = "claude-code".to_string();
    let mut display_name: Option<String> = None;
    let mut project: Option<String> = None;
    let mut state: Option<LifecycleState> = None;
    let mut pid: Option<u32> = None;
    let mut tool: Option<ToolCategory> = None;
    let mut exit_code: Option<i32> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--id" => {
                i += 1;
                id = Some(args.get(i).ok_or("Missing value for --id")?.clone());
            }
            "--provider" => {
                i += 1;
                provider = args.get(i).ok_or("Missing value for --provider")?.clone();
            }
            "--display-name" => {
                i += 1;
                display_name = Some(
                    args.get(i)
                        .ok_or("Missing value for --display-name")?
                        .clone(),
                );
            }
            "--project" => {
                i += 1;
                project = Some(args.get(i).ok_or("Missing value for --project")?.clone());
            }
            "--state" => {
                i += 1;
                let s_str = args.get(i).ok_or("Missing value for --state")?;
                state = Some(LifecycleState::from_str(s_str)?);
            }
            "--pid" => {
                i += 1;
                let p_str = args.get(i).ok_or("Missing value for --pid")?;
                pid = Some(
                    p_str
                        .parse()
                        .map_err(|e| format!("Invalid PID '{p_str}': {e}"))?,
                );
            }
            "--tool" => {
                i += 1;
                let t_str = args.get(i).ok_or("Missing value for --tool")?;
                tool = Some(parse_tool_category(t_str)?);
            }
            "--exit-code" => {
                i += 1;
                let c_str = args.get(i).ok_or("Missing value for --exit-code")?;
                exit_code = Some(
                    c_str
                        .parse()
                        .map_err(|e| format!("Invalid exit code '{c_str}': {e}"))?,
                );
            }
            other => return Err(format!("Unrecognized option: '{other}'")),
        }
        i += 1;
    }

    match action {
        "start" => {
            let session_id = id.ok_or("session start requires --id")?;
            let (canonical_p, default_name) = normalize_provider(&provider);
            let final_display = display_name.unwrap_or(default_name);
            let project_path = project.unwrap_or_else(|| "/tmp/mock-project".to_string());
            let initial_state = state.unwrap_or(LifecycleState::Starting);

            Ok(SessionCommand::Start {
                id: session_id,
                provider: canonical_p,
                display_name: final_display,
                project: project_path,
                state: initial_state,
                pid,
            })
        }
        "transition" => {
            let session_id = id.ok_or("session transition requires --id")?;
            let target_state = state.ok_or("session transition requires --state")?;
            Ok(SessionCommand::Transition {
                id: session_id,
                state: target_state,
                tool,
            })
        }
        "heartbeat" => {
            let session_id = id.ok_or("session heartbeat requires --id")?;
            Ok(SessionCommand::Heartbeat { id: session_id })
        }
        "terminate" => {
            let session_id = id.ok_or("session terminate requires --id")?;
            Ok(SessionCommand::Terminate {
                id: session_id,
                exit_code,
            })
        }
        other => Err(format!("Unrecognized session action: '{other}'")),
    }
}

fn parse_worker_command(action: &str, args: &[String]) -> Result<WorkerCommand, String> {
    let mut provider = "claude-code".to_string();
    let mut project = PathBuf::from("/tmp/mock-worker");
    let mut hold_seconds = 60;
    let mut pid: Option<u32> = None;
    let mut signal = "SIGKILL".to_string();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--provider" => {
                i += 1;
                provider = args.get(i).ok_or("Missing value for --provider")?.clone();
            }
            "--project" => {
                i += 1;
                project = PathBuf::from(args.get(i).ok_or("Missing value for --project")?);
            }
            "--hold-seconds" => {
                i += 1;
                let s_str = args.get(i).ok_or("Missing value for --hold-seconds")?;
                hold_seconds = s_str
                    .parse()
                    .map_err(|e| format!("Invalid hold seconds: {e}"))?;
            }
            "--pid" => {
                i += 1;
                let p_str = args.get(i).ok_or("Missing value for --pid")?;
                pid = Some(p_str.parse().map_err(|e| format!("Invalid PID: {e}"))?);
            }
            "--signal" => {
                i += 1;
                signal = args
                    .get(i)
                    .ok_or("Missing value for --signal")?
                    .to_uppercase();
            }
            other => return Err(format!("Unrecognized worker option: '{other}'")),
        }
        i += 1;
    }

    match action {
        "run" => Ok(WorkerCommand::Run {
            provider,
            project,
            hold_seconds,
        }),
        "kill" => {
            let target_pid = pid.ok_or("worker kill requires --pid")?;
            Ok(WorkerCommand::Kill {
                pid: target_pid,
                signal,
            })
        }
        other => Err(format!("Unrecognized worker action: '{other}'")),
    }
}
