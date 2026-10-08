use crate::protocol::{AggregateStateDto, SessionDto};
use std::sync::Arc;
use tokio::sync::RwLock;
use watchai_core::session::SessionRegistry;
use zbus::{fdo, interface, object_server::SignalContext};

pub const INTERFACE_NAME: &str = "org.freedesktop.WatchAI";
pub const OBJECT_PATH: &str = "/org/freedesktop/WatchAI";
pub const BUS_NAME: &str = "org.freedesktop.WatchAI";

/// The local D-Bus service exposing WatchAI session and aggregate states.
pub struct WatchAiDbusService {
    registry: SessionRegistry,
    aggregate: Arc<RwLock<AggregateStateDto>>,
    version: &'static str,
}

impl WatchAiDbusService {
    pub fn new(registry: SessionRegistry) -> Self {
        let initial_aggregate = AggregateStateDto {
            state: "IDLE".to_string(),
            active_session_count: 0,
            waiting_session_count: 0,
            error_session_count: 0,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        Self {
            registry,
            aggregate: Arc::new(RwLock::new(initial_aggregate)),
            version: "0.1.0",
        }
    }

    pub fn aggregate(&self) -> Arc<RwLock<AggregateStateDto>> {
        self.aggregate.clone()
    }
}

#[interface(name = "org.freedesktop.WatchAI")]
impl WatchAiDbusService {
    /// Return current aggregate state and counters.
    async fn get_aggregate_state(&self) -> (String, u32, u32, u32, String) {
        let agg = self.aggregate.read().await;
        (
            agg.state.clone(),
            agg.active_session_count,
            agg.waiting_session_count,
            agg.error_session_count,
            agg.updated_at.clone(),
        )
    }

    /// Return all active and retaining sessions as an array of structs.
    async fn get_sessions(&self) -> Vec<SessionDto> {
        let sessions = self.registry.list().await;
        sessions.iter().map(SessionDto::from).collect()
    }

    /// Return a single session by session_id.
    async fn get_session(&self, session_id: String) -> fdo::Result<SessionDto> {
        match self.registry.get(&session_id).await {
            Some(s) => Ok(SessionDto::from(&s)),
            None => Err(fdo::Error::Failed(format!(
                "Session '{session_id}' not found"
            ))),
        }
    }

    // --- Signals ---

    #[zbus(signal, name = "AggregateStateChanged")]
    pub async fn emit_aggregate_state_changed(
        signal_ctxt: &SignalContext<'_>,
        state: &str,
        active_session_count: u32,
        waiting_session_count: u32,
        error_session_count: u32,
        updated_at: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "SessionAdded")]
    pub async fn emit_session_added(
        signal_ctxt: &SignalContext<'_>,
        session: &SessionDto,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "SessionUpdated")]
    pub async fn emit_session_updated(
        signal_ctxt: &SignalContext<'_>,
        session: &SessionDto,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "SessionRemoved")]
    pub async fn emit_session_removed(
        signal_ctxt: &SignalContext<'_>,
        session_id: &str,
    ) -> zbus::Result<()>;

    // --- Properties ---

    #[zbus(property, name = "AggregateState")]
    async fn aggregate_state(&self) -> String {
        self.aggregate.read().await.state.clone()
    }

    #[zbus(property, name = "ActiveSessionCount")]
    async fn active_session_count(&self) -> u32 {
        self.aggregate.read().await.active_session_count
    }

    #[zbus(property, name = "DaemonVersion")]
    fn daemon_version(&self) -> &str {
        self.version
    }
}
