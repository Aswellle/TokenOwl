pub mod builtin;
pub mod manifest;
pub mod registry;

// Re-export ProviderAdapter trait and related types for consumers
pub use crate::domain::provider::{
    AccountContext, AccountIdentity, FetchContext, HealthStatus, ProviderAdapter,
    ProviderDescriptor, QuotaSnapshotSet, UsagePoint, UsageRange,
};
