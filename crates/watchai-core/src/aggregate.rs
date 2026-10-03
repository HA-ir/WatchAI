use chrono::{DateTime, Duration, Utc};

use crate::session::AgentSession;
use crate::state::LifecycleState;

/// Duration in seconds that terminal states (SUCCESS, CANCELLED, ERROR)
/// participate in the top-bar aggregate state before resetting to IDLE.
pub const COMPLETION_DWELL_SECONDS: i64 = 10;

/// Output of pure desktop state aggregation calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregateCalculation {
    /// The computed desktop aggregate state.
    pub aggregate_state: LifecycleState,
    /// Count of active or dwelling sessions.
    pub active_session_count: u32,
    /// Count of sessions in WAITING.
    pub waiting_session_count: u32,
    /// Count of sessions in ERROR (regardless of dwell).
    pub error_session_count: u32,
    /// Contextually focused session ID for single-session inspection.
    pub focused_session_id: Option<String>,
}

/// Evaluates whether a session is currently within its 10-second completion dwell window.
pub fn within_dwell(session: &AgentSession, now: DateTime<Utc>) -> bool {
    if !session.current_state.is_terminal() {
        return false;
    }

    if now < session.state_entered_at {
        // Clock skew / entered in immediate future: treat as within dwell
        return true;
    }

    (now - session.state_entered_at) <= Duration::seconds(COMPLETION_DWELL_SECONDS)
}

/// Computes the effective priority score for aggregation at evaluation time `now`.
///
/// Priority order:
/// ERROR (80) > WAITING (70) > WORKING (60) > STARTING (50) > CANCELLED (40) > SUCCESS (30) > UNKNOWN (20) > IDLE (10)
/// Terminal states past their 10-second dwell window return priority 0.
pub fn effective_priority(session: &AgentSession, now: DateTime<Utc>) -> u8 {
    match session.current_state {
        LifecycleState::Error => {
            if within_dwell(session, now) {
                80
            } else {
                0
            }
        }
        LifecycleState::Waiting => 70,
        LifecycleState::Working => 60,
        LifecycleState::Starting => 50,
        LifecycleState::Cancelled => {
            if within_dwell(session, now) {
                40
            } else {
                0
            }
        }
        LifecycleState::Success => {
            if within_dwell(session, now) {
                30
            } else {
                0
            }
        }
        LifecycleState::Unknown => 20,
        LifecycleState::Idle => 10,
    }
}

/// Checks if a session counts towards `active_session_count`.
///
/// True for non-terminal states (STARTING, WORKING, WAITING, UNKNOWN)
/// and terminal states (SUCCESS, CANCELLED, ERROR) currently within their 10s dwell.
pub fn is_active_session(session: &AgentSession, now: DateTime<Utc>) -> bool {
    match session.current_state {
        LifecycleState::Starting
        | LifecycleState::Working
        | LifecycleState::Waiting
        | LifecycleState::Unknown => true,
        LifecycleState::Success | LifecycleState::Cancelled | LifecycleState::Error => {
            within_dwell(session, now)
        }
        LifecycleState::Idle => false,
    }
}

/// Comparator for deterministic contextual session selection:
/// 1. Higher effective priority wins.
/// 2. Later (more recent) `state_entered_at` wins.
/// 3. Lexicographically smaller `session_id` wins.
pub fn compare_contextual_priority(
    a: &AgentSession,
    b: &AgentSession,
    now: DateTime<Utc>,
) -> std::cmp::Ordering {
    let prio_a = effective_priority(a, now);
    let prio_b = effective_priority(b, now);

    prio_a
        .cmp(&prio_b)
        .then_with(|| a.state_entered_at.cmp(&b.state_entered_at))
        .then_with(|| b.session_id.cmp(&a.session_id)) // inverted so smaller session_id compares Greater
}

/// Computes the desktop aggregate state, session counters, and contextual focus
/// for a snapshot of observed agent sessions at timestamp `now`.
///
/// This function is deterministic and strictly side-effect free.
pub fn compute_aggregate_state(
    sessions: &[AgentSession],
    now: DateTime<Utc>,
) -> AggregateCalculation {
    if sessions.is_empty() {
        return AggregateCalculation {
            aggregate_state: LifecycleState::Idle,
            active_session_count: 0,
            waiting_session_count: 0,
            error_session_count: 0,
            focused_session_id: None,
        };
    }

    let mut active_count = 0u32;
    let mut waiting_count = 0u32;
    let mut error_count = 0u32;

    for s in sessions {
        if is_active_session(s, now) {
            active_count += 1;
        }
        if s.current_state == LifecycleState::Waiting {
            waiting_count += 1;
        }
        if s.current_state == LifecycleState::Error {
            error_count += 1;
        }
    }

    // Select the contextually dominant session
    let best_session = sessions
        .iter()
        .max_by(|a, b| compare_contextual_priority(a, b, now));

    let (aggregate_state, focused_session_id) = match best_session {
        Some(s) => {
            if effective_priority(s, now) == 0 {
                // All sessions are terminal and expired past dwell
                (LifecycleState::Idle, None)
            } else {
                (s.current_state, Some(s.session_id.clone()))
            }
        }
        None => (LifecycleState::Idle, None),
    };

    AggregateCalculation {
        aggregate_state,
        active_session_count: active_count,
        waiting_session_count: waiting_count,
        error_session_count: error_count,
        focused_session_id,
    }
}
