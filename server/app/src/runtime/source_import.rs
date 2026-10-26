//! Discovers reviewed import heads and delegates each finite slice to the existing Issue Source consumer.
use super::source_management::SourceManagement;
use octacity_server_application::*;
use octacity_server_domain::{IssueSourceImportId, ProjectId};
use octacity_server_store::{IssueSourceImportStore, ManagementSecurityScope};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(super) struct SourceImportWorker {
  sources: Arc<SourceManagement>,
  context: ManagementRequestContext,
  cursor: tokio::sync::Mutex<Option<(ProjectId, IssueSourceImportId)>>,
}
impl SourceImportWorker {
  /// Source bounds are validated by the shared offline/runtime profile loader before composition.
  pub(super) fn new(sources: Arc<SourceManagement>, principal: &str) -> Self {
    Self {
      sources,
      context: super::local_management::request_context(principal),
      cursor: tokio::sync::Mutex::new(None),
    }
  }
  pub(super) fn settings(&self) -> crate::config::WorkerPolicy {
    self.sources.profiles.consumer.worker()
  }
  pub(super) async fn run_once(&self, cancellation: CancellationToken) -> Result<u16, ApplicationError> {
    let settings = self.settings();
    let scope =
      ManagementSecurityScope::new(self.context.security_scope().as_str()).map_err(|_| ApplicationError::invalid())?;
    let mut cursor = self.cursor.lock().await;
    let imports = self
      .sources
      .store
      .active_issue_source_imports(&scope, *cursor, settings.batch_size())
      .await?;
    *cursor = if imports.len() == usize::from(settings.batch_size()) {
      imports.last().map(|i| (i.project_id, i.id))
    } else {
      None
    };
    drop(cursor);
    let mut completed = 0;
    for import in imports {
      if cancellation.is_cancelled() {
        break;
      }
      let result = async {
        let profile = self
          .sources
          .observation_profile(
            &self.context,
            import.project_id,
            &import.definition.source,
            &import.definition.connection,
            tokio::time::Instant::now() + self.sources.timeout,
          )
          .await?;
        let policy = self.sources.profiles.consumer.read_policy(profile.polling)?;
        let clock = Arc::new(SystemFactoryExecutionClock);
        let at = clock.now()?;
        let until = octacity_server_domain::Timestamp::from_unix_millis(
          at.unix_millis()
            .checked_add(settings.claim_lifetime().as_millis() as i64)
            .ok_or_else(ApplicationError::invalid)?,
        )
        .map_err(|_| ApplicationError::invalid())?;
        let worker = Arc::new(IssueSourceReadHandlers::new(
          self.sources.store.clone(),
          self.sources.clone(),
          self.sources.policy.clone(),
          clock,
          policy,
        )?);
        AuthorizedCommandHandler::new(self.sources.policy.clone(), worker)
          .handle_command(
            &self.context,
            RunIssueSourceReadCommand {
              project_id: import.project_id,
              target: IssueSourceReadTarget {
                pull_generation: 0,
                source: import.definition.source.clone(),
                issue_id: None,
                import: Some(import),
              },
              owner: format!("issue-source.{}", uuid::Uuid::new_v4())
                .parse()
                .map_err(|_| ApplicationError::invalid())?,
              until,
            },
          )
          .await
          .map_err(|_| ApplicationError::unavailable())?;
        Ok::<(), ApplicationError>(())
      }
      .await;
      match result {
        Ok(()) => completed += 1,
        Err(_) if cancellation.is_cancelled() => break,
        Err(_) => tracing::warn!("Issue Source import slice deferred; retained progress will be retried"),
      }
    }
    Ok(completed)
  }
}
