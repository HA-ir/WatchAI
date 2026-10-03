use std::time::Duration;
use tracing::{error, info, warn};
use watchai_adapters::registry::AdapterRegistry;
use watchai_core::session::{AgentSession, SessionRegistry};
use watchai_core::state::LifecycleState;
use watchai_ipc::dbus_service::{WatchAiDbusService, BUS_NAME, OBJECT_PATH};
use watchai_ipc::protocol::{AggregateStateDto, SessionDto};

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

    // Background discovery & polling loop
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;

            // Probe adapters for active sessions
            let discovered = bg_adapters.discover_all().await;
            for d in discovered {
                if bg_registry.get(&d.session_id).await.is_none() {
                    info!(
                        "Discovered new session: provider={}, id={}",
                        d.provider_id, d.session_id
                    );
                    let session = AgentSession::new(
                        d.session_id.clone(),
                        d.provider_id.clone(),
                        d.provider_display_name.clone(),
                        &d.project_path,
                        d.process_id,
                        d.initial_state,
                        d.adapter_status,
                    );
                    bg_registry.upsert(session.clone()).await;

                    // Emit SessionAdded signal
                    let dto = SessionDto::from(&session);
                    if let Err(e) =
                        WatchAiDbusService::emit_session_added(bg_iface.signal_context(), &dto)
                            .await
                    {
                        warn!("Failed to emit SessionAdded signal: {}", e);
                    }

                    // Update aggregate state and emit AggregateStateChanged signal
                    let agg_dto = AggregateStateDto {
                        state: d.initial_state.to_string(),
                        active_session_count: bg_registry.count().await as u32,
                        waiting_session_count: if d.initial_state == LifecycleState::Waiting {
                            1
                        } else {
                            0
                        },
                        error_session_count: if d.initial_state == LifecycleState::Error {
                            1
                        } else {
                            0
                        },
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    };

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
        }
    });

    info!("WatchAI Daemon is running. Press Ctrl+C to terminate.");
    tokio::signal::ctrl_c().await?;
    info!("WatchAI Daemon shutting down gracefully.");

    Ok(())
}
