mod sync;

use chrono::Utc;
use std::time::Duration;
use tracing::{error, info, warn};
use watchai_adapters::registry::AdapterRegistry;
use watchai_core::liveness::{
    prune_retained_sessions, run_liveness_cycle, RealProcStatReader,
    LIVENESS_CHECK_INTERVAL_SECONDS,
};
use watchai_core::session::{AgentSession, SessionRegistry};
use watchai_ipc::dbus_service::{WatchAiDbusService, BUS_NAME, OBJECT_PATH};
use watchai_ipc::protocol::SessionDto;

use crate::sync::sync_aggregate_state;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,watchai_daemon=debug,watchai_adapters=debug".into()),
        )
        .init();

    info!("Starting WatchAI Daemon v0.1.0...");

    let registry = SessionRegistry::new();
    let adapter_registry = AdapterRegistry::default_registry();
    let dbus_service = WatchAiDbusService::new(registry.clone());
    let aggregate_lock = dbus_service.aggregate();

    // 1. Immediate startup /proc discovery sweep before starting background tasks (T025)
    info!("Performing immediate startup /proc discovery sweep...");
    let startup_discovered = adapter_registry.discover_all().await;
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
    let object_server = connection.object_server();
    let iface_ref = object_server
        .interface::<_, WatchAiDbusService>(OBJECT_PATH)
        .await?;

    let bg_registry = registry.clone();
    let bg_adapters = adapter_registry.clone();
    let bg_iface = iface_ref.clone();
    let bg_agg = aggregate_lock.clone();

    // Spawn unified liveness, discovery, retention, and dwell loop (T026–T029)
    let loop_handle = tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(Duration::from_secs(LIVENESS_CHECK_INTERVAL_SECONDS));
        let proc_reader = RealProcStatReader;

        loop {
            interval.tick().await;
            let now = Utc::now();

            // A. Liveness check: verify /proc survival and PID reuse (T026)
            let outcome = run_liveness_cycle(&bg_registry, &proc_reader, now).await;
            for updated_session in outcome.updated_sessions {
                let dto = SessionDto::from(&updated_session);
                if let Err(e) =
                    WatchAiDbusService::emit_session_updated(bg_iface.signal_context(), &dto).await
                {
                    warn!("Failed to emit SessionUpdated signal: {}", e);
                }
            }

            // B. Terminal retention pruning: prune sessions > 60s in terminal state (T029)
            let pruned_ids = prune_retained_sessions(&bg_registry, now).await;
            for pruned_id in pruned_ids {
                info!("Pruned expired terminal session from memory: {}", pruned_id);
                if let Err(e) =
                    WatchAiDbusService::emit_session_removed(bg_iface.signal_context(), &pruned_id)
                        .await
                {
                    warn!("Failed to emit SessionRemoved signal: {}", e);
                }
            }

            // C. Periodic process discovery sweep for new sessions
            let discovered = bg_adapters.discover_all().await;
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
                        warn!("Failed to emit SessionAdded signal: {}", e);
                    }
                }
            }

            // D. Synchronize aggregate state, evaluate 10s completion dwell, and throttle signals (T027, T028)
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
                    warn!("Failed to emit AggregateStateChanged signal: {}", e);
                }
            }
        }
    });

    info!("WatchAI Daemon is running. Press Ctrl+C to terminate.");

    // Graceful shutdown handling (T030)
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received. Terminating background tasks cleanly...");
    loop_handle.abort();
    info!("WatchAI Daemon shutdown complete.");

    Ok(())
}
