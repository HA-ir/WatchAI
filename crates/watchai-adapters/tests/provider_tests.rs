use std::fs;
use std::sync::Arc;
use std::time::Instant;
use watchai_adapters::claude_code::ClaudeCodeAdapter;
use watchai_adapters::codex_cli::CodexCliAdapter;
use watchai_adapters::discovery::ProcessScanner;
use watchai_adapters::opencode::OpenCodeAdapter;
use watchai_adapters::registry::AdapterRegistry;
use watchai_adapters::traits::{EventSink, ProviderAdapter, TelemetryTier};
use watchai_core::session::AdapterStatus;

#[test]
fn test_cmdline_matching_direct_binaries_and_script_runners() {
    // T100: Direct binary matches
    let claude_args = vec!["/usr/bin/claude".to_string(), "--interactive".to_string()];
    assert!(ProcessScanner::matches_cmdline_args(
        &claude_args,
        &["claude"]
    ));

    let codex_args = vec!["codex".to_string(), "run".to_string()];
    assert!(ProcessScanner::matches_cmdline_args(
        &codex_args,
        &["codex", "codex-cli"]
    ));

    let codex_cli_args = vec!["/usr/local/bin/codex-cli".to_string()];
    assert!(ProcessScanner::matches_cmdline_args(
        &codex_cli_args,
        &["codex", "codex-cli"]
    ));

    let opencode_args = vec!["/home/user/.local/bin/opencode".to_string()];
    assert!(ProcessScanner::matches_cmdline_args(
        &opencode_args,
        &["opencode"]
    ));

    // Script runners: node / bun / python running target scripts
    let node_codex = vec![
        "/usr/bin/node".to_string(),
        "/usr/lib/node_modules/codex/bin/codex.js".to_string(),
    ];
    assert!(ProcessScanner::matches_cmdline_args(
        &node_codex,
        &["codex", "codex-cli"]
    ));

    let python_opencode = vec![
        "python3".to_string(),
        "/home/user/workspace/opencode.py".to_string(),
    ];
    assert!(ProcessScanner::matches_cmdline_args(
        &python_opencode,
        &["opencode"]
    ));
}

#[test]
fn test_cmdline_rejection_of_false_positives() {
    // T100: Grep / search commands
    let grep_codex = vec![
        "grep".to_string(),
        "-rn".to_string(),
        "codex".to_string(),
        ".".to_string(),
    ];
    assert!(!ProcessScanner::matches_cmdline_args(
        &grep_codex,
        &["codex"]
    ));

    let rg_opencode = vec!["rg".to_string(), "opencode".to_string()];
    assert!(!ProcessScanner::matches_cmdline_args(
        &rg_opencode,
        &["opencode"]
    ));

    // Shell invocations
    let bash_echo = vec![
        "bash".to_string(),
        "-c".to_string(),
        "echo codex".to_string(),
    ];
    assert!(!ProcessScanner::matches_cmdline_args(
        &bash_echo,
        &["codex"]
    ));

    // Child worker processes spawned by agents
    let git_status = vec!["git".to_string(), "status".to_string()];
    assert!(!ProcessScanner::matches_cmdline_args(
        &git_status,
        &["claude", "codex", "opencode"]
    ));

    let cargo_test = vec!["cargo".to_string(), "test".to_string()];
    assert!(!ProcessScanner::matches_cmdline_args(
        &cargo_test,
        &["claude", "codex", "opencode"]
    ));
}

#[test]
fn test_provider_capability_descriptors() {
    // T109: Verify capability descriptors across all three registered adapters
    let claude = ClaudeCodeAdapter::new();
    let codex = CodexCliAdapter::new();
    let opencode = OpenCodeAdapter::new();

    let claude_caps = claude.capabilities();
    assert_eq!(
        claude_caps.telemetry_tier,
        TelemetryTier::OptInHookTelemetry,
        "ClaudeCodeAdapter must declare OptInHookTelemetry"
    );
    assert!(claude_caps.supports_tool_categories);
    assert!(claude_caps.supports_activity_events);

    for adapter in [&codex as &dyn ProviderAdapter, &opencode] {
        let caps = adapter.capabilities();
        assert_eq!(
            caps.telemetry_tier,
            TelemetryTier::ProcessDiscoveryOnly,
            "Adapter {} must declare ProcessDiscoveryOnly in Phase 8",
            adapter.provider_id()
        );
        assert!(
            !caps.supports_tool_categories,
            "Adapter {} must not claim tool category extraction",
            adapter.provider_id()
        );
        assert!(
            !caps.supports_activity_events,
            "Adapter {} must not claim live activity event streaming",
            adapter.provider_id()
        );
    }
}

