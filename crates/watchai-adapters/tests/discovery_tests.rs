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
