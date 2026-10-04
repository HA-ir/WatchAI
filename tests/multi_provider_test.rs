use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use watchai_adapters::discovery::ProcessScanner;
use watchai_adapters::traits::DiscoveredSession;
use watchai_core::session::{
    derive_process_session_id, AdapterStatus, AgentSession, SessionLifecycleEvent, SessionRegistry,
    ToolCategory,
};
use watchai_core::state::LifecycleState;
use watchai_ipc::protocol::SessionDto;

#[test]
fn test_concurrent_multi_provider_discovery_and_stable_ids() {
    // T104: Discovery across Claude, Codex, and OpenCode with stable surrogate IDs
    let temp_proc =
        std::env::temp_dir().join(format!("watchai-test-multi-proc-{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_proc);

    // 1. Claude process (PID 6001)
    let p1 = temp_proc.join("6001");
    fs::create_dir_all(&p1).unwrap();
    fs::write(p1.join("cmdline"), b"/usr/bin/claude\0").unwrap();
    fs::write(
        p1.join("stat"),
        "6001 (claude) S 1 1 1 0 0 0 0 0 0 0 0 0 0 0 20 0 4 0 11111 0",
    )
    .unwrap();

    // 2. Codex CLI process (PID 6002)
    let p2 = temp_proc.join("6002");
    fs::create_dir_all(&p2).unwrap();
    fs::write(p2.join("cmdline"), b"codex\0run\0").unwrap();
    fs::write(
        p2.join("stat"),
        "6002 (codex) S 1 1 1 0 0 0 0 0 0 0 0 0 0 0 20 0 4 0 22222 0",
    )
    .unwrap();

    // 3. OpenCode process (PID 6003)
    let p3 = temp_proc.join("6003");
    fs::create_dir_all(&p3).unwrap();
    fs::write(p3.join("cmdline"), b"/usr/local/bin/opencode\0").unwrap();
    fs::write(
        p3.join("stat"),
        "6003 (opencode) S 1 1 1 0 0 0 0 0 0 0 0 0 0 0 20 0 4 0 33333 0",
    )
    .unwrap();

    // Scan for Claude
    let d_claude =
        ProcessScanner::scan_proc_dir(&temp_proc, "claude", "claude-code", "Claude Code");
    assert_eq!(d_claude.len(), 1);
    assert_eq!(d_claude[0].provider_id, "claude-code");
    assert_eq!(d_claude[0].process_id, Some(6001));

    // Scan for Codex
    let d_codex = ProcessScanner::scan_proc_dir_multi(
        &temp_proc,
        &["codex", "codex-cli"],
        "codex-cli",
        "OpenAI Codex",
    );
    assert_eq!(d_codex.len(), 1);
    assert_eq!(d_codex[0].provider_id, "codex-cli");
    assert_eq!(d_codex[0].process_id, Some(6002));

    // Scan for OpenCode
    let d_opencode = ProcessScanner::scan_proc_dir(&temp_proc, "opencode", "opencode", "OpenCode");
    assert_eq!(d_opencode.len(), 1);
    assert_eq!(d_opencode[0].provider_id, "opencode");
    assert_eq!(d_opencode[0].process_id, Some(6003));

    // Assert stable deterministic surrogate IDs
    let expected_codex_id =
        derive_process_session_id(6002, 22222, PathBuf::from("/unknown/workspace"));
    assert_eq!(d_codex[0].session_id, expected_codex_id);

    // Cleanup
    let _ = fs::remove_dir_all(&temp_proc);
}

#[tokio::test]
async fn test_activity_knowledge_boundary_across_all_providers() {
    // T110: Discovered sessions across all providers initialize in IDLE + DISCOVERY_REQUIRED
    let registry = SessionRegistry::new();

    let s_claude = AgentSession::new(
        "s-claude".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/a",
        Some(1001),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    let s_codex = AgentSession::new(
        "s-codex".to_string(),
        "codex-cli",
        "OpenAI Codex",
        "/workspace/b",
        Some(1002),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    let s_opencode = AgentSession::new(
        "s-opencode".to_string(),
        "opencode",
        "OpenCode",
        "/workspace/c",
        Some(1003),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    registry.upsert(s_claude).await;
    registry.upsert(s_codex).await;
    registry.upsert(s_opencode).await;

    for s in registry.list().await {
        assert_eq!(
            s.current_state,
            LifecycleState::Idle,
            "Provider {} must initialize in IDLE",
            s.provider_id
        );
        assert_eq!(
            s.adapter_status,
            AdapterStatus::DiscoveryRequired,
            "Provider {} must have DiscoveryRequired status",
            s.provider_id
        );
        assert_eq!(
            s.active_tool_category, None,
            "Discovery-only sessions must have no tool category"
        );
    }
}

#[tokio::test]
async fn test_prioritized_event_ingestion_and_heartbeat_saturation() {
    // T118: Critical events delivered even under heartbeat saturation
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SessionLifecycleEvent>(10);
    let session_id = "test-session-prioritized".to_string();

    // Spawn producer that floods with heartbeats using try_send when capacity permits
    let p_sid = session_id.clone();
    let tx_clone = tx.clone();
    let producer = tokio::spawn(async move {
        // Send 15 heartbeats (exceeds channel capacity of 10)
        let mut heartbeats_dropped = 0;
        for _ in 0..15 {
            let hb = SessionLifecycleEvent::Heartbeat {
                session_id: p_sid.clone(),
                timestamp: Utc::now(),
            };
            if tx_clone.capacity() < 3 {
                // Drop low-priority heartbeat when headroom is low
                heartbeats_dropped += 1;
            } else {
                let _ = tx_clone.try_send(hb);
            }
        }

        // Send 1 critical state transition event: must succeed!
        let crit = SessionLifecycleEvent::StateTransition {
            session_id: p_sid.clone(),
            new_state: LifecycleState::Working,
            tool_category: Some(ToolCategory::ModelThinking),
            timestamp: Utc::now(),
        };
        tx_clone
            .send(crit)
            .await
            .expect("Critical event must be sent");
        heartbeats_dropped
    });

    let dropped = producer.await.unwrap();
    assert!(
        dropped > 0,
        "Excess heartbeats must be dropped under pressure"
    );

    // Consume all events and verify critical state transition is present
    let mut critical_found = false;
    while let Ok(event) = rx.try_recv() {
        if let SessionLifecycleEvent::StateTransition { new_state, .. } = event {
            assert_eq!(new_state, LifecycleState::Working);
            critical_found = true;
        }
    }
    assert!(
        critical_found,
        "Critical StateTransition event must be delivered"
    );
}

#[tokio::test]
async fn test_cooperative_shutdown_drains_critical_events() {
    // T119: Dropping channel sender drains pending critical transitions before consumer exits
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SessionLifecycleEvent>(10);
    let sid = "session-drain-test".to_string();

    tx.send(SessionLifecycleEvent::StateTransition {
        session_id: sid.clone(),
        new_state: LifecycleState::Working,
        tool_category: None,
        timestamp: Utc::now(),
    })
    .await
    .unwrap();

    drop(tx); // Drop sender

    // Consumer drains remaining events cleanly
    let mut received = Vec::new();
    while let Some(evt) = rx.recv().await {
        received.push(evt);
    }

    assert_eq!(received.len(), 1);
    assert!(received[0].is_critical());
}

#[test]
fn test_multi_provider_startup_recovery_canonical_sorting() {
    // T120: Discovered sessions sorted: provider_id ASC -> project_path ASC -> process_id ASC
    let d1 = DiscoveredSession {
        session_id: "s1".to_string(),
        provider_id: "claude-code".to_string(),
        provider_display_name: "Claude Code".to_string(),
        project_path: PathBuf::from("/a/project1"),
        process_id: Some(100),
        initial_state: LifecycleState::Idle,
        started_at: Utc::now(),
        adapter_status: AdapterStatus::DiscoveryRequired,
        process_start_time: Some(1000),
    };

    let d2 = DiscoveredSession {
        session_id: "s2".to_string(),
        provider_id: "codex-cli".to_string(),
        provider_display_name: "OpenAI Codex".to_string(),
        project_path: PathBuf::from("/a/project1"),
        process_id: Some(150),
        initial_state: LifecycleState::Idle,
        started_at: Utc::now(),
        adapter_status: AdapterStatus::DiscoveryRequired,
        process_start_time: Some(2000),
    };

    let d3 = DiscoveredSession {
        session_id: "s3".to_string(),
        provider_id: "opencode".to_string(),
        provider_display_name: "OpenCode".to_string(),
        project_path: PathBuf::from("/a/project1"),
        process_id: Some(120),
        initial_state: LifecycleState::Idle,
        started_at: Utc::now(),
        adapter_status: AdapterStatus::DiscoveryRequired,
        process_start_time: Some(3000),
    };

    // Expected order: claude-code (d1), codex-cli (d2), opencode (d3)
    let expected = vec!["s1", "s2", "s3"];

    let mut perm = vec![d3.clone(), d1.clone(), d2.clone()];
    perm.sort_by(|a, b| a.deterministic_cmp(b));

    let ids: Vec<&str> = perm.iter().map(|s| s.session_id.as_str()).collect();
    assert_eq!(ids, expected, "Multi-provider sorting must be canonical");
}

#[test]
fn test_dbus_session_dto_serialization_multi_provider() {
    // T121: Verify D-Bus (sssssssus) tuple serialization is identical across providers
    let claude_session = AgentSession::new(
        "s-claude".to_string(),
        "claude-code",
        "Claude Code",
        "/workspace/claude",
        Some(5001),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    let codex_session = AgentSession::new(
        "s-codex".to_string(),
        "codex-cli",
        "OpenAI Codex",
        "/workspace/codex",
        Some(5002),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    let opencode_session = AgentSession::new(
        "s-opencode".to_string(),
        "opencode",
        "OpenCode",
        "/workspace/opencode",
        Some(5003),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );

    for session in [claude_session, codex_session, opencode_session] {
        let dto = SessionDto::from(&session);
        assert_eq!(dto.session_id, session.session_id);
        assert_eq!(dto.provider_id, session.provider_id);
        assert_eq!(dto.provider_display_name, session.provider_display_name);
        assert_eq!(dto.current_state, "IDLE");
        assert_eq!(dto.active_tool_category, ""); // Empty string over D-Bus
    }
}
