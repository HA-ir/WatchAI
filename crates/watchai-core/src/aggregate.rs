use chrono::{DateTime, Duration, Utc};

use crate::session::AgentSession;
use crate::state::LifecycleState;

/// Duration in seconds that terminal states (SUCCESS, CANCELLED, ERROR)
/// participate in the top-bar aggregate state before resetting to IDLE.
pub const COMPLETION_DWELL_SECONDS: i64 = 60;

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

/// Evaluates whether `candidate` is strictly preferred over `current_best`
/// according to the deterministic contextual ordering:
/// 1. Higher effective priority wins.
/// 2. Later (more recent) `state_entered_at` wins.
/// 3. Lexicographically smaller `session_id` wins (e.g. "alpha" beats "beta").
pub fn is_preferred(
    candidate: &AgentSession,
    current_best: &AgentSession,
    now: DateTime<Utc>,
) -> bool {
    let prio_cand = effective_priority(candidate, now);
    let prio_best = effective_priority(current_best, now);

    if prio_cand != prio_best {
        return prio_cand > prio_best;
    }

    if candidate.state_entered_at != current_best.state_entered_at {
        return candidate.state_entered_at > current_best.state_entered_at;
    }

    candidate.session_id < current_best.session_id
}

/// Comparator for deterministic contextual session selection:
/// Returns `Ordering::Greater` if `a` is preferred over `b`,
/// `Ordering::Less` if `b` is preferred over `a`,
/// and `Ordering::Equal` if they have identical attributes and session_id.
pub fn compare_contextual_priority(
    a: &AgentSession,
    b: &AgentSession,
    now: DateTime<Utc>,
) -> std::cmp::Ordering {
    if a.session_id == b.session_id
        && a.state_entered_at == b.state_entered_at
        && effective_priority(a, now) == effective_priority(b, now)
    {
        std::cmp::Ordering::Equal
    } else if is_preferred(a, b, now) {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Less
    }
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

    // Select the contextually dominant session in an input-order-independent manner
    let mut best_session: Option<&AgentSession> = None;
    for s in sessions {
        match best_session {
            None => best_session = Some(s),
            Some(cur) => {
                if is_preferred(s, cur, now) {
                    best_session = Some(s);
                }
            }
        }
    }

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
