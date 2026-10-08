use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use watchai_adapters::discovery::ProcessScanner;
use watchai_adapters::registry::AdapterRegistry;
use watchai_adapters::traits::{
    async_trait, DiscoveredSession, EventSink, IngestionError, ProviderAdapter,
};
use watchai_core::aggregate::compute_aggregate_state;
use watchai_core::session::{
    derive_process_session_id, process_lifecycle_event, AdapterStatus, AgentSession,
    EventProcessingOutcome, SessionLifecycleEvent, SessionRegistry, ToolCategory,
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
    // T118: Prove 256-capacity channel saturation semantics with 300 heartbeats (Audit Finding 2)
    // Invariants:
    // 1. Channel is bounded at 256.
    // 2. Heartbeats are shed when capacity < 50.
    // 3. Critical StateTransition and SessionTerminated survive flood.
    // 4. Zero deadlock, zero sleeps, deterministic.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SessionLifecycleEvent>(256);
    let sink = EventSink::new(tx, None);
    let session_id = "test-session-256-saturation".to_string();

    // 1. Flood with 300 heartbeats
    let mut heartbeats_accepted = 0;
    let mut heartbeats_shed = 0;
    for _ in 0..300 {
        let hb = SessionLifecycleEvent::Heartbeat {
            session_id: session_id.clone(),
            timestamp: Utc::now(),
        };
        match sink.ingest(hb).await {
            Ok(()) => heartbeats_accepted += 1,
            Err(IngestionError::HeartbeatShed) => heartbeats_shed += 1,
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    // Capacity is 256; heartbeats shed when remaining capacity < 50 (i.e. at 207 queued)
    assert_eq!(
        heartbeats_accepted, 207,
        "Exactly 207 heartbeats must be accepted before shedding threshold"
    );
    assert_eq!(
        heartbeats_shed, 93,
        "Remaining 93 heartbeats must be shed to preserve headroom"
    );
    assert_eq!(
        sink.capacity(),
        49,
        "Channel must retain 49 reserved slots for critical events"
    );

    // 2. Send critical StateTransition: MUST succeed immediately!
    let crit_trans = SessionLifecycleEvent::StateTransition {
        session_id: session_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: Some(ToolCategory::ModelThinking),
        timestamp: Utc::now(),
    };
    sink.ingest(crit_trans)
        .await
        .expect("Critical StateTransition must not be dropped");

    // 3. Send critical SessionTerminated: MUST succeed immediately!
    let crit_term = SessionLifecycleEvent::SessionTerminated {
        session_id: session_id.clone(),
        exit_code: Some(0),
        timestamp: Utc::now(),
    };
    sink.ingest(crit_term)
        .await
        .expect("Critical SessionTerminated must not be dropped");

    // 4. Drain channel and verify critical events are present in correct order
    let mut received_heartbeats = 0;
    let mut transition_found = false;
    let mut termination_found = false;

    while let Ok(evt) = rx.try_recv() {
        match evt {
            SessionLifecycleEvent::Heartbeat { .. } => received_heartbeats += 1,
            SessionLifecycleEvent::StateTransition { new_state, .. } => {
                assert_eq!(new_state, LifecycleState::Working);
                transition_found = true;
            }
            SessionLifecycleEvent::SessionTerminated { exit_code, .. } => {
                assert_eq!(exit_code, Some(0));
                termination_found = true;
            }
            SessionLifecycleEvent::SessionRegistered { .. } => {}
        }
    }

    assert_eq!(received_heartbeats, 207);
    assert!(
        transition_found,
        "Critical StateTransition must be delivered"
    );
    assert!(
        termination_found,
        "Critical SessionTerminated must be delivered"
    );
}

#[tokio::test]
async fn test_unknown_session_event_explicitly_rejected() {
    // Audit Finding 4: Unknown session events have explicit rejection semantics
    let registry = SessionRegistry::new();

    // 1. Create and upsert known session in IDLE
    let known_id = "session-known-1".to_string();
    let session = AgentSession::new(
        known_id.clone(),
        "claude-code",
        "Claude Code",
        "/workspace/known",
        Some(9001),
        LifecycleState::Idle,
        AdapterStatus::DiscoveryRequired,
    );
    registry.upsert(session).await;

    // 2. Known session transition -> Accepted
    let known_event = SessionLifecycleEvent::StateTransition {
        session_id: known_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: Some(ToolCategory::ModelThinking),
        timestamp: Utc::now(),
    };
    let outcome1 = process_lifecycle_event(&registry, &known_event).await;
    assert_eq!(
        outcome1,
        EventProcessingOutcome::Applied {
            session_id: known_id.clone(),
            new_state: LifecycleState::Working
        }
    );
    assert_eq!(
        registry.get(&known_id).await.unwrap().current_state,
        LifecycleState::Working
    );

    // 3. Unknown session transition -> DroppedUnknownSession
    let ghost_id = "session-ghost-unknown".to_string();
    let ghost_event = SessionLifecycleEvent::StateTransition {
        session_id: ghost_id.clone(),
        new_state: LifecycleState::Working,
        tool_category: None,
        timestamp: Utc::now(),
    };
    let outcome2 = process_lifecycle_event(&registry, &ghost_event).await;
    assert_eq!(
        outcome2,
        EventProcessingOutcome::DroppedUnknownSession {
            session_id: ghost_id.clone()
        }
    );

    // 4. Verify unknown session was NOT created in registry
    assert!(registry.get(&ghost_id).await.is_none());
    assert_eq!(
        registry.count().await,
        1,
        "Unknown event must never create a phantom session"
    );

    // 5. Verify aggregate state is unmutated by ghost event
    let agg = compute_aggregate_state(&registry.list().await, Utc::now());
    assert_eq!(agg.active_session_count, 1);
    assert_eq!(agg.aggregate_state, LifecycleState::Working);

    // 6. Unknown heartbeat -> DroppedUnknownSession
    let ghost_hb = SessionLifecycleEvent::Heartbeat {
        session_id: ghost_id.clone(),
        timestamp: Utc::now(),
    };
    let outcome3 = process_lifecycle_event(&registry, &ghost_hb).await;
    assert_eq!(
        outcome3,
        EventProcessingOutcome::DroppedUnknownSession {
            session_id: ghost_id.clone()
        }
    );

    // 7. Invalid transition on known session -> DroppedInvalidTransition
    let invalid_event = SessionLifecycleEvent::StateTransition {
        session_id: known_id.clone(),
        new_state: LifecycleState::Starting, // Working -> Starting is invalid
        tool_category: None,
        timestamp: Utc::now(),
    };
    let outcome4 = process_lifecycle_event(&registry, &invalid_event).await;
    assert!(matches!(
        outcome4,
        EventProcessingOutcome::DroppedInvalidTransition { .. }
    ));
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

#[tokio::test]
async fn test_adapter_registry_attaches_event_sink_and_shutdown_cancellation() {
    // Audit Finding 1 & Shutdown Safety: EventSink connected to AdapterRegistry and cancellation on shutdown
    let mut registry = AdapterRegistry::default_registry();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SessionLifecycleEvent>(1);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    assert!(registry.event_sink().is_none());

    let sink = EventSink::new(tx, Some(shutdown_rx));
    registry.attach_event_sink(sink.clone());

    assert!(registry.event_sink().is_some());

    // Fill the 1-capacity buffer
    sink.ingest(SessionLifecycleEvent::StateTransition {
        session_id: "s1".to_string(),
        new_state: LifecycleState::Working,
        tool_category: None,
        timestamp: Utc::now(),
    })
    .await
    .unwrap();

    // Now buffer is full (capacity = 0). Next critical send would block unless cancelled by shutdown!
    let sink_clone = sink.clone();
    let blocked_send = tokio::spawn(async move {
        sink_clone
            .ingest(SessionLifecycleEvent::StateTransition {
                session_id: "s2".to_string(),
                new_state: LifecycleState::Working,
                tool_category: None,
                timestamp: Utc::now(),
            })
            .await
    });

    // Trigger shutdown signal: MUST unblock blocked_send with IngestionError::Shutdown!
    shutdown_tx.send(true).unwrap();

    let send_result = blocked_send.await.unwrap();
    assert_eq!(
        send_result,
        Err(IngestionError::Shutdown),
        "Shutdown signal must cooperatively unblock critical senders without deadlock"
    );

    // Drain first event
    assert!(rx.recv().await.is_some());
}

#[tokio::test]
async fn test_adapter_retains_event_sink_and_streams_events_behaviorally() {
    // Proves behavioral contract of ProviderAdapter::attach_event_sink without exposing getters in the production trait.
    // The adapter receives the sink via attach_event_sink, retains it, and uses it to stream events to the daemon.
    struct TelemetryTestAdapter {
        sink: std::sync::RwLock<Option<EventSink>>,
    }

    impl TelemetryTestAdapter {
        fn new() -> Self {
            Self {
                sink: std::sync::RwLock::new(None),
            }
        }

        async fn emit_event(&self, event: SessionLifecycleEvent) -> Result<(), IngestionError> {
            let sink = {
                let guard = self.sink.read().unwrap();
                guard
                    .clone()
                    .expect("EventSink must be attached to adapter")
            };
            sink.ingest(event).await
        }
    }

    #[async_trait]
    impl ProviderAdapter for TelemetryTestAdapter {
        fn provider_id(&self) -> &'static str {
            "telemetry-test"
        }
        fn display_name(&self) -> &'static str {
            "Telemetry Test"
        }
        fn attach_event_sink(&self, sink: EventSink) {
            *self.sink.write().unwrap() = Some(sink);
        }
        async fn check_environment(&self) -> AdapterStatus {
            AdapterStatus::Active
        }
        async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
            Vec::new()
        }
    }

    let mut registry = AdapterRegistry::new();
    let adapter = std::sync::Arc::new(TelemetryTestAdapter::new());
    registry.register(adapter.clone());

    let (tx, mut rx) = tokio::sync::mpsc::channel::<SessionLifecycleEvent>(10);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let sink = EventSink::new(tx, Some(shutdown_rx));
    registry.attach_event_sink(sink);

    // 1. Adapter retains sink and successfully emits lifecycle events to daemon receiver
    adapter
        .emit_event(SessionLifecycleEvent::StateTransition {
            session_id: "test-sess-1".to_string(),
            new_state: LifecycleState::Working,
            tool_category: None,
            timestamp: Utc::now(),
        })
        .await
        .unwrap();

    let received = rx
        .recv()
        .await
        .expect("Daemon receiver must receive event streamed by adapter");
    assert_eq!(received.session_id(), "test-sess-1");

    // 2. Adapter's retained sink handles cooperative shutdown cancellation under buffer saturation
    // Fill the 10-slot buffer so the next send is guaranteed to block
    for i in 0..10 {
        adapter
            .emit_event(SessionLifecycleEvent::StateTransition {
                session_id: format!("fill-{}", i),
                new_state: LifecycleState::Working,
                tool_category: None,
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
    }

    let blocked_adapter = adapter.clone();
    let blocked_task = tokio::spawn(async move {
        blocked_adapter
            .emit_event(SessionLifecycleEvent::StateTransition {
                session_id: "blocked-send".to_string(),
                new_state: LifecycleState::Working,
                tool_category: None,
                timestamp: Utc::now(),
            })
            .await
    });

    shutdown_tx.send(true).unwrap();
    let blocked_outcome = blocked_task.await.unwrap();
    assert_eq!(blocked_outcome, Err(IngestionError::Shutdown));
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

    let mut perm = [d3.clone(), d1.clone(), d2.clone()];
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
