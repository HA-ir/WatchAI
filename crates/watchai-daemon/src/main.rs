use chrono::Utc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use watchai_adapters::registry::AdapterRegistry;
use watchai_adapters::traits::EventSink;
use watchai_core::liveness::{
    prune_retained_sessions, run_liveness_cycle, RealProcStatReader,
    LIVENESS_CHECK_INTERVAL_SECONDS,
};
use watchai_core::session::{
    process_lifecycle_event, AgentSession, EventProcessingOutcome, SessionLifecycleEvent,
    SessionRegistry,
};
use watchai_daemon::logging;
use watchai_daemon::sync::sync_aggregate_state;
use watchai_daemon::systemd::SystemdNotifier;
use watchai_ipc::dbus_service::{WatchAiDbusService, BUS_NAME, OBJECT_PATH};
use watchai_ipc::protocol::SessionDto;

pub const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Rate-limiter to prevent log saturation during persistent errors or signal failures.
pub struct RateLimiter {
    min_interval: Duration,
    last_emitted: HashMap<String, Instant>,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last_emitted: HashMap::new(),
        }
    }

    pub fn should_log(&mut self, key: &str) -> bool {
        let now = Instant::now();
        if let Some(last) = self.last_emitted.get(key) {
            if now.duration_since(*last) < self.min_interval {
                return false;
            }
        }
        self.last_emitted.insert(key.to_string(), now);
        true
    }
}

