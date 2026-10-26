//! Discovers explicit import and independent retained-link pull targets over the existing Issue Source journal.
use super::source_management::SourceManagement;
use octacity_server_application::*;
use octacity_server_domain::{IssueId, IssueSourceImportId, ProjectId};
use octacity_server_store::{IssueSourceImportStore, IssueSourceReadStore, ManagementSecurityScope};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(super) struct IssueSourceWorker {
  sources: Arc<SourceManagement>,
  context: ManagementRequestContext,
  discovery: tokio::sync::Mutex<Discovery>,
}
#[derive(Default)]
struct Discovery {
  imports: Option<(ProjectId, IssueSourceImportId)>,
  links: Option<(ProjectId, IssueId)>,
  links_first: bool,
}

impl IssueSourceWorker {
  /// The shared loader validates Source bounds before composition; construction performs no read.
  pub(super) fn new(sources: Arc<SourceManagement>, principal: &str) -> Self {
    Self {
      sources,
      context: super::local_management::request_context(principal),
      discovery: tokio::sync::Mutex::new(Discovery::default()),
    }
  }
  pub(super) fn settings(&self) -> crate::config::WorkerPolicy {
    self.sources.profiles.consumer.worker()
  }
  pub(super) async fn run_once(&self, cancellation: CancellationToken) -> Result<u16, ApplicationError> {
    let settings = self.settings();
    let scope =
      ManagementSecurityScope::new(self.context.security_scope().as_str()).map_err(|_| ApplicationError::invalid())?;
    let mut discovery = self.discovery.lock().await;
    let mut targets = Vec::new();
    // Both kinds share one pass budget. Alternate first choice so even batch_size=1
    // makes progress on retained links while import controls remain continuously active.
    for links in [discovery.links_first, !discovery.links_first] {
      let remaining = settings.batch_size() - targets.len() as u16;
      if remaining == 0 {
        break;
      }
      if links {
        let reads = self
          .sources
          .store
          .active_issue_source_reads(&scope, discovery.links, remaining)
          .await?;
        discovery.links = if reads.len() == usize::from(remaining) {
          reads.last().and_then(|(p, t)| t.issue_id.map(|i| (*p, i)))
        } else {
          None
        };
        targets.extend(reads);
      } else {
        let imports = self
          .sources
          .store
          .active_issue_source_imports(&scope, discovery.imports, remaining)
          .await?;
        discovery.imports = if imports.len() == usize::from(remaining) {
          imports.last().map(|i| (i.project_id, i.id))
        } else {
          None
        };
        targets.extend(imports.into_iter().map(|import| {
          (
            import.project_id,
            IssueSourceReadTarget {
              source: import.definition.source.clone(),
              connection: None,
              import: Some(import),
              issue_id: None,
              pull_generation: 0,
            },
          )
        }));
      }
    }
    discovery.links_first = !discovery.links_first;
    drop(discovery);
    let mut completed = 0;
    for (project, target) in targets {
      if cancellation.is_cancelled() {
        break;
      }
      match self.read(project, target).await {
        Ok(()) => completed += 1,
        Err(_) if cancellation.is_cancelled() => break,
        Err(_) => tracing::warn!("Issue Source read slice deferred; retained progress will be retried"),
      }
    }
    Ok(completed)
  }
  async fn read(&self, project: ProjectId, target: IssueSourceReadTarget) -> Result<(), ApplicationError> {
    let connection = target
      .read_connection()
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let profile = self
      .sources
      .observation_profile(
        &self.context,
        project,
        &target.source,
        connection,
        tokio::time::Instant::now() + self.sources.timeout,
      )
      .await?;
    let policy = self.sources.profiles.consumer.read_policy(profile.polling)?;
    let clock = Arc::new(SystemFactoryExecutionClock);
    let at = clock.now()?;
    let until = octacity_server_domain::Timestamp::from_unix_millis(
      at.unix_millis()
        .checked_add(self.settings().claim_lifetime().as_millis() as i64)
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
          project_id: project,
          target,
          owner: format!("issue-source.{}", uuid::Uuid::new_v4())
            .parse()
            .map_err(|_| ApplicationError::invalid())?,
          until,
        },
      )
      .await
      .map_err(|e| match e {
        ManagementHandlerError::Application(error) => error,
        ManagementHandlerError::Forbidden(_) => ApplicationError::capability_unavailable(),
      })?;
    Ok(())
  }
}
