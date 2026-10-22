//! Controlled independent code-reader peer shared by PostgreSQL and imported-task pilots.
use octacity_server_application::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
#[derive(Default)]
pub struct Reader {
  pub revoked: AtomicBool,
  pub hidden: AtomicBool,
  pub reads: AtomicUsize,
  pub checks: bool,
}
#[async_trait::async_trait]
impl IssueCodeReviewObservations for Reader {
  async fn visibility(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
  ) -> Result<IssueCodeReviewRepositoryVisibility, ApplicationError> {
    Ok(if self.hidden.load(Ordering::SeqCst) {
      IssueCodeReviewRepositoryVisibility::none()
    } else {
      IssueCodeReviewRepositoryVisibility::all()
    })
  }
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    if self.revoked.load(Ordering::SeqCst) {
      Err(ApplicationError::capability_unavailable())
    } else {
      Ok(())
    }
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    self.authorize(context, project, selection).await?;
    let read = self.reads.fetch_add(1, Ordering::SeqCst);
    Ok(IssueCodeReviewObservation {
      selection: selection.clone(),
      observed_at: at(20 + read as i64),
      title: Some("Retained review".parse().unwrap()),
      number: None,
      url: None,
      author: None,
      state: Some(if read == 0 {
        IssueCodeReviewState::Open
      } else {
        IssueCodeReviewState::Merged
      }),
      draft: None,
      head: Some(if read == 0 { "old-head" } else { "new-head" }.parse().unwrap()),
      base: None,
      changed_files: None,
      additions: None,
      deletions: None,
      review: None,
      checks: None,
      check_details: self.checks.then(|| IssueExternalChecks {
        source: IssueExternalCheckSource::ExternalCi,
        head: if read == 0 { "old-head" } else { "new-head" }.parse().unwrap(),
        observed_at: at(20 + read as i64),
        has_more: true,
        items: [
          IssueCheckState::Pass,
          IssueCheckState::Fail,
          IssueCheckState::Missing,
          IssueCheckState::Indeterminate,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, state)| IssueExternalCheck {
          id: format!("check-{index}").parse().unwrap(),
          name: format!("Visible check {index}").parse().unwrap(),
          producer: None,
          state,
          required: Some(true),
        })
        .collect(),
      }),
    })
  }
}
