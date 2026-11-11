//! Real child exits and controlled observations make the association race deterministic.
use super::*;
use std::{cell::Cell, process::Command};

fn exited_child(success: bool) -> Child {
  let mut child = Command::new("sh")
    .args(["-c", if success { "exit 0" } else { "exit 1" }])
    .spawn()
    .unwrap();
  // Reaping makes the subsequent status observation deterministic; an observer
  // can still return a stale read that started before this exit.
  assert_eq!(child.wait().unwrap().success(), success);
  child
}

#[tokio::test]
async fn associated_checkpoint_rechecks_a_stale_read_after_successful_child_exit() {
  let mut child = exited_child(true);
  let observed = Cell::new(false);
  let checkpoint = wait_for_checkpoint(&mut child, "associated", || async {
    if observed.replace(true) {
      Some("committed checkpoint")
    } else {
      None
    }
  })
  .await;
  assert_eq!(checkpoint, "committed checkpoint");
}

#[tokio::test]
#[should_panic(expected = "successful child exited without durable association")]
async fn associated_checkpoint_rejects_successful_exit_without_a_durable_association() {
  let mut child = exited_child(true);
  wait_for_checkpoint(&mut child, "associated", || async { None::<()> }).await;
}

#[tokio::test]
#[should_panic(expected = "child exited before crash boundary associated")]
async fn associated_checkpoint_rejects_a_failed_exit_with_a_durable_association() {
  let mut child = exited_child(false);
  wait_for_checkpoint(&mut child, "associated", || async { Some("committed checkpoint") }).await;
}

#[tokio::test]
#[should_panic(expected = "child exited before crash boundary associated")]
async fn associated_checkpoint_rejects_a_failed_exit_without_an_association() {
  let mut child = exited_child(false);
  wait_for_checkpoint(&mut child, "associated", || async { None::<()> }).await;
}

#[tokio::test]
#[should_panic(expected = "child exited before crash boundary first-byte")]
async fn paused_checkpoint_rejects_premature_successful_exit() {
  let mut child = exited_child(true);
  wait_for_checkpoint(&mut child, "first-byte", || async { None::<()> }).await;
}
