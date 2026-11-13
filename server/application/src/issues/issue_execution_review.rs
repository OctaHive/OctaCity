//! Read-only use of the same selection and retained preparation as execution admission.
use super::*;
use octacity_server_factory::{FactoryDigest, FactoryTaskSubject};
use octacity_server_store::{IssueExecutionSnapshot, IssuePropertyStore, IssueStore, ProjectTextInputStore};
use serde::Serialize;

/// Reviews one exact execution intent without a replay key, reservation or publication.
#[derive(Clone, Debug)]
pub struct ReviewIssueExecutionQuery {
  /// Exact Issue version, target and optional legacy scheduling selection.
  pub intent: RequestIssueExecutionIntent,
}
impl Query for ReviewIssueExecutionQuery {
  type Outcome = IssueExecutionReview;
}
impl ManagementAuthorizationTarget for ReviewIssueExecutionQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = RequestIssueExecutionCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.issue_id)
  }
}
/// Reviewed input only; it grants no admission or human acceptance.
#[derive(Clone, Serialize)]
pub struct IssueExecutionReview {
  /// Exact reviewed Issue revision.
  pub version: IssueVersion,
  /// Exact selected target, including a producer Run only in explicit producer mode.
  pub target: IssueExecutionTarget,
  /// Full selection identity, absent for an explicit producer-result input.
  pub snapshot_digest: Option<FactoryDigest>,
  /// Independently authorized selected data; absent for an explicit producer-result input.
  pub snapshot: Option<Box<IssueExecutionSnapshot>>,
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore + IssuePropertyStore, F: FactoryAdmissionStore + FactoryRunStore>
  ManagementQueryUseCase<ReviewIssueExecutionQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ReviewIssueExecutionQuery,
  ) -> Result<IssueExecutionReview, ApplicationError> {
    crate::issues::issue::visible_issue(grant, query.intent.issue_id)?;
    self.authorize_intent(context, &query.intent).await?;
    let issue = self
      .store
      .issue(query.intent.project_id, query.intent.issue_id, None)
      .await?;
    if issue.version != query.intent.expected_version {
      return Err(ApplicationError::Store(octacity_server_store::StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Issue,
      }));
    }
    let snapshot = self.select_input(context, &query.intent).await?;
    issue_execution_documents(&issue.content, &query.intent.target.mapping)?;
    Ok(IssueExecutionReview {
      version: issue.version,
      target: query.intent.target,
      snapshot_digest: snapshot.as_ref().map(IssueExecutionSnapshot::digest).transpose()?,
      snapshot: snapshot.map(Box::new),
    })
  }
}

