use serde::{Deserialize, Serialize};
use watchai_core::session::AgentSession;
use zbus::zvariant::{OwnedValue, Type, Value};

/// D-Bus tuple representation of an individual session matching `(sssssssus)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct SessionDto {
    pub session_id: String,
    pub provider_id: String,
    pub provider_display_name: String,
    pub project_name: String,
    pub current_state: String,
    pub started_at: String,
    pub state_entered_at: String,
    pub process_id: u32,
    pub active_tool_category: String,
}

impl From<&AgentSession> for SessionDto {
    fn from(s: &AgentSession) -> Self {
        Self {
            session_id: s.session_id.clone(),
            provider_id: s.provider_id.clone(),
            provider_display_name: s.provider_display_name.clone(),
            project_name: s.project_name.clone(),
            current_state: s.current_state.to_string(),
            started_at: s.started_at.to_rfc3339(),
            state_entered_at: s.state_entered_at.to_rfc3339(),
            process_id: s.process_id.unwrap_or(0),
            active_tool_category: s
                .active_tool_category
                .as_ref()
                .map(|c| c.as_str().to_string())
                .unwrap_or_default(),
        }
    }
}

/// D-Bus tuple representation of the desktop aggregate state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Value, OwnedValue)]
pub struct AggregateStateDto {
    pub state: String,
    pub active_session_count: u32,
    pub waiting_session_count: u32,
    pub error_session_count: u32,
    pub updated_at: String,
}
