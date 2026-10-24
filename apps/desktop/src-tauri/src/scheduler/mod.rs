//! Provider refresh: one account, one adapter dispatch, one persistence path.

use crate::credentials::CredentialStore;
use crate::domain::provider::{ProviderError, ProviderErrorKind};
use crate::domain::state::{BackoffPolicy, RefreshState, retry_after_delay};
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::storage::{LocalRepository, StorageError};

/// Error type for refresh operations.
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
}

/// Result of a successful refresh.
#[derive(Debug, Clone)]
pub struct RefreshResult {
    pub state: RefreshState,
    pub snapshot_count: usize,
    pub observed_at: Option<u64>,
}

/// Refresh a single account and persist the normalized snapshot set.
///
/// Adapter dispatch follows the wire protocol the account was created under:
/// ids prefixed `anthropic:` ride the native adapter, everything else the
/// OpenAI-compatible transport.
pub async fn refresh_account(
    repository: LocalRepository,
    adapter: &OpenAiCompatibleAdapter,
    anthropic: &crate::providers::builtin::anthropic::AnthropicAdapter,
    google: &crate::providers::builtin::google::GoogleAiAdapter,
    credentials: &CredentialStore,
    account_id: &str,
    started_at: u64,
) -> Result<RefreshResult, RefreshError> {
    let account = repository
        .account(account_id)?
        .ok_or_else(|| ProviderError::new(ProviderErrorKind::ConfigInvalid, "account not found"))?;
    let config = repository.provider(&account.provider_id)?.ok_or_else(|| {
        ProviderError::new(ProviderErrorKind::ConfigInvalid, "provider not found")
    })?;
    repository.mark_refresh_started(account_id, started_at)?;
    let fetched = if account.provider_id.starts_with("anthropic:") {
        native_fetch(anthropic, &account, &config, account_id, credentials).await
    } else if account.provider_id.starts_with("gemini:") {
        native_fetch(google, &account, &config, account_id, credentials).await
    } else {
        adapter.fetch(&config, account_id, credentials).await
    };
    match fetched {
        Ok(result) => {
            repository.upsert_snapshot_set(account_id, &result.snapshots, result.observed_at)?;
            let state = repository.mark_refresh_succeeded(account_id, result.observed_at)?;
            Ok(RefreshResult {
                state,
                snapshot_count: result.snapshots.len(),
                observed_at: Some(result.observed_at),
            })
        }
        Err(error) => {
            let state = repository.refresh_state(account_id)?;
            // Failures BEFORE this one: storage increments its own counter,
            // so the pre-increment value is exactly what the policy expects.
            let prior_failures = state.consecutive_failures;
            // A provider's `Retry-After` (429) names the moment the rate
            // window lifts, which beats our own guess when present; anything
            // unparsable falls back to the exponential backoff.
            let policy = BackoffPolicy::default();
            let provider_wait = error
                .retry_at
                .as_deref()
                .and_then(|value| retry_after_delay(value, started_at, policy.cap));
            let delay = provider_wait.unwrap_or_else(|| policy.delay_for_attempt(prior_failures));
            repository.mark_refresh_failed(
                account_id,
                started_at,
                format!("{:?}", error.code),
                Some(delay.as_secs()),
            )?;
            Err(RefreshError::Provider(error))
        }
    }
}

async fn native_fetch<A: crate::domain::provider::ProviderAdapter>(
    adapter: &A,
    account: &crate::domain::state::AccountMetadata,
    config: &crate::domain::provider::OpenAiCompatibleConfig,
    account_id: &str,
    credentials: &CredentialStore,
) -> Result<crate::providers::builtin::openai::FetchResult, ProviderError> {
    adapter
        .fetch_quota(
            &crate::domain::provider::AccountContext {
                account_id: account_id.to_owned(),
                provider_id: account.provider_id.clone(),
                credential_ref: account.credential_ref.clone(),
            },
            &crate::domain::provider::FetchContext {
                // Request timeout comes from the adapter's own AdapterLimits
                // (15s for the native adapters); `default_refresh` is the
                // *refresh interval* (300s) and would be a 20x-too-long
                // request timeout if passed here.
                timeout: crate::domain::provider::FetchContext::default().timeout,
                base_url: Some(config.base_url.clone()),
                credentials: Some(credentials.clone()),
                // The account's config is the only correct request target:
                // the native adapters are constructed from a placeholder
                // default, so they must read this rather than their own.
                config: Some(config.clone()),
            },
        )
        .await
        .map(|set| crate::providers::builtin::openai::FetchResult {
            snapshots: set,
            observed_at: unix_seconds(),
        })
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