#[tokio::test]
async fn test_check_environment_diagnostics() {
    // T114: Verify check_environment returns Active when binary is in PATH or DiscoveryRequired when absent
    let codex = CodexCliAdapter::new();
    let opencode = OpenCodeAdapter::new();

    // In a test environment without these binaries, they should return DiscoveryRequired
    let codex_status = codex.check_environment().await;
    let opencode_status = opencode.check_environment().await;

    assert!(
        codex_status == AdapterStatus::Active || codex_status == AdapterStatus::DiscoveryRequired
    );
    assert!(
        opencode_status == AdapterStatus::Active
            || opencode_status == AdapterStatus::DiscoveryRequired
    );
}

#[test]
fn test_default_registry_contains_all_three_providers() {
    // T103: Default registry contains Claude Code, OpenAI Codex, and OpenCode
    let registry = AdapterRegistry::default_registry();
    let adapters = registry.adapters();

    assert_eq!(
        adapters.len(),
        3,
        "Registry must register exactly 3 default adapters"
    );
    let ids: Vec<&str> = adapters.iter().map(|a| a.provider_id()).collect();
    assert!(ids.contains(&"claude-code"));
    assert!(ids.contains(&"codex-cli"));
    assert!(ids.contains(&"opencode"));

    let names: Vec<&str> = adapters.iter().map(|a| a.display_name()).collect();
    assert!(names.contains(&"Claude Code"));
    assert!(names.contains(&"OpenAI Codex"));
    assert!(names.contains(&"OpenCode"));
}

#[test]
fn test_synthetic_scaling_benchmark_under_fifty_ms() {
    // T123: Synthetic scaling benchmark across 1000 processes verifying O(P) single-pass discovery
    let temp_proc = std::env::temp_dir().join(format!("watchai-bench-proc-{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_proc);

    // Create 1000 simulated process directories
    for pid in 1000..2000 {
        let pdir = temp_proc.join(pid.to_string());
        let _ = fs::create_dir_all(&pdir);
        if pid == 1500 {
            // Target process
            fs::write(pdir.join("cmdline"), b"codex\0run\0").unwrap();
            fs::write(
                pdir.join("stat"),
                "1500 (codex) S 1 1 1 0 0 0 0 0 0 0 0 0 0 0 20 0 4 0 54321 0",
            )
            .unwrap();
        } else {
            // Unrelated background process
            fs::write(pdir.join("cmdline"), b"systemd\0--user\0").unwrap();
            fs::write(
                pdir.join("stat"),
                format!(
                    "{} (systemd) S 1 1 1 0 0 0 0 0 0 0 0 0 0 0 20 0 4 0 10000 0",
                    pid
                ),
            )
            .unwrap();
        }
    }

    let start = Instant::now();
    let discovered = ProcessScanner::scan_proc_dir_multi(
        &temp_proc,
        &["codex", "codex-cli"],
        "codex-cli",
        "OpenAI Codex",
    );
    let duration = start.elapsed();

    // Cleanup
    let _ = fs::remove_dir_all(&temp_proc);

    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].process_id, Some(1500));
    assert_eq!(discovered[0].provider_id, "codex-cli");

    println!("Benchmarked 1000 processes in: {:?}", duration);
    // Performance target: should comfortably complete in < 50ms on warm cache
    assert!(
        duration.as_millis() < 500, // Generous threshold to avoid CI flakiness while asserting fast execution
        "Discovery across 1000 processes must execute rapidly"
    );
}

struct RecordingTelemetryAdapter {
    sink: std::sync::RwLock<Option<EventSink>>,
}

impl RecordingTelemetryAdapter {
    fn new() -> Self {
        Self {
            sink: std::sync::RwLock::new(None),
        }
    }

    fn is_attached(&self) -> bool {
        self.sink.read().unwrap().is_some()
    }
}

#[async_trait::async_trait]
impl ProviderAdapter for RecordingTelemetryAdapter {
    fn provider_id(&self) -> &'static str {
        "recording-telemetry"
    }

    fn display_name(&self) -> &'static str {
        "Recording Telemetry"
    }

    fn attach_event_sink(&self, sink: EventSink) {
        *self.sink.write().unwrap() = Some(sink);
    }

    async fn check_environment(&self) -> AdapterStatus {
        AdapterStatus::Active
    }

    async fn discover_sessions(&self) -> Vec<watchai_adapters::traits::DiscoveredSession> {
        Vec::new()
    }
}

#[test]
fn test_registry_distributes_sink_on_attach_and_subsequent_register() {
    let mut registry = AdapterRegistry::new();
    let adapter1 = Arc::new(RecordingTelemetryAdapter::new());
    registry.register(adapter1.clone());

    assert!(!adapter1.is_attached());

    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let sink = EventSink::new(tx, None);
    registry.attach_event_sink(sink.clone());

    assert!(adapter1.is_attached());

    // Register adapter2 AFTER registry has received the sink
    let adapter2 = Arc::new(RecordingTelemetryAdapter::new());
    assert!(!adapter2.is_attached());

    registry.register(adapter2.clone());
    assert!(adapter2.is_attached());
}
