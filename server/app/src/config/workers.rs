use std::time::Duration;

/// Shared bounded scheduling policy for one durable worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WorkerPolicy {
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
}

impl WorkerPolicy {
  pub(crate) const fn new(poll_milliseconds: u64, claim_milliseconds: u64, batch_size: u16) -> Self {
    Self {
      poll_interval: Duration::from_millis(poll_milliseconds),
      claim_lifetime: Duration::from_millis(claim_milliseconds),
      batch_size,
    }
  }

  pub(crate) const fn poll_interval(self) -> Duration {
    self.poll_interval
  }

  pub(crate) const fn claim_lifetime(self) -> Duration {
    self.claim_lifetime
  }

  pub(crate) const fn batch_size(self) -> u16 {
    self.batch_size
  }

  pub(crate) fn is_bounded(self, maximum_duration: Duration, maximum_batch_size: u16) -> bool {
    !self.poll_interval.is_zero()
      && self.poll_interval <= maximum_duration
      && !self.claim_lifetime.is_zero()
      && self.claim_lifetime <= maximum_duration
      && self.batch_size > 0
      && self.batch_size <= maximum_batch_size
  }
}

/// Durable worker scheduling plus bounded exponential retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetryingWorkerPolicy {
  worker: WorkerPolicy,
  max_attempts: u16,
  initial_retry_milliseconds: u64,
  maximum_retry_milliseconds: u64,
}

impl RetryingWorkerPolicy {
  pub(crate) const fn new(
    worker: WorkerPolicy,
    max_attempts: u16,
    initial_retry_milliseconds: u64,
    maximum_retry_milliseconds: u64,
  ) -> Self {
    Self {
      worker,
      max_attempts,
      initial_retry_milliseconds,
      maximum_retry_milliseconds,
    }
  }

  pub(crate) const fn worker(self) -> WorkerPolicy {
    self.worker
  }

  pub(crate) const fn max_attempts(self) -> u16 {
    self.max_attempts
  }

  pub(crate) const fn initial_retry_milliseconds(self) -> u64 {
    self.initial_retry_milliseconds
  }

  pub(crate) const fn maximum_retry_milliseconds(self) -> u64 {
    self.maximum_retry_milliseconds
  }

  pub(crate) fn is_bounded(self, maximum_duration: Duration, maximum_batch_size: u16, maximum_attempts: u16) -> bool {
    self.worker.is_bounded(maximum_duration, maximum_batch_size)
      && self.max_attempts > 0
      && self.max_attempts <= maximum_attempts
      && self.initial_retry_milliseconds > 0
      && self.maximum_retry_milliseconds >= self.initial_retry_milliseconds
      && Duration::from_millis(self.maximum_retry_milliseconds) <= maximum_duration
  }
}

/// Shared scheduling and retry policy for both webhook worker queues.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WebhookWorkerPolicy {
  delivery: RetryingWorkerPolicy,
  managed_batch_size: u16,
}

impl WebhookWorkerPolicy {
  pub(crate) const fn new(delivery: RetryingWorkerPolicy, managed_batch_size: u16) -> Self {
    Self {
      delivery,
      managed_batch_size,
    }
  }

  pub(crate) const fn delivery(self) -> RetryingWorkerPolicy {
    self.delivery
  }

  pub(crate) const fn managed_batch_size(self) -> u16 {
    self.managed_batch_size
  }

  pub(crate) fn is_bounded(
    self,
    maximum_duration: Duration,
    maximum_delivery_batch: u16,
    maximum_managed_batch: u16,
    maximum_attempts: u16,
  ) -> bool {
    self
      .delivery
      .is_bounded(maximum_duration, maximum_delivery_batch, maximum_attempts)
      && self.managed_batch_size > 0
      && self.managed_batch_size <= maximum_managed_batch
  }
}
