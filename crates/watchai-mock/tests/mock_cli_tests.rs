use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;
use watchai_core::session::{SessionLifecycleEvent, ToolCategory};
use watchai_core::state::LifecycleState;
use watchai_mock::cli::{parse_args, Command, SessionCommand, WorkerCommand};

fn unique_test_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("watchai-{}-{}", prefix, nanos))
}

#[test]
fn test_cli_parsing_session_commands() {
    // 1. session start
    let args = vec![
        "watchai-mock".to_string(),
        "session".to_string(),
        "start".to_string(),
        "--id".to_string(),
        "sess-1".to_string(),
        "--provider".to_string(),
        "claude-code".to_string(),
        "--project".to_string(),
        "/tmp/project1".to_string(),
        "--state".to_string(),
        "Starting".to_string(),
    ];
    let opts = parse_args(args).expect("Parsing must succeed");
    assert_eq!(
        opts.command,
        Command::Session(SessionCommand::Start {
            id: "sess-1".to_string(),
            provider: "claude-code".to_string(),
            display_name: "Claude Code".to_string(),
            project: "/tmp/project1".to_string(),
            state: LifecycleState::Starting,
            pid: None,
        })
    );

    // 2. session transition with tool category
    let args2 = vec![
        "watchai-mock".to_string(),
        "session".to_string(),
        "transition".to_string(),
        "--id".to_string(),
        "sess-1".to_string(),
        "--state".to_string(),
        "Waiting".to_string(),
        "--tool".to_string(),
        "ShellExecution".to_string(),
    ];
    let opts2 = parse_args(args2).expect("Parsing must succeed");
    assert_eq!(
        opts2.command,
        Command::Session(SessionCommand::Transition {
            id: "sess-1".to_string(),
            state: LifecycleState::Waiting,
            tool: Some(ToolCategory::ShellExecution),
        })
    );

    // 3. session heartbeat
    let args3 = vec![
        "watchai-mock".to_string(),
        "session".to_string(),
        "heartbeat".to_string(),
        "--id".to_string(),
        "sess-1".to_string(),
    ];
    let opts3 = parse_args(args3).expect("Parsing must succeed");
    assert_eq!(
        opts3.command,
        Command::Session(SessionCommand::Heartbeat {
            id: "sess-1".to_string(),
        })
    );

    // 4. session terminate
    let args4 = vec![
        "watchai-mock".to_string(),
        "session".to_string(),
        "terminate".to_string(),
        "--id".to_string(),
        "sess-1".to_string(),
        "--exit-code".to_string(),
        "0".to_string(),
    ];
    let opts4 = parse_args(args4).expect("Parsing must succeed");
    assert_eq!(
        opts4.command,
        Command::Session(SessionCommand::Terminate {
            id: "sess-1".to_string(),
            exit_code: Some(0),
        })
    );
}

#[test]
fn test_cli_parsing_worker_commands() {
    let args = vec![
        "watchai-mock".to_string(),
        "worker".to_string(),
        "run".to_string(),
        "--provider".to_string(),
        "claude-code".to_string(),
        "--project".to_string(),
        "/tmp/work1".to_string(),
        "--hold-seconds".to_string(),
        "30".to_string(),
    ];
    let opts = parse_args(args).expect("Parsing worker run must succeed");
    assert_eq!(
        opts.command,
        Command::Worker(WorkerCommand::Run {
            provider: "claude-code".to_string(),
            project: PathBuf::from("/tmp/work1"),
            hold_seconds: 30,
        })
    );

    let args_kill = vec![
        "watchai-mock".to_string(),
        "worker".to_string(),
        "kill".to_string(),
        "--pid".to_string(),
        "9999".to_string(),
        "--signal".to_string(),
        "SIGTERM".to_string(),
    ];
    let opts_kill = parse_args(args_kill).expect("Parsing worker kill must succeed");
    assert_eq!(
        opts_kill.command,
        Command::Worker(WorkerCommand::Kill {
            pid: 9999,
            signal: "SIGTERM".to_string(),
        })
    );
}

#[tokio::test]
async fn test_mock_socket_transmission_and_serialization() {
    let socket_path = unique_test_path("mock-socket-test");
    let listener = UnixListener::bind(&socket_path).expect("Failed to bind test socket");

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        line
    });

    // Simulate mock CLI sending session transition
    std::env::set_var("WATCHAI_TEST_SOCKET", socket_path.to_str().unwrap());

    let (mut client_stream, _) = (
        tokio::net::UnixStream::connect(&socket_path).await.unwrap(),
        (),
    );
    let event = SessionLifecycleEvent::StateTransition {
        session_id: "mock-sess-1".to_string(),
        new_state: LifecycleState::Working,
        tool_category: Some(ToolCategory::FileRead),
        timestamp: chrono::Utc::now(),
    };
    let mut json = serde_json::to_string(&event).unwrap();
    json.push('\n');
    tokio::io::AsyncWriteExt::write_all(&mut client_stream, json.as_bytes())
        .await
        .unwrap();

    let received_line = tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("Server timeout")
        .expect("Server join error");

    let parsed_event: SessionLifecycleEvent =
        serde_json::from_str(&received_line).expect("Must deserialize to SessionLifecycleEvent");
    assert_eq!(parsed_event.session_id(), "mock-sess-1");
    if let SessionLifecycleEvent::StateTransition {
        new_state,
        tool_category,
        ..
    } = parsed_event
    {
        assert_eq!(new_state, LifecycleState::Working);
        assert_eq!(tool_category, Some(ToolCategory::FileRead));
    } else {
        panic!("Unexpected event variant");
    }

    let _ = fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_worker_run_and_kill() {
    let project_dir = unique_test_path("worker-test-proj");
    fs::create_dir_all(&project_dir).unwrap();

    let script_path = project_dir.join("claude.py");
    fs::write(&script_path, "import time, sys\ntime.sleep(60.0)\n").unwrap();

    let mut child = std::process::Command::new("python3")
        .arg(&script_path)
        .arg("60")
        .current_dir(&project_dir)
        .spawn()
        .expect("Failed to spawn python worker");

    let pid = child.id();
    assert!(pid > 0);

    // Verify worker process is running
    assert!(PathBuf::from(format!("/proc/{}", pid)).exists());

    // Kill using nix signal
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();

    let _ = child.wait();

    // Verify worker process is dead
    assert!(!PathBuf::from(format!("/proc/{}", pid)).exists());

    let _ = fs::remove_dir_all(&project_dir);
}