/// Waits for an operating system shutdown signal (SIGINT or SIGTERM on Unix).
pub async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C (SIGINT) handler");
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(e) => {
                warn!("Failed to install SIGTERM signal handler: {}", e);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C). Initiating graceful shutdown...");
        }
        _ = terminate => {
            info!("Received SIGTERM. Initiating graceful shutdown...");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging with automated privacy redaction filters (T061)
    logging::init_logging();

    info!("Starting WatchAI Daemon v0.1.0...");

    let notifier = SystemdNotifier::from_env();
    let registry = SessionRegistry::new();
    let mut adapter_registry = AdapterRegistry::default_registry();
    let dbus_service = WatchAiDbusService::new(registry.clone());
    let aggregate_lock = dbus_service.aggregate();

    // 0. Startup environment health diagnostics across all registered adapters (T113)
    for adapter in adapter_registry.adapters() {
        let status = adapter.check_environment().await;
        info!(
            "Adapter [{}] ({}) environment status: {:?}",
            adapter.provider_id(),
            adapter.display_name(),
            status
        );
    }

    // 1. Immediate startup /proc discovery sweep across all registered providers before D-Bus claim (T075, T076, T077, T120)
    info!("Performing immediate startup /proc discovery sweep across all providers...");
    let mut startup_discovered = adapter_registry.discover_all().await;
    // Canonical deterministic sorting: provider_id ASC, project_path ASC, process_id ASC (T076, T120)
    startup_discovered.sort_by(|a, b| a.deterministic_cmp(b));

    for d in startup_discovered {
        info!(
            "Discovered surviving session on startup: provider={}, id={}",
            d.provider_id, d.session_id
        );
        let mut session = AgentSession::new(
            d.session_id.clone(),
            d.provider_id.clone(),
            d.provider_display_name.clone(),
            &d.project_path,
            d.process_id,
            d.initial_state,
            d.adapter_status,
        );
        if let Some(st) = d.process_start_time {
            session = session.with_process_start_time(Some(st));
        }
        registry.upsert(session).await;
    }
    // Initialize aggregate state from startup discovery
    sync_aggregate_state(&registry, &aggregate_lock, Utc::now()).await;

    // Connect to user D-Bus session bus and request well-known name
    info!("Requesting D-Bus session bus name: '{}'...", BUS_NAME);
    let connection = match zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, dbus_service)?
        .build()
        .await
    {
        Ok(conn) => {
            info!("Successfully registered D-Bus service at '{}'", OBJECT_PATH);
            conn
        }
        Err(e) => {
            error!("Failed to register on D-Bus session bus: {}", e);
            return Err(e.into());
        }
    };

    // Obtain interface reference for signal emission
    let iface_ref = connection
        .clone()
        .object_server()
        .interface::<_, WatchAiDbusService>(OBJECT_PATH)
        .await?;

    let bg_registry = registry.clone();
    let bg_adapters = adapter_registry.clone();
    let bg_iface = iface_ref.clone();
    let bg_agg = aggregate_lock.clone();

    // Shutdown coordination channel
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    // Initialize bounded asynchronous event ingestion channel (T116, T117)
    let (event_tx, mut event_rx) =
        tokio::sync::mpsc::channel::<SessionLifecycleEvent>(EVENT_CHANNEL_CAPACITY);

    // Connect event ingestion sink to adapter registry architecture (Audit Finding 1)
    let event_sink = EventSink::new(event_tx.clone(), Some(shutdown_tx.subscribe()));
    adapter_registry.attach_event_sink(event_sink);

    let ingest_registry = registry.clone();
    let ingest_agg = aggregate_lock.clone();
    let ingest_iface = iface_ref.clone();
    let mut ingest_shutdown_rx = shutdown_tx.subscribe();

    // Spawn asynchronous event ingestion consumer task (T116, T117)
    let ingest_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            let outcome = process_lifecycle_event(&ingest_registry, &event).await;
                            match outcome {
                                EventProcessingOutcome::Applied { session_id, .. } => {
                                    if let Some(session) = ingest_registry.get(&session_id).await {
                                        let dto = SessionDto::from(&session);
                                        if let Err(e) = WatchAiDbusService::emit_session_updated(
                                            ingest_iface.signal_context(),
                                            &dto,
                                        ).await {
                                            warn!("Failed to emit SessionUpdated signal: {}", e);
                                        }

                                        let now = Utc::now();
                                        if let Some(agg_dto) = sync_aggregate_state(&ingest_registry, &ingest_agg, now).await {
                                            if let Err(e) = WatchAiDbusService::emit_aggregate_state_changed(
                                                ingest_iface.signal_context(),
                                                &agg_dto.state,
                                                agg_dto.active_session_count,
                                                agg_dto.waiting_session_count,
                                                agg_dto.error_session_count,
                                                &agg_dto.updated_at,
                                            ).await {
                                                warn!("Failed to emit AggregateStateChanged signal: {}", e);
                                            }
                                        }
                                    }
                                }
                                EventProcessingOutcome::HeartbeatApplied { .. } => {
                                    // Heartbeat updated last_seen_at; no state change signal needed
                                }
                                EventProcessingOutcome::DroppedUnknownSession { session_id } => {
                                    debug!("Ingestion consumer dropped event for unknown session: {}", session_id);
                                }
                                EventProcessingOutcome::DroppedInvalidTransition { session_id, error } => {
                                    debug!("Ingestion consumer dropped invalid transition for {}: {}", session_id, error);
                                }
                            }
                        }
                        None => break, // All event senders dropped
                    }
                }
                _ = ingest_shutdown_rx.changed() => {
                    // Drain remaining critical events on shutdown (T119)
                    while let Ok(event) = event_rx.try_recv() {
                        if event.is_critical() {
                            let _ = process_lifecycle_event(&ingest_registry, &event).await;
                        }
                    }
                    break;
                }
            }
        }
    });

    // Spawn unified liveness, discovery, retention, and dwell loop (T062–T065)
    let loop_handle = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(Duration::from_secs(LIVENESS_CHECK_INTERVAL_SECONDS));
        let proc_reader = RealProcStatReader;
        let mut rate_limiter = RateLimiter::new(Duration::from_secs(60));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let now = Utc::now();

                    // A. Liveness check: verify /proc survival and PID reuse (T062)
                    let outcome = run_liveness_cycle(&bg_registry, &proc_reader, now).await;
                    for updated_session in outcome.updated_sessions {
                        let dto = SessionDto::from(&updated_session);
                        if let Err(e) =
                            WatchAiDbusService::emit_session_updated(bg_iface.signal_context(), &dto).await
                        {
                            if rate_limiter.should_log("session_updated_signal_err") {
                                warn!("Failed to emit SessionUpdated signal (rate-limited): {}", e);
                            }
                        }
                    }

                    // B. Terminal retention pruning: prune sessions > 60s in terminal state (T065)
                    let pruned_ids = prune_retained_sessions(&bg_registry, now).await;
                    for pruned_id in pruned_ids {
                        info!("Pruned expired terminal session from memory: {}", pruned_id);
                        if let Err(e) =
                            WatchAiDbusService::emit_session_removed(bg_iface.signal_context(), &pruned_id)
                                .await
                        {
                            if rate_limiter.should_log("session_removed_signal_err") {
                                warn!("Failed to emit SessionRemoved signal (rate-limited): {}", e);
                            }
                        }
                    }

                    // C. Periodic process discovery sweep for new sessions across all providers
                    let mut discovered = bg_adapters.discover_all().await;
                    discovered.sort_by(|a, b| a.deterministic_cmp(b));

                    for d in discovered {
                        if bg_registry.get(&d.session_id).await.is_none() {
                            info!(
                                "Discovered new session: provider={}, id={}",
                                d.provider_id, d.session_id
                            );
                            let mut session = AgentSession::new(
                                d.session_id.clone(),
                                d.provider_id.clone(),
                                d.provider_display_name.clone(),
                                &d.project_path,
                                d.process_id,
                                d.initial_state,
                                d.adapter_status,
                            );
                            if let Some(st) = d.process_start_time {
                                session = session.with_process_start_time(Some(st));
                            }
                            bg_registry.upsert(session.clone()).await;

                            let dto = SessionDto::from(&session);
                            if let Err(e) =
                                WatchAiDbusService::emit_session_added(bg_iface.signal_context(), &dto)
                                    .await
                            {
                                if rate_limiter.should_log("session_added_signal_err") {
                                    warn!("Failed to emit SessionAdded signal (rate-limited): {}", e);
                                }
                            }
                        }
                    }

                    // D. Synchronize aggregate state, evaluate 10s completion dwell, and throttle signals (T063, T064)
                    if let Some(agg_dto) = sync_aggregate_state(&bg_registry, &bg_agg, now).await {
                        info!(
                            "Aggregate state changed: state={}, active={}, waiting={}, error={}",
                            agg_dto.state,
                            agg_dto.active_session_count,
                            agg_dto.waiting_session_count,
                            agg_dto.error_session_count
                        );
                        if let Err(e) = WatchAiDbusService::emit_aggregate_state_changed(
                            bg_iface.signal_context(),
                            &agg_dto.state,
                            agg_dto.active_session_count,
                            agg_dto.waiting_session_count,
                            agg_dto.error_session_count,
                            &agg_dto.updated_at,
                        )
                        .await
                        {
                            if rate_limiter.should_log("aggregate_state_signal_err") {
                                warn!("Failed to emit AggregateStateChanged signal (rate-limited): {}", e);
                            }
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Background tasks received shutdown signal. Terminating loop cleanly.");
                    break;
                }
            }
        }
    });

    info!("WatchAI Daemon is running. Monitoring agent sessions (SIGINT / SIGTERM to stop).");

    // Report ready to systemd ONLY after initialization, discovery, and worker loops are running (T059)
    let initial_sessions_count = registry.count().await;
    let _ = notifier.notify_status(&format!("Monitoring {} session(s)", initial_sessions_count));
    if let Err(e) = notifier.notify_ready() {
        warn!("Failed to send systemd READY notification: {}", e);
    } else if notifier.is_available() {
        info!("Successfully signaled systemd readiness (READY=1)");
    }

    // Graceful shutdown handling (T060, T119)
    wait_for_shutdown_signal().await;
    let _ = notifier.notify_stopping();
    info!("Initiating clean termination of background workers...");

    // Repeated signals during drain are caught harmlessly without bypassing cleanup (T060)
    let repeated_signal_handle = tokio::spawn(async {
        wait_for_shutdown_signal().await;
        info!("Received repeated shutdown signal during drain; graceful shutdown already in progress.");
    });

    let _ = shutdown_tx.send(true);

    // Bounded shutdown drain: ensure workers terminate cleanly within 5.0s deadline without hanging (T060)
    let drain_future = async {
        let _ = loop_handle.await;
        drop(event_tx); // Drop daemon event sender to allow consumer to finish
        let _ = ingest_handle.await;
    };

    if tokio::time::timeout(Duration::from_secs(5), drain_future)
        .await
        .is_err()
    {
        warn!("Worker shutdown drain exceeded 5.0s timeout. Proceeding to D-Bus release.");
    }
    repeated_signal_handle.abort();

    // Release D-Bus bus name cleanly so broker broadcasts NameOwnerChanged(old, "") to clients (T060)
    info!("Releasing D-Bus bus name '{}'...", BUS_NAME);
    if let Err(e) = connection.release_name(BUS_NAME).await {
        warn!(
            "Failed to cleanly release D-Bus bus name '{}': {}",
            BUS_NAME, e
        );
    } else {
        info!("Successfully released D-Bus bus name '{}'", BUS_NAME);
    }

    info!("All background tasks terminated. WatchAI Daemon shutdown complete.");

    Ok(())
}
