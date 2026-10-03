use chrono::{DateTime, Utc};
use std::fs;
use std::io::Error;
use std::path::Path;
use tracing::{debug, warn};

use crate::session::{AgentSession, SessionRegistry};
use crate::state::LifecycleState;

/// Fixed interval in seconds between periodic background liveness checks.
pub const LIVENESS_CHECK_INTERVAL_SECONDS: u64 = 2;

/// Silence timeout for active unmonitored sessions in WORKING before transitioning to UNKNOWN.
pub const SILENCE_TIMEOUT_WORKING_SECONDS: i64 = 300;

/// Silence timeout for active unmonitored sessions in STARTING before transitioning to UNKNOWN.
pub const SILENCE_TIMEOUT_STARTING_SECONDS: i64 = 60;

/// Duration in seconds that terminal states (SUCCESS, CANCELLED, ERROR)
/// are retained in memory before being pruned from the SessionRegistry.
pub const RETENTION_WINDOW_SECONDS: i64 = 60;

/// Abstraction for reading `/proc/[pid]/stat` to enable deterministic unit testing.
pub trait ProcStatReader: Send + Sync {
    /// Read the `/proc/[pid]/stat` file content for a given PID.
    fn read_stat(&self, pid: u32) -> Result<String, Error>;
}

/// Production implementation reading real Linux `/proc/[pid]/stat`.
pub struct RealProcStatReader;

impl ProcStatReader for RealProcStatReader {
    fn read_stat(&self, pid: u32) -> Result<String, Error> {
        let path = format!("/proc/{}/stat", pid);
        fs::read_to_string(Path::new(&path))
    }
}

/// Reason why an agent process was declared dead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadReason {
    /// Two consecutive /proc read failures confirmed process disappearance.
    ProcessTerminated { consecutive_failures: u32 },
    /// Process start time differed from recorded start time, indicating PID recycling.
    PidReused {
        recorded_start_time: u64,
        current_start_time: u64,
    },
}

/// Outcome of checking an individual session's liveness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LivenessCheckResult {
    /// Process exists and start time matches recorded identity.
    Alive,
    /// A single read failure was recorded; failure counter incremented to 1 (tolerated).
    TransientFailure { failures: u32 },
    /// Process is confirmed dead; session transitioned to ERROR.
    Dead { reason: DeadReason },
    /// Session has no associated PID to monitor via /proc.
    NotMonitored,
}