/// Reads an original execution input under current Issue and independent material authority.
#[derive(Clone, Debug)]
pub struct GetIssueExecutionSnapshotQuery {
  /// Exact retained request; no latest-Run selection.
  pub execution: GetIssueExecutionQuery,
  /// Trusted server observation time; never decoded from a management request body.
  pub checked_at: Timestamp,
}
impl Query for GetIssueExecutionSnapshotQuery {
  type Outcome = IssueExecutionSnapshotObservation;
}
impl ManagementAuthorizationTarget for GetIssueExecutionSnapshotQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueExecutionQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.execution.management_resource()
  }
}
/// Unavailable retained input never appears as an empty snapshot or new current selection.
#[derive(Clone, Serialize)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum IssueExecutionSnapshotObservation {
  /// Original execution used the explicit producer-result contract.
  NotSelected,
  /// Private input or a selected source is unavailable under current retention/authority.
  Unavailable {
    /// Original sealed selection identity.
    digest: FactoryDigest,
  },
  /// Original retained input, with current body authorization checked independently.
  Available {
    /// Original sealed selection identity.
    digest: FactoryDigest,
    /// Exact frozen selection, never recomputed from current Issue values.
    snapshot: Box<IssueExecutionSnapshot>,
    /// Frozen repository subject once preparation resolved it.
    subject: Option<FactoryTaskSubject>,
  },
}
#[async_trait::async_trait]
impl<S: IssueExecutionStore + IssueStore + IssuePropertyStore, F: ProjectTextInputStore + FactoryRunStore>
  ManagementQueryUseCase<GetIssueExecutionSnapshotQuery> for IssueExecutionHandlers<S, F>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetIssueExecutionSnapshotQuery,
  ) -> Result<IssueExecutionSnapshotObservation, ApplicationError> {
    let checked_at = query.checked_at;
    let query = query.execution;
    crate::issues::issue::visible_issue(grant, query.issue_id)?;
    self.store.issue(query.project_id, query.issue_id, None).await?;
    let request = self
      .store
      .issue_execution(query.project_id, query.issue_id, query.request_id)
      .await?;
    let Some(digest) = request.snapshot_digest else {
      return Ok(IssueExecutionSnapshotObservation::NotSelected);
    };
    // Keep an expired/denied exact input unavailable without disclosing a storage failure.
    let read = self.retained_snapshot(context, &request, checked_at).await;
    Ok(match read {
      Ok((snapshot, subject)) => IssueExecutionSnapshotObservation::Available {
        digest,
        snapshot: Box::new(snapshot),
        subject,
      },
      Err(_) => IssueExecutionSnapshotObservation::Unavailable { digest },
    })
  }
}
impl<S: IssueExecutionStore + IssueStore + IssuePropertyStore, F: ProjectTextInputStore + FactoryRunStore>
  IssueExecutionHandlers<S, F>
{
  async fn retained_snapshot(
    &self,
    context: &ManagementRequestContext,
    request: &IssueExecutionRequest,
    checked_at: Timestamp,
  ) -> Result<(IssueExecutionSnapshot, Option<FactoryTaskSubject>), ApplicationError> {
    let intent = &request.intent;
    // A preparation lookup is partitioned by its original admission scope. In
    // another scope, `None` cannot establish that preparation never happened.
    self
      .store
      .issue_execution_for_admission(
        intent.project_id,
        intent.issue_id,
        request.id,
        &context.security_scope().to_store(),
      )
      .await?;
    let issue = self
      .store
      .issue(intent.project_id, intent.issue_id, Some(intent.expected_version))
      .await?;
    if !issue.content_available {
      return Err(ApplicationError::unavailable());
    }
    let preparation = self
      .factory
      .project_text_preparation(
        intent.project_id,
        &context.security_scope().to_store(),
        &request.admission_key(),
      )
      .await?;
    let (snapshot, subject) = if let Some(preparation) = preparation {
      if checked_at >= preparation.expires_at {
        return Err(ApplicationError::unavailable());
      }
      let documents = self
        .factory
        .project_text_documents(
          intent.project_id,
          &context.security_scope().to_store(),
          &request.admission_key(),
        )
        .await?
        .ok_or_else(ApplicationError::unavailable)?;
      let snapshot = documents
        .issue_snapshot()
        .cloned()
        .ok_or_else(ApplicationError::unavailable)?;
      let subject = preparation.frozen_revision.map(|revision| {
        FactoryTaskSubject::Exact(octacity_server_factory::ExactSubject::new(
          intent.project_id,
          intent.target.repository_id,
          revision,
        ))
      });
      (snapshot, subject)
    } else {
      if request.association.is_some() {
        return Err(ApplicationError::unavailable());
      }
      let selected = self
        .factory
        .factory_admission_context(octacity_server_store::ReadFactoryAdmissionContext {
          project_id: intent.project_id,
          configuration_id: intent.target.configuration_id,
          configuration_version: intent.target.configuration_version,
          repository_id: Some(intent.target.repository_id),
          repository_version: Some(intent.target.repository_version),
        })
        .await?;
      let snapshot = crate::issues::issue_execution_snapshot::select(
        self.store.as_ref(),
        crate::issues::issue_execution_snapshot::IssueSnapshotReadContext {
          policy: self.policy.as_ref(),
          context,
          materials: self.materials.as_deref(),
        },
        &issue,
        &intent.target,
        selected.configuration.configuration.flow(),
        request.snapshot_digest,
      )
      .await?;
      (snapshot, None)
    };
    if Some(snapshot.digest()?) != request.snapshot_digest {
      return Err(ApplicationError::invalid());
    }
    for (alias, materials) in &snapshot.materials {
      let reader = self.materials.as_ref().ok_or_else(ApplicationError::unavailable)?;
      for (index, material) in materials.iter().enumerate() {
        let (text, source) = reader.read(context, &material.selection.reference).await?;
        if source != material.source
          || snapshot
            .values
            .get(alias)
            .and_then(|v| v.get(index))
            .and_then(serde_json::Value::as_str)
            != Some(text.as_str())
        {
          return Err(ApplicationError::unavailable());
        }
      }
    }
    Ok((snapshot, subject))
  }
}
