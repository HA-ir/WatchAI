use crate::claude_code::ClaudeCodeAdapter;
use crate::traits::{DiscoveredSession, ProviderAdapter};
use std::sync::Arc;

/// Catalog of active provider adapters, completely decoupling the daemon binary from concrete adapters.
#[derive(Default, Clone)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn ProviderAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Register a provider adapter into the catalog.
    pub fn register(&mut self, adapter: Arc<dyn ProviderAdapter>) {
        self.adapters.push(adapter);
    }

    /// Construct a registry pre-populated with default built-in adapters.
    pub fn default_registry() -> Self {
        let mut reg = Self::new();
        reg.register(Arc::new(ClaudeCodeAdapter::new()));
        reg
    }

    /// Run discovery across all registered provider adapters and collect active sessions.
    pub async fn discover_all(&self) -> Vec<DiscoveredSession> {
        let mut all_sessions = Vec::new();
        for adapter in &self.adapters {
            let found = adapter.discover_sessions().await;
            all_sessions.extend(found);
        }
        all_sessions
    }

    /// Return the list of registered adapters.
    pub fn adapters(&self) -> &[Arc<dyn ProviderAdapter>] {
        &self.adapters
    }
}
