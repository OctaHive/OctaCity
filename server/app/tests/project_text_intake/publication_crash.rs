//! Process death is injected after observable committed publication boundaries.
use super::*;
#[path = "publication_checkpoint.rs"]
mod checkpoint;

pub(super) async fn crash_at(
  database: &postgres::TestDatabase,
  store: &PostgresStore,
  issue: &IssueRevision,
  submission: &ProjectTextSubmission,
  request: &IssueExecutionRequest,
  directory: &std::path::Path,
  stage: &str,
) -> ProjectTextPreparation {
  use std::process::{Command, Stdio};
  struct Child(std::process::Child);
  impl Drop for Child {
    fn drop(&mut self) {
      let _ = self.0.kill();
      let _ = self.0.wait();
    }
  }
  let root = directory;
  let url = std::env::var("OCTACITY_POSTGRES_URL").unwrap();
  let (base, _) = url.rsplit_once('/').unwrap();
  let url = format!("{base}/{}", database.name);
  let spec = root.join("child.json");
  let pause_after = match stage {
    "first-byte" => 1,
    "both-bytes" => 2,
    _ => 0,
  };
  let specification = serde_json::json!({
    "database": url, "submission": submission, "issue_id": issue.id,
    "request_id": request.id, "root": root, "pause_after": pause_after,
  });
  std::fs::write(&spec, serde_json::to_vec(&specification).unwrap()).unwrap();
  let mut barrier = database.pool.begin().await.unwrap();
  match stage {
    "published" => {
      sqlx::query("LOCK TABLE factory_work_envelopes IN SHARE MODE")
        .execute(&mut *barrier)
        .await
        .unwrap();
    }
    "admitted" => {
      sqlx::query("LOCK TABLE issue_work_associations IN SHARE MODE")
        .execute(&mut *barrier)
        .await
        .unwrap();
    }
    _ => {}
  }
  let mut child = Child(
    Command::new(std::env::current_exe().unwrap())
      .args(["--exact", "crash_child", "--ignored", "--nocapture"])
      .env("OCTACITY_WORK_CARD_CRASH", &spec)
      .stdout(Stdio::null())
      .stderr(Stdio::piped())
      .spawn()
      .unwrap(),
  );
  let retained = checkpoint::wait_for_checkpoint(&mut child.0, stage, || async {
    // Observe external barriers before reading metadata: the child may freeze
    // or admit between the two reads, making an earlier record a stale baseline.
    let external_ready = match stage {
      "first-byte" | "both-bytes" => root.join("paused").exists(),
      "associated" => store
        .issue_execution(issue.project_id, issue.id, request.id)
        .await
        .unwrap()
        .association
        .is_some(),
      _ => true,
    };
    if external_ready
      && let Some(record) = store
        .project_text_preparation(
          issue.project_id,
          &ManagementSecurityScope::trusted_network(),
          &request.admission_key(),
        )
        .await
        .unwrap()
    {
      let ready = match stage {
        "first-byte" | "both-bytes" | "associated" => true,
        "published" => record.published_at.is_some(),
        "admitted" => record.admission.is_some(),
        _ => false,
      };
      if ready {
        return Some(record);
      }
    }
    None
  })
  .await;
  if stage != "associated" {
    child.0.kill().unwrap();
  }
  let status = child.0.wait().unwrap();
  if stage == "associated" {
    assert!(status.success(), "child failed after crash boundary {stage}: {status}");
  }
  barrier.rollback().await.unwrap();
  retained
}
