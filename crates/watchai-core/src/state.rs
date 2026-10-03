use serde::{Deserialize, Serialize};
use std::fmt;

/// The finite set of valid lifecycle states for an observed AI coding agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LifecycleState {
    /// No active task processing; session is at an interactive prompt or unmonitored.
    Idle,
    /// Process initialized, bootstrapping, or loading context.
    Starting,
    /// Actively processing, executing tools, or generating code.
    Working,
    /// Blocked mid-task awaiting user permission, input, or tool approval.
    Waiting,
    /// Task execution completed successfully (dwells briefly before settling to Idle).
    Success,
    /// Session stopped or failed due to an error.
    Error,
    /// Session or active task was cancelled/interrupted by user.
    Cancelled,
    /// State cannot be reliably determined (telemetry dropped or unverified).
    Unknown,
}

impl LifecycleState {
    /// Return the canonical string identifier for IPC and logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Starting => "STARTING",
            Self::Working => "WORKING",
            Self::Waiting => "WAITING",
            Self::Success => "SUCCESS",
            Self::Error => "ERROR",
            Self::Cancelled => "CANCELLED",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Return the priority score used for aggregate status computation.
    ///
    /// Priority order: ERROR > WAITING > WORKING > STARTING > CANCELLED > SUCCESS > UNKNOWN > IDLE
    pub fn priority_score(&self) -> u8 {
        match self {
            Self::Error => 80,
            Self::Waiting => 70,
            Self::Working => 60,
            Self::Starting => 50,
            Self::Cancelled => 40,
            Self::Success => 30,
            Self::Unknown => 20,
            Self::Idle => 10,
        }
    }

    /// Check if transitioning from `self` to `target` is a valid FSM edge.
    pub fn can_transition_to(&self, target: LifecycleState) -> bool {
        if *self == target {
            return true; // Self-transitions (heartbeats / updates) are permitted.
        }

        match self {
            Self::Idle => matches!(target, Self::Starting | Self::Working | Self::Unknown),
            Self::Starting => matches!(
                target,
                Self::Working
                    | Self::Idle
                    | Self::Waiting
                    | Self::Error
                    | Self::Cancelled
                    | Self::Unknown
            ),
            Self::Working => matches!(
                target,
                Self::Waiting | Self::Success | Self::Error | Self::Cancelled | Self::Unknown
            ),
            Self::Waiting => matches!(
                target,
                Self::Working | Self::Cancelled | Self::Error | Self::Unknown
            ),
            Self::Success => matches!(
                target,
                Self::Working | Self::Idle | Self::Cancelled | Self::Starting
            ),
            Self::Error => matches!(target, Self::Starting | Self::Working | Self::Idle),
            Self::Cancelled => matches!(target, Self::Starting | Self::Working | Self::Idle),
            Self::Unknown => matches!(
                target,
                Self::Starting
                    | Self::Working
                    | Self::Waiting
                    | Self::Idle
                    | Self::Error
                    | Self::Cancelled
            ),
        }
    }

    /// Return true if this state is terminal (completed task/run).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success | Self::Error | Self::Cancelled)
    }
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for LifecycleState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "IDLE" => Ok(Self::Idle),
            "STARTING" => Ok(Self::Starting),
            "WORKING" => Ok(Self::Working),
            "WAITING" => Ok(Self::Waiting),
            "SUCCESS" => Ok(Self::Success),
            "ERROR" => Ok(Self::Error),
            "CANCELLED" => Ok(Self::Cancelled),
            "UNKNOWN" => Ok(Self::Unknown),
            other => Err(format!("Unrecognized lifecycle state: '{other}'")),
        }
    }
}
