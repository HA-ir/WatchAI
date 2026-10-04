use crate::discovery::ProcessScanner;
use crate::traits::{DiscoveredSession, EventSink, ProviderAdapter, ProviderCapabilities};
use async_trait::async_trait;
use std::sync::RwLock;
use tracing::debug;
use watchai_core::session::AdapterStatus;

/// Provider adapter for Anthropic's Claude Code CLI agent.
pub struct ClaudeCodeAdapter {
    event_sink: RwLock<Option<EventSink>>,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self {
            event_sink: RwLock::new(None),
        }
    }

    /// Retrieve the attached EventSink if present.
    pub fn event_sink(&self) -> Option<EventSink> {
        self.event_sink.read().ok().and_then(|g| g.clone())
    }
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderAdapter for ClaudeCodeAdapter {
    fn provider_id(&self) -> &'static str {
        "claude-code"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::process_discovery_only()
    }

    fn attach_event_sink(&self, sink: EventSink) {
        if let Ok(mut guard) = self.event_sink.write() {
            *guard = Some(sink);
        }
    }

    fn event_sink(&self) -> Option<EventSink> {
        self.event_sink.read().ok().and_then(|g| g.clone())
    }

    async fn check_environment(&self) -> AdapterStatus {
        // Check if `claude` binary is in PATH
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                if dir.join("claude").is_file() {
                    return AdapterStatus::Active;
                }
            }
        }

        // Also check default npm/homebrew paths
        if let Ok(home) = std::env::var("HOME") {
            let home_bin = std::path::Path::new(&home).join(".local/bin/claude");
            if home_bin.is_file() {
                return AdapterStatus::Active;
            }
        }

        debug!("Claude Code binary not found in standard paths");
        AdapterStatus::DiscoveryRequired
    }

    async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
        // Use non-invasive /proc scanner targeting "claude"
        ProcessScanner::scan_processes("claude", self.provider_id(), self.display_name())
    }
}
