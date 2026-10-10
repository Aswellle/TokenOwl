// Refresh scheduling and orchestration
// Original content moved from src-tauri/src/refresh.rs
//! Manual/background refresh orchestration for local provider accounts.
//!
//! A failed request updates refresh state but never deletes the last normalized snapshot;
//! callers can continue to display it until the configured hard-expiry boundary.

use crate::credentials::CredentialStore;
use crate::domain::provider::{ProviderError, ProviderErrorKind};
use crate::domain::state::{BackoffPolicy, RefreshState};
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::storage::{LocalRepository, StorageError};
use std::time::Duration;
use thiserror::Error;

/// Retry times are persisted deterministically; scheduling jitter is applied by
/// the caller's scheduler instead of being baked into stored state.
const RETRY_JITTER_SAMPLE: f64 = 0.0;

#[derive(Debug, Error)]
pub enum RefreshError {
    #[error("refresh storage failed: {0}")]
    Storage(#[from] StorageError),
    #[error("provider refresh failed: {0}")]
    Provider(#[from] ProviderError),
}

#[derive(Clone, Debug)]
pub struct RefreshResult {
    pub state: RefreshState,
    pub snapshot_count: usize,
    pub observed_at: Option<u64>,
}

/// Refreshes one account and persists normalized data plus explicit refresh status.
///
/// `started_at` and the adapter's observation time are supplied by the caller/runtime,
/// making tests deterministic and avoiding hidden clock dependencies in persistence.
pub async fn refresh_account(
    repository: LocalRepository,
    adapter: &OpenAiCompatibleAdapter,
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
    match adapter.fetch(&config, account_id, credentials).await {
        Ok(result) => {
            let snapshot_count = result.snapshots.len();
            repository.upsert_snapshot_set(account_id, &result.snapshots, result.observed_at)?;
            let state = repository.mark_refresh_succeeded(account_id, result.observed_at)?;
            Ok(RefreshResult {
                state,
                snapshot_count,
                observed_at: Some(result.observed_at),
            })
        }
        Err(error) => {
            let refresh_state = repository.refresh_state(account_id)?;
            let next_attempt_at = retry_at(&refresh_state, started_at, &error);
            repository.mark_refresh_failed(
                account_id,
                started_at,
                format!("{:?}", error.code),
                next_attempt_at,
            )?;
            Err(RefreshError::Provider(error))
        }
    }
}

fn retry_at(state: &RefreshState, now: u64, error: &ProviderError) -> Option<u64> {
    if !error.retryable {
        return None;
    }
    let policy = BackoffPolicy {
        base: Duration::from_secs(300),
        cap: Duration::from_secs(3_600),
        jitter: Duration::ZERO,
    };
    Some(
        now.saturating_add(
            policy
                .delay(
                    state.consecutive_failures.saturating_add(1),
                    RETRY_JITTER_SAMPLE,
                )
                .as_secs(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::OpenAiCompatibleConfig;
    use crate::domain::state::AccountMetadata;
    use std::collections::HashMap;
    #[test]
    fn account_not_found_is_structured() {
        let repository = LocalRepository::open_in_memory().expect("database");
        let adapter =
            OpenAiCompatibleAdapter::with_default_limits(OpenAiCompatibleConfig::default())
                .expect("adapter");
        let result = futures_block_on(refresh_account(
            repository,
            &adapter,
            &CredentialStore::default(),
            "missing",
            10,
        ));
        assert!(
            matches!(result, Err(RefreshError::Provider(error)) if error.code == crate::domain::provider::ProviderErrorKind::ConfigInvalid)
        );
    }

    // A tiny test-only executor avoids adding a runtime dependency to the native core.
    fn futures_block_on<F: Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};

        let mut context = Context::from_waker(Waker::noop());
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[allow(dead_code)]
    fn _fixtures_compile() {
        let _ = OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1");
        let _ = AccountMetadata::from_auth_input(
            "account",
            "generic",
            "Account",
            "api_key",
            None,
            HashMap::new(),
            true,
            None,
        );
    }
}
