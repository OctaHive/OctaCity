use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_server_application::JobEventWaiter;
use tokio::sync::Notify;

/// Process-local wake-up hints for durable Job-event long polls.
///
/// One shared signal keeps notification memory bounded independently of the
/// number of Jobs. Wake-ups may be coalesced or lost: the application service
/// always re-reads authoritative state after wake or timeout.
#[derive(Clone, Default)]
pub struct JobEventNotificationHub {
  signal: Arc<Notify>,
}

impl JobEventNotificationHub {
  /// Wakes current Job-event followers after an event commit.
  ///
  /// The identity is accepted for a stable producer-facing seam. The first
  /// implementation deliberately uses one bounded process-wide signal.
  pub fn notify(&self, _job_identity: &str) {
    self.signal.notify_waiters();
  }
}

#[async_trait]
impl JobEventWaiter for JobEventNotificationHub {
  async fn wait_for_job_events(&self, _job_identity: &str, _observed_cursor: u64, timeout: Duration) {
    let _ = tokio::time::timeout(timeout, self.signal.notified()).await;
  }
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use super::*;

  #[tokio::test]
  async fn notification_wakes_a_bounded_wait() {
    let hub = JobEventNotificationHub::default();
    let waiter = hub.clone();
    let task = tokio::spawn(async move {
      waiter.wait_for_job_events("job", 0, Duration::from_secs(10)).await;
    });
    tokio::task::yield_now().await;
    hub.notify("job");
    task.await.unwrap();
  }

  #[tokio::test]
  async fn wait_is_bounded_without_a_notification() {
    let hub = JobEventNotificationHub::default();
    let started = Instant::now();
    hub.wait_for_job_events("job", 0, Duration::from_millis(1)).await;
    assert!(started.elapsed() < Duration::from_secs(1));
  }
}