/// Parses the process start time (field 22, in clock ticks since boot)
/// from the raw text content of `/proc/[pid]/stat`.
///
/// Handles arbitrary `comm` values containing spaces and nested parentheses
/// by scanning from the rightmost `)`.
pub fn parse_proc_stat_starttime(stat_content: &str) -> Option<u64> {
    let close_paren_idx = stat_content.rfind(')')?;
    let rest = &stat_content[close_paren_idx + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // starttime is the 20th field after comm (field 22 overall, 0-indexed as index 19)
    fields.get(19).and_then(|val| val.parse::<u64>().ok())
}

/// Verifies OS process liveness for a single session using the provided stat reader.
///
/// Implements PID reuse protection and 2-consecutive-check failure hysteresis:
/// - If reading stat fails or is unparseable: increments failure counter; at 2 consecutive failures, transitions to ERROR.
/// - If start time differs from recorded start time: immediately transitions to ERROR.
/// - If successful: resets failure counter to 0 and updates last_seen_at.
pub fn check_session_liveness<R: ProcStatReader>(
    session: &mut AgentSession,
    reader: &R,
) -> LivenessCheckResult {
    let pid = match session.process_id {
        Some(p) => p,
        None => return LivenessCheckResult::NotMonitored,
    };

    match reader.read_stat(pid) {
        Ok(stat_content) => {
            let current_start_time = match parse_proc_stat_starttime(&stat_content) {
                Some(st) => st,
                None => {
                    session.consecutive_proc_failures += 1;
                    if session.consecutive_proc_failures >= 2 {
                        let _ = session.transition_to(
                            LifecycleState::Error,
                            session.sequence_number + 1,
                            None,
                        );
                        return LivenessCheckResult::Dead {
                            reason: DeadReason::ProcessTerminated {
                                consecutive_failures: session.consecutive_proc_failures,
                            },
                        };
                    } else {
                        return LivenessCheckResult::TransientFailure {
                            failures: session.consecutive_proc_failures,
                        };
                    }
                }
            };

            // Check for PID reuse
            if let Some(recorded) = session.process_start_time {
                if recorded != current_start_time {
                    warn!(
                        "PID reuse detected for session {}: recorded starttime {}, live starttime {}",
                        session.session_id, recorded, current_start_time
                    );
                    let _ = session.transition_to(
                        LifecycleState::Error,
                        session.sequence_number + 1,
                        None,
                    );
                    return LivenessCheckResult::Dead {
                        reason: DeadReason::PidReused {
                            recorded_start_time: recorded,
                            current_start_time,
                        },
                    };
                }
            } else {
                session.process_start_time = Some(current_start_time);
            }

            session.consecutive_proc_failures = 0;
            session.touch_proc_liveness();
            LivenessCheckResult::Alive
        }
        Err(e) => {
            session.consecutive_proc_failures += 1;
            debug!(
                "Failed to read stat for PID {} (failure {}/2): {}",
                pid, session.consecutive_proc_failures, e
            );

            if session.consecutive_proc_failures >= 2 {
                let _ =
                    session.transition_to(LifecycleState::Error, session.sequence_number + 1, None);
                LivenessCheckResult::Dead {
                    reason: DeadReason::ProcessTerminated {
                        consecutive_failures: session.consecutive_proc_failures,
                    },
                }
            } else {
                LivenessCheckResult::TransientFailure {
                    failures: session.consecutive_proc_failures,
                }
            }
        }
    }
}

/// Checks adaptive silence timeout for unmonitored sessions.
/// Transitions unresponsive WORKING (>300s) or STARTING (>60s) sessions to UNKNOWN.
pub fn check_silence_timeout(session: &mut AgentSession, now: DateTime<Utc>) -> bool {
    let threshold_secs = match session.current_state {
        LifecycleState::Working => SILENCE_TIMEOUT_WORKING_SECONDS,
        LifecycleState::Starting => SILENCE_TIMEOUT_STARTING_SECONDS,
        _ => return false,
    };

    let elapsed = (now - session.last_seen_at).num_seconds();
    if elapsed > threshold_secs {
        let _ = session.transition_to(LifecycleState::Unknown, session.sequence_number + 1, None);
        true
    } else {
        false
    }
}

/// Checks if a session has exceeded its 60-second terminal retention window.
pub fn is_retention_expired(session: &AgentSession, now: DateTime<Utc>) -> bool {
    if !session.current_state.is_terminal() {
        return false;
    }
    (now - session.state_entered_at).num_seconds() >= RETENTION_WINDOW_SECONDS
}

/// Prunes expired terminal sessions from the registry and returns their session IDs.
pub async fn prune_retained_sessions(
    registry: &SessionRegistry,
    now: DateTime<Utc>,
) -> Vec<String> {
    let sessions = registry.list().await;
    let mut pruned = Vec::new();
    for s in sessions {
        if is_retention_expired(&s, now) && registry.remove(&s.session_id).await.is_some() {
            pruned.push(s.session_id);
        }
    }
    pruned
}

/// Summary of state changes discovered during a liveness cycle.
#[derive(Debug, Default)]
pub struct LivenessCycleOutcome {
    /// Sessions that transitioned state or changed metadata.
    pub updated_sessions: Vec<AgentSession>,
    /// Sessions that were confirmed dead.
    pub dead_sessions: Vec<AgentSession>,
}

/// Executes a complete liveness and silence check cycle across all registered sessions.
pub async fn run_liveness_cycle<R: ProcStatReader>(
    registry: &SessionRegistry,
    reader: &R,
    now: DateTime<Utc>,
) -> LivenessCycleOutcome {
    let sessions = registry.list().await;
    let mut outcome = LivenessCycleOutcome::default();

    for mut session in sessions {
        let initial_state = session.current_state;
        let initial_failures = session.consecutive_proc_failures;

        // Terminal sessions (SUCCESS, CANCELLED, ERROR) have completed:
        // Do NOT run liveness or silence checks on them. They remain retained in their terminal
        // state until pruned by `prune_retained_sessions`.
        if !session.current_state.is_terminal() {
            // 1. Process survival check if PID is present
            if session.process_id.is_some() {
                let res = check_session_liveness(&mut session, reader);
                if let LivenessCheckResult::Dead { .. } = res {
                    outcome.dead_sessions.push(session.clone());
                }
            }

            // 2. Activity silence timeout check:
            // Crucial invariant: /proc process presence is NOT activity telemetry.
            // If telemetry has stopped for >300s (WORKING) or >60s (STARTING),
            // the session transitions to UNKNOWN regardless of whether its PID is still running.
            if !session.current_state.is_terminal() {
                check_silence_timeout(&mut session, now);
            }
        }

        if session.current_state != initial_state
            || session.consecutive_proc_failures != initial_failures
        {
            registry.upsert(session.clone()).await;
            if session.current_state != initial_state {
                outcome.updated_sessions.push(session);
            }
        }
    }

    outcome
}
