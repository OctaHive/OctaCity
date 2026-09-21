use std::time::Duration;

use async_trait::async_trait;
use octacity_server_application::ReadyJobWaiter;
use tokio::sync::watch;

/// Constant-memory process-local wake-up hints for ready-queue long polls.
///
/// One generation signal wakes all local polls after PostgreSQL reports a
/// committed ready-queue change. Notifications are never authoritative; the
/// application always repeats the transactional queue claim after waking.
#[derive(Clone)]
pub struct ReadyJobNotificationHub {
  generation: watch::Sender<u64>,
}

impl Default for ReadyJobNotificationHub {
  fn default() -> Self {
    let (generation, _) = watch::channel(0);
    Self { generation }
  }
}

impl ReadyJobNotificationHub {
  /// Wakes current placement polls after any ready-queue commit.
  pub fn notify(&self) {
    self
      .generation
      .send_modify(|generation| *generation = generation.wrapping_add(1));
  }
}

#[async_trait]
impl ReadyJobWaiter for ReadyJobNotificationHub {
  type Checkpoint = watch::Receiver<u64>;

  fn checkpoint(&self, _pool_identity: &str) -> Self::Checkpoint {
    self.generation.subscribe()
  }

  async fn wait_for_ready_job(&self, mut checkpoint: Self::Checkpoint, timeout: Duration) {
    let _ = tokio::time::timeout(timeout, checkpoint.changed()).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn a_notification_after_the_checkpoint_cannot_be_lost() {
    let hub = ReadyJobNotificationHub::default();
    let checkpoint = hub.checkpoint("linux");
    hub.notify();
    tokio::time::timeout(
      Duration::from_millis(100),
      hub.wait_for_ready_job(checkpoint, Duration::from_secs(1)),
    )
    .await
    .expect("a committed generation after the pre-claim checkpoint must wake");
  }
}
