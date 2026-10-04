use crate::discovery::ProcessScanner;
use crate::traits::{DiscoveredSession, ProviderAdapter, ProviderCapabilities};
use async_trait::async_trait;
use tracing::debug;
use watchai_core::session::AdapterStatus;

/// Provider adapter for OpenAI Codex CLI agent.
pub struct CodexCliAdapter;

impl CodexCliAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodexCliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderAdapter for CodexCliAdapter {
    fn provider_id(&self) -> &'static str {
        "codex-cli"
    }

    fn display_name(&self) -> &'static str {
        "OpenAI Codex"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::process_discovery_only()
    }

    async fn check_environment(&self) -> AdapterStatus {
        // Check if `codex` or `codex-cli` binary is in PATH
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                if dir.join("codex").is_file() || dir.join("codex-cli").is_file() {
                    return AdapterStatus::Active;
                }
            }
        }

        // Also check default local user bin paths
        if let Ok(home) = std::env::var("HOME") {
            let home_bin = std::path::Path::new(&home).join(".local/bin");
            if home_bin.join("codex").is_file() || home_bin.join("codex-cli").is_file() {
                return AdapterStatus::Active;
            }
        }

        debug!("Codex CLI binary not found in standard paths");
        AdapterStatus::DiscoveryRequired
    }

    async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
        ProcessScanner::scan_processes_multi(
            &["codex", "codex-cli"],
            self.provider_id(),
            self.display_name(),
        )
    }
}
