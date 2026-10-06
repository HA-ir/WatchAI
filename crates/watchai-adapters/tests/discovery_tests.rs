use std::fs;
use watchai_adapters::discovery::ProcessScanner;

#[test]
fn test_read_process_start_time_returns_none_on_missing_or_corrupt_stat() {
    let temp_dir = std::env::temp_dir().join(format!("watchai-test-proc-{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Missing stat file -> returns None
    assert_eq!(ProcessScanner::read_process_start_time(&temp_dir), None);

    // 2. Corrupted stat file without comm or field 22 -> returns None
    fs::write(temp_dir.join("stat"), "corrupted-content-no-fields").unwrap();
    assert_eq!(ProcessScanner::read_process_start_time(&temp_dir), None);

    // 3. Valid stat file -> returns parsed ticks
    let valid_stat =
        "1234 (claude) S 1 1 1 0 0 4194304 100 0 0 0 10 20 0 0 20 0 4 0 987654 12345678";
    fs::write(temp_dir.join("stat"), valid_stat).unwrap();
    assert_eq!(
        ProcessScanner::read_process_start_time(&temp_dir),
        Some(987654)
    );

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_scan_proc_dir_isolates_faults_and_discovers_siblings() {
    let mock_proc = std::env::temp_dir().join(format!("watchai-mock-proc-{}", std::process::id()));
    let _ = fs::create_dir_all(&mock_proc);

    // 1. Non-numeric directory: should be skipped
    let _ = fs::create_dir_all(mock_proc.join("sys"));

    // 2. PID 101: missing cmdline -> should be skipped without failing sweep
    let _ = fs::create_dir_all(mock_proc.join("101"));

    // 3. PID 102: matching cmdline "claude", but unreadable/missing stat -> should be skipped
    let pid102 = mock_proc.join("102");
    let _ = fs::create_dir_all(&pid102);
    fs::write(pid102.join("cmdline"), b"claude\0--interactive\0").unwrap();

    // 4. PID 103: matching cmdline, valid stat with starttime 55555 -> MUST be discovered!
    let pid103 = mock_proc.join("103");
    let _ = fs::create_dir_all(&pid103);
    fs::write(pid103.join("cmdline"), b"claude\0").unwrap();
    let stat103 = "103 (claude) S 1 1 1 0 0 4194304 100 0 0 0 10 20 0 0 20 0 4 0 55555 12345678";
    fs::write(pid103.join("stat"), stat103).unwrap();

    // Execute scan
    let discovered =
        ProcessScanner::scan_proc_dir(&mock_proc, "claude", "claude-code", "Claude Code");

    assert_eq!(
        discovered.len(),
        1,
        "Faulty sibling entries must be skipped while valid PID 103 is discovered"
    );
    let s = &discovered[0];
    assert_eq!(s.process_id, Some(103));
    assert_eq!(s.process_start_time, Some(55555));
    assert_eq!(s.initial_state, watchai_core::state::LifecycleState::Idle);
    assert_eq!(
        s.adapter_status,
        watchai_core::session::AdapterStatus::DiscoveryRequired
    );

    // Cleanup
    let _ = fs::remove_dir_all(&mock_proc);
}

#[test]
fn test_scan_processes_multi_respects_proc_root_env_and_fallbacks() {
    let mock_proc =
        std::env::temp_dir().join(format!("watchai-mock-multi-proc-{}", std::process::id()));
    let _ = fs::create_dir_all(&mock_proc);

    let pid200 = mock_proc.join("200");
    let _ = fs::create_dir_all(&pid200);
    fs::write(pid200.join("cmdline"), b"claude\0").unwrap();
    let stat200 = "200 (claude) S 1 1 1 0 0 4194304 100 0 0 0 10 20 0 0 20 0 4 0 66666 12345678";
    fs::write(pid200.join("stat"), stat200).unwrap();

    // With WATCHAI_PROC_ROOT pointing to mock_proc, scan_processes_multi discovers PID 200
    std::env::set_var("WATCHAI_PROC_ROOT", mock_proc.to_str().unwrap());
    let discovered =
        ProcessScanner::scan_processes_multi(&["claude"], "claude-code", "Claude Code");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].process_id, Some(200));

    // Cleanup and reset
    std::env::remove_var("WATCHAI_PROC_ROOT");
    let _ = fs::remove_dir_all(&mock_proc);
}
