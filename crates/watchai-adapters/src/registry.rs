use crate::claude_code::ClaudeCodeAdapter;
use crate::codex_cli::CodexCliAdapter;
use crate::opencode::OpenCodeAdapter;
use crate::traits::{DiscoveredSession, EventSink, ProviderAdapter};
use std::sync::Arc;

/// Catalog of active provider adapters, completely decoupling the daemon binary from concrete adapters.
#[derive(Default, Clone)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn ProviderAdapter>>,
    event_sink: Option<EventSink>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
            event_sink: None,
        }
    }

    /// Register a provider adapter into the catalog.
    pub fn register(&mut self, adapter: Arc<dyn ProviderAdapter>) {
        if let Some(sink) = &self.event_sink {
            adapter.attach_event_sink(sink.clone());
        }
        self.adapters.push(adapter);
    }

    /// Construct a registry pre-populated with default built-in adapters.
    pub fn default_registry() -> Self {
        let mut reg = Self::new();
        reg.register(Arc::new(ClaudeCodeAdapter::new()));
        reg.register(Arc::new(CodexCliAdapter::new()));
        reg.register(Arc::new(OpenCodeAdapter::new()));
        reg
    }

    /// Attach an EventSink handle to all registered adapters.
    pub fn attach_event_sink(&mut self, sink: EventSink) {
        for adapter in &self.adapters {
            adapter.attach_event_sink(sink.clone());
        }
        self.event_sink = Some(sink);
    }

    /// Retrieve the attached EventSink if available.
    pub fn event_sink(&self) -> Option<&EventSink> {
        self.event_sink.as_ref()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use watchai_core::session::AdapterStatus;

    struct TestRecordingAdapter {
        attached: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl ProviderAdapter for TestRecordingAdapter {
        fn provider_id(&self) -> &'static str {
            "test-recording"
        }
        fn display_name(&self) -> &'static str {
            "Test Recording"
        }
        fn attach_event_sink(&self, _sink: EventSink) {
            self.attached.store(true, Ordering::SeqCst);
        }
        async fn check_environment(&self) -> AdapterStatus {
            AdapterStatus::Active
        }
        async fn discover_sessions(&self) -> Vec<DiscoveredSession> {
            Vec::new()
        }
    }

    #[test]
    fn test_registry_distributes_event_sink_to_adapters() {
        let mut registry = AdapterRegistry::new();
        let flag = Arc::new(AtomicBool::new(false));
        let adapter = Arc::new(TestRecordingAdapter {
            attached: flag.clone(),
        });
        registry.register(adapter);

        assert!(!flag.load(Ordering::SeqCst));

        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let sink = EventSink::new(tx, None);
        registry.attach_event_sink(sink);

        assert!(flag.load(Ordering::SeqCst));

        // Register adapter post-attach
        let post_flag = Arc::new(AtomicBool::new(false));
        let post_adapter = Arc::new(TestRecordingAdapter {
            attached: post_flag.clone(),
        });
        registry.register(post_adapter);

        assert!(post_flag.load(Ordering::SeqCst));
    }
}
