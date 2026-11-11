//! Waits for a durable crash checkpoint while supervising its publishing process.
use std::{future::Future, process::Child, time::Duration};

/// The observer returns a checkpoint only after all of its durable barriers hold.
pub(super) async fn wait_for_checkpoint<T, F, P>(child: &mut Child, stage: &str, mut observe: F) -> T
where
  F: FnMut() -> P,
  P: Future<Output = Option<T>>,
{
  tokio::time::timeout(Duration::from_secs(15), async {
    loop {
      let checkpoint = observe().await;
      if let Some(status) = child.try_wait().unwrap() {
        assert!(
          stage == "associated" && status.success(),
          "child exited before crash boundary {stage}: {status}"
        );
        // The first read may have started before the child committed. A read
        // after successful exit must confirm the durable association.
        break match checkpoint {
          Some(checkpoint) => checkpoint,
          None => observe()
            .await
            .expect("successful child exited without durable association"),
        };
      }
      if let Some(checkpoint) = checkpoint {
        break checkpoint;
      }
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  })
  .await
  .expect("child reaches retained crash boundary")
}

#[path = "publication_checkpoint_tests.rs"]
mod tests;
