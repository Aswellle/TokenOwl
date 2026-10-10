// Provider registry and lookup
use crate::domain::provider::{ProviderAdapter, ProviderDescriptor};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for provider adapters.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn ProviderAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider adapter, replacing any existing adapter with the same ID.
    pub fn register(
        &mut self,
        adapter: Arc<dyn ProviderAdapter>,
    ) -> Option<Arc<dyn ProviderAdapter>> {
        let id = adapter.descriptor().id.clone();
        self.adapters.insert(id, adapter)
    }

    /// Look up a provider adapter by ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn ProviderAdapter>> {
        self.adapters.get(id).cloned()
    }

    /// Remove a provider adapter by ID.
    pub fn remove(&mut self, id: &str) -> Option<Arc<dyn ProviderAdapter>> {
        self.adapters.remove(id)
    }

    /// Returns the number of registered adapters.
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Returns true if no adapters are registered.
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Iterate over all registered adapters.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &dyn ProviderAdapter)> {
        self.adapters
            .iter()
            .map(|(id, adapter)| (id.as_str(), adapter.as_ref()))
    }

    /// Look up a descriptor by ID without needing the full adapter instance.
    pub fn descriptor(&self, id: &str) -> Option<ProviderDescriptor> {
        self.adapters.get(id).map(|a| a.descriptor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::HealthStatus;

    struct MockAdapter;

    #[async_trait::async_trait]
    impl ProviderAdapter for MockAdapter {
        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor {
                id: "mock".to_owned(),
                display_name: "Mock Provider".to_owned(),
                ..ProviderDescriptor::default()
            }
        }

        async fn validate_account(
            &self,
            _account: &crate::domain::provider::AccountContext,
        ) -> crate::domain::provider::ProviderResult<crate::domain::provider::AccountIdentity>
        {
            unimplemented!()
        }

        async fn fetch_quota(
            &self,
            _account: &crate::domain::provider::AccountContext,
            _ctx: &crate::domain::provider::FetchContext,
        ) -> crate::domain::provider::ProviderResult<crate::domain::provider::QuotaSnapshotSet>
        {
            unimplemented!()
        }

        async fn fetch_usage(
            &self,
            _account: &crate::domain::provider::AccountContext,
            _range: crate::domain::provider::UsageRange,
            _ctx: &crate::domain::provider::FetchContext,
        ) -> crate::domain::provider::ProviderResult<Vec<crate::domain::provider::UsagePoint>>
        {
            unimplemented!()
        }

        async fn health_check(
            &self,
            _account: &crate::domain::provider::AccountContext,
            _ctx: &crate::domain::provider::FetchContext,
        ) -> crate::domain::provider::ProviderResult<HealthStatus> {
            Ok(HealthStatus::Healthy)
        }
    }

    #[test]
    fn register_and_lookup_adapter() {
        let mut registry = AdapterRegistry::new();
        let adapter: Arc<dyn ProviderAdapter> = Arc::new(MockAdapter);
        assert!(registry.register(adapter).is_none());
        assert_eq!(registry.len(), 1);
        assert!(registry.get("mock").is_some());
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn register_replaces_existing_adapter() {
        let mut registry = AdapterRegistry::new();
        let adapter1: Arc<dyn ProviderAdapter> = Arc::new(MockAdapter);
        let adapter2: Arc<dyn ProviderAdapter> = Arc::new(MockAdapter);
        registry.register(adapter1);
        assert!(registry.register(adapter2).is_some());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn remove_adapter() {
        let mut registry = AdapterRegistry::new();
        let adapter: Arc<dyn ProviderAdapter> = Arc::new(MockAdapter);
        registry.register(adapter);
        assert!(registry.remove("mock").is_some());
        assert!(registry.is_empty());
    }

    #[test]
    fn descriptor_lookup_without_full_adapter() {
        let mut registry = AdapterRegistry::new();
        let adapter: Arc<dyn ProviderAdapter> = Arc::new(MockAdapter);
        registry.register(adapter);
        let desc = registry.descriptor("mock").expect("descriptor");
        assert_eq!(desc.id, "mock");
        assert_eq!(desc.display_name, "Mock Provider");
    }
}
