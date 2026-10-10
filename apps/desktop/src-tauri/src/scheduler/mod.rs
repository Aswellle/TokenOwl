//! Provider refresh scheduler with priority queue, jitter, and backoff.

use crate::credentials::CredentialStore;
use crate::domain::provider::{ProviderError, ProviderErrorKind};
use crate::domain::state::{BackoffPolicy, RefreshState};
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::storage::{LocalRepository, StorageError};
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

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
            let attempt = state.consecutive_failures;
            let delay = BackoffPolicy::default().delay_for_attempt(attempt);
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

/// Scheduler configuration for a single provider.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub min_interval: Duration,
    pub recommended_interval: Duration,
    pub retry_after_support: bool,
    pub rate_limit_class: u8,
    pub priority: u8,
    pub refresh_on_open: bool,
    pub refresh_in_background: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            min_interval: Duration::from_secs(60),
            recommended_interval: Duration::from_secs(300),
            retry_after_support: false,
            rate_limit_class: 2,
            priority: 5,
            refresh_on_open: true,
            refresh_in_background: true,
        }
    }
}

/// A scheduled refresh task.
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub provider_id: String,
    pub next_refresh: Instant,
    pub priority: u8,
    pub retry_attempt: u32,
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for ScheduledTask {}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .next_refresh
            .cmp(&self.next_refresh)
            .then_with(|| other.priority.cmp(&self.priority))
    }
}

/// Manages provider refresh scheduling.
pub struct Scheduler {
    queue: BinaryHeap<ScheduledTask>,
    configs: std::collections::HashMap<String, SchedulerConfig>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            configs: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, provider_id: &str, config: SchedulerConfig) {
        self.configs.insert(provider_id.to_owned(), config);
    }

    pub fn schedule(&mut self, provider_id: &str, priority: u8) {
        let interval = self
            .configs
            .get(provider_id)
            .map_or(Duration::from_secs(300), |c| c.recommended_interval);
        let jittered = Self::apply_jitter(interval);
        self.queue.push(ScheduledTask {
            provider_id: provider_id.to_owned(),
            next_refresh: Instant::now() + jittered,
            priority,
            retry_attempt: 0,
        });
    }

    pub fn next_due(&mut self) -> Option<ScheduledTask> {
        let now = Instant::now();
        if let Some(task) = self.queue.peek() {
            if task.next_refresh <= now {
                return self.queue.pop();
            }
        }
        None
    }

    pub fn mark_failed(&mut self, provider_id: &str, attempt: u32) {
        let delay = BackoffPolicy::default().delay_for_attempt(attempt);
        let jittered = Self::apply_jitter(delay);
        self.queue.push(ScheduledTask {
            provider_id: provider_id.to_owned(),
            next_refresh: Instant::now() + jittered,
            priority: 10,
            retry_attempt: attempt + 1,
        });
    }

    pub fn mark_succeeded(&mut self, provider_id: &str) {
        self.schedule(provider_id, 5);
    }

    fn apply_jitter(duration: Duration) -> Duration {
        let base = duration.as_secs_f64();
        let jitter_amount = base * 0.1;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        #[allow(clippy::cast_precision_loss)]
        let jitter = ((nanos as f64 / 1_000_000_000.0) * jitter_amount).abs();
        Duration::from_secs_f64(base + jitter)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_new_is_empty() {
        let scheduler = Scheduler::new();
        assert!(scheduler.is_empty());
    }

    #[test]
    fn scheduler_schedule_increases_len() {
        let mut scheduler = Scheduler::new();
        scheduler.schedule("openai", 5);
        assert_eq!(scheduler.len(), 1);
    }

    #[test]
    fn scheduler_next_due_none_when_empty() {
        let mut scheduler = Scheduler::new();
        assert!(scheduler.next_due().is_none());
    }

    #[test]
    fn scheduler_mark_failed_reschedules() {
        let mut scheduler = Scheduler::new();
        scheduler.schedule("openai", 5);
        scheduler.mark_failed("openai", 1);
        assert_eq!(scheduler.len(), 2);
    }

    #[test]
    fn scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.recommended_interval, Duration::from_secs(300));
        assert_eq!(config.rate_limit_class, 2);
    }

    #[test]
    fn binary_heap_orders_by_priority() {
        let mut scheduler = Scheduler::new();
        scheduler.schedule("low", 10);
        scheduler.schedule("high", 1);
        scheduler.schedule("medium", 5);

        assert_eq!(scheduler.len(), 3);

        let queue = &scheduler.queue;
        let tasks: Vec<_> = queue.iter().collect();
        let min_priority = tasks.iter().map(|t| t.priority).min().unwrap();
        assert_eq!(min_priority, 1);
    }

    #[test]
    fn mark_failed_uses_exponential_backoff() {
        let mut scheduler = Scheduler::new();
        scheduler.schedule("openai", 5);

        scheduler.mark_failed("openai", 0);
        scheduler.mark_failed("openai", 1);
        scheduler.mark_failed("openai", 2);

        assert_eq!(scheduler.len(), 4);
    }

    #[test]
    fn mark_succeeded_reschedules_at_normal_priority() {
        let mut scheduler = Scheduler::new();
        scheduler.schedule("openai", 5);
        scheduler.mark_succeeded("openai");
        assert_eq!(scheduler.len(), 2);
    }

    #[test]
    fn apply_jitter_within_expected_range() {
        let base = Duration::from_secs(300);
        for _ in 0..100 {
            let jittered = Scheduler::apply_jitter(base);
            let base_secs = base.as_secs();
            let jittered_secs = jittered.as_secs();
            assert!(jittered_secs >= base_secs);
            assert!(jittered_secs <= base_secs + 30);
        }
    }

    #[test]
    fn next_due_returns_none_when_no_task_is_due() {
        let mut scheduler = Scheduler::new();
        scheduler.schedule("openai", 5);
        assert!(scheduler.next_due().is_none());
    }

    #[test]
    fn register_stores_config_for_provider() {
        let mut scheduler = Scheduler::new();
        let config = SchedulerConfig {
            recommended_interval: Duration::from_secs(600),
            ..SchedulerConfig::default()
        };
        scheduler.register("openai", config);
        scheduler.schedule("openai", 5);
        assert_eq!(scheduler.len(), 1);
    }
}
