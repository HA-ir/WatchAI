use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;
use watchai_core::aggregate::compute_aggregate_state;
use watchai_core::session::SessionRegistry;
use watchai_ipc::protocol::AggregateStateDto;

/// Recomputes desktop aggregate state and updates the D-Bus aggregate DTO if changed.
///
/// Returns `Some(new_dto)` if the aggregate state or any of the counters changed
/// (signaling that an AggregateStateChanged D-Bus emission is required).
/// Returns `None` if the state and counters are identical (throttled / suppressed).
pub async fn sync_aggregate_state(
    registry: &SessionRegistry,
    aggregate_lock: &Arc<RwLock<AggregateStateDto>>,
    now: DateTime<Utc>,
) -> Option<AggregateStateDto> {
    let sessions = registry.list().await;
    let calc = compute_aggregate_state(&sessions, now);

    let new_dto = AggregateStateDto {
        state: calc.aggregate_state.to_string(),
        active_session_count: calc.active_session_count,
        waiting_session_count: calc.waiting_session_count,
        error_session_count: calc.error_session_count,
        updated_at: now.to_rfc3339(),
    };

    let changed = {
        let current = aggregate_lock.read().await;
        current.state != new_dto.state
            || current.active_session_count != new_dto.active_session_count
            || current.waiting_session_count != new_dto.waiting_session_count
            || current.error_session_count != new_dto.error_session_count
    };

    if changed {
        let mut write_guard = aggregate_lock.write().await;
        *write_guard = new_dto.clone();
        Some(new_dto)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use watchai_core::session::{AdapterStatus, AgentSession};
    use watchai_core::state::LifecycleState;

    #[tokio::test]
    async fn test_signal_throttling_suppresses_duplicate_emissions() {
        let registry = SessionRegistry::new();
        let initial_dto = AggregateStateDto {
            state: "IDLE".to_string(),
            active_session_count: 0,
            waiting_session_count: 0,
            error_session_count: 0,
            updated_at: Utc::now().to_rfc3339(),
        };
        let agg_lock = Arc::new(RwLock::new(initial_dto));
        let now = Utc::now();

        // 1. Initial check with empty registry: matches IDLE (all 0) -> no change
        let res = sync_aggregate_state(&registry, &agg_lock, now).await;
        assert!(res.is_none(), "Must suppress duplicate initial emission");

        // 2. Add a WORKING session -> state changes to WORKING -> must emit!
        let s1 = AgentSession::new(
            "s1".to_string(),
            "claude-code",
            "Claude Code",
            "/home/user/project",
            Some(100),
            LifecycleState::Working,
            AdapterStatus::Active,
        );
        registry.upsert(s1).await;

        let res2 = sync_aggregate_state(&registry, &agg_lock, now).await;
        assert!(res2.is_some(), "State change must trigger emission");
        let dto2 = res2.unwrap();
        assert_eq!(dto2.state, "WORKING");
        assert_eq!(dto2.active_session_count, 1);

        // 3. Re-run sync with identical session state -> must suppress emission!
        let res3 = sync_aggregate_state(&registry, &agg_lock, now + Duration::seconds(1)).await;
        assert!(
            res3.is_none(),
            "Redundant tick without state/counter change must be throttled"
        );
    }
}
