//! Optional provider-neutral read composition. Manual tasks install none of these dependencies.
use crate::*;
pub use octacity_server_domain::MAX_IMMUTABLE_REVISION_BYTES;
use octacity_server_domain::{IssueId, ProjectId};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  AttachIssueCodeReview, IdempotencyKey, IssueCodeReviewProvenance, IssueCodeReviewStore, ReadIssueCodeReviews,
};
pub use octacity_server_store::{
  AttachIssueCodeReviewIntent, IssueCodeReviewAssociation, IssueCodeReviewBinding, IssueCodeReviewObservation,
  IssueCodeReviewOutcome, IssueCodeReviewRepositoryVisibility, IssueCodeReviewSelection, IssueCodeReviewState,
  IssueExternalCheck, IssueExternalCheckSource, IssueExternalChecks,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[path = "issue_external_checks.rs"]
mod checks;
#[path = "issue_code_review_observation.rs"]
mod observation;
pub use checks::*;
pub use observation::*;
/// Narrow normalized review read. Implementations use installed code-repository grants,
/// independently from task read/update or Delivery; they must not dispatch arbitrary URLs.
#[async_trait::async_trait]
pub trait IssueCodeReviewObservations: Send + Sync {
  /// Current local metadata visibility without provider/credential I/O, before membership/count/cursor reads;
  /// revoking a protected
  /// reader may leave already retained repository metadata visible to the Issue operator.
  async fn visibility(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
  ) -> Result<octacity_server_store::IssueCodeReviewRepositoryVisibility, ApplicationError>;
  /// Rechecks current caller/native repository visibility and independent read authority, without I/O.
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError>;
  /// Verifies that the installed native repository is this candidate's local code repository.
  /// Readers without that independent mapping must keep optional lineage unavailable.
  async fn authorize_candidate(
    &self,
    _: &ManagementRequestContext,
    _: &IssueCodeReviewSelection,
    _: &octacity_server_factory::CandidateSubject,
  ) -> Result<(), ApplicationError> {
    Err(ApplicationError::capability_unavailable())
  }
  /// Verifies, without provider/credential I/O, that the frozen publication policy selects this exact
  /// installed native repository.
  /// Candidate repository mapping alone cannot distinguish independently readable mirrors.
  /// Readers without the original policy mapping cannot establish Delivery provenance.
  async fn authorize_publication(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: &IssueCodeReviewSelection,
    _: &octacity_server_factory::ImmutableReference,
  ) -> Result<(), ApplicationError> {
    Err(ApplicationError::capability_unavailable())
  }
  /// One bounded explicitly requested safe observation using existing supervised adapter machinery.
  async fn read(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError>;
}
/// Original retained Delivery receipt and immutable policy; the installed reader verifies its native destination.
#[derive(Clone, Debug)]
pub struct IssueCodeReviewPublication {
  /// Original publication receipt.
  pub receipt: FactoryDigest,
  /// Frozen policy selected by the producing action, never a caller-supplied target.
  pub target: octacity_server_factory::ImmutableReference,
}
/// Existing exact execution journal verification; no provider access or graph mutation.
#[async_trait::async_trait]
pub trait IssueCodeReviewExecutionReader: Send + Sync {
  /// Verifies the original association/record/candidate; Delivery additionally returns its original receipt
  /// and frozen publication policy for independent native-destination verification.
  async fn verify(
    &self,
    context: &ManagementRequestContext,
    binding: &IssueCodeReviewBinding,
    selection: &IssueCodeReviewSelection,
    delivery: bool,
  ) -> Result<Option<IssueCodeReviewPublication>, ApplicationError>;
}
/// Trusted Delivery association derives provenance from an existing publication record.
#[derive(Clone, Debug)]
pub struct AttachIssueCodeReviewDeliveryCommand {
  /// Original reviewed association, including required verified execution binding.
  pub attachment: AttachIssueCodeReviewCommand,
}
impl Command for AttachIssueCodeReviewDeliveryCommand {
  type Outcome = IssueCodeReviewOutcome;
}
impl ManagementAuthorizationTarget for AttachIssueCodeReviewDeliveryCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping = AttachIssueCodeReviewCommand::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    self.attachment.management_resource()
  }
}
/// Explicit reviewed attachment. No native state, actor, candidate setter or merge action.
#[derive(Clone, Debug)]
pub struct AttachIssueCodeReviewCommand {
  /// Frozen original selection and current Issue review.
  pub intent: AttachIssueCodeReviewIntent,
  /// Original scoped transport retry key.
  pub idempotency_key: IdempotencyKey,
}
impl Command for AttachIssueCodeReviewCommand {
  type Outcome = IssueCodeReviewOutcome;
}
impl ManagementAuthorizationTarget for AttachIssueCodeReviewCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Update, ManagementResourceKind::Issue);
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.intent.issue_id)
  }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  scope: FactoryDigest,
  boundary: u64,
  after: u64,
}
/// Bounded retained membership, using the existing opaque observation codec.
#[derive(Clone, Debug)]
pub struct ListIssueCodeReviewsQuery {
  project_id: ProjectId,
  issue_id: IssueId,
  after: Option<Cursor>,
  limit: u16,
}
impl ListIssueCodeReviewsQuery {
  /// Shared Issue page bounds and canonical cursor grammar, before any protected read.
  pub fn try_new(
    project_id: ProjectId,
    issue_id: IssueId,
    after: Option<String>,
    limit: u16,
  ) -> Result<Self, ApplicationError> {
    if limit == 0 || limit > octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      project_id,
      issue_id,
      after: after
        .map(|s| {
          crate::opaque_cursor::decode_canonical(&s, MAX_ISSUE_OBSERVATION_CURSOR_BYTES)
            .map_err(|_| ApplicationError::invalid())
        })
        .transpose()?,
      limit,
    })
  }
}
impl Query for ListIssueCodeReviewsQuery {
  type Outcome = IssueCodeReviewPage;
}
impl ManagementAuthorizationTarget for ListIssueCodeReviewsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.issue_id)
  }
}
/// Current read availability is separate from retained facts and merge/acceptance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCodeReviewAvailability {
  /// Current reader can be used.
  Available,
  /// Retained metadata remains; current reads are revoked/unavailable.
  Blocked,
}
/// Safe optional review projection.
#[derive(Clone, Debug, Serialize)]
pub struct IssueCodeReviewProjection {
  /// Original exact association.
  pub association: IssueCodeReviewAssociation,
  /// Last known safe observation, without current-authority claims.
  pub observation: Option<IssueCodeReviewObservation>,
  /// Independently current protected-read availability.
  pub availability: IssueCodeReviewAvailability,
}
/// Partial retained membership, with no overall success or acceptance inference.
#[derive(Clone, Debug, Serialize)]
pub struct IssueCodeReviewPage {
  /// Bounded safe rows only.
  pub items: Vec<IssueCodeReviewProjection>,
  /// Scope/visibility/original membership-bound continuation.
  pub next_cursor: Option<String>,
  /// Explicit retained observation contract.
  pub observation: &'static str,
}
/// Optional composition over ordinary Issue authorization, safe storage and installed readers.
pub struct IssueCodeReviewHandlers<S> {
  executions: Option<Arc<dyn IssueCodeReviewExecutionReader>>,
  store: Arc<S>,
  reader: Arc<dyn IssueCodeReviewObservations>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> IssueCodeReviewHandlers<S> {
  /// Adds the existing exact journal verifier only when optional execution lineage is supported.
  pub fn with_execution_reader(mut self, reader: Arc<dyn IssueCodeReviewExecutionReader>) -> Self {
    self.executions = Some(reader);
    self
  }
  async fn visible_repository(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    if !self
      .reader
      .visibility(context, project)
      .await?
      .allows(&selection.repository_identity())
    {
      return Err(
        octacity_server_store::StoreError::NotFound {
          entity: octacity_server_domain::EntityKind::Issue,
        }
        .into(),
      );
    }
    Ok(())
  }
  async fn read_scope(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    project: ProjectId,
    issue: IssueId,
    review: Option<FactoryDigest>,
  ) -> Result<
    (
      FactoryDigest,
      octacity_server_store::IssueCodeReviewRepositoryVisibility,
    ),
    ApplicationError,
  >
  where
    S: octacity_server_store::IssueStore,
  {
    self.store.issue(project, issue, None).await?;
    let issue_scope = crate::issue_activity::observation_scope(
      context,
      grant,
      serde_json::json!({"project":project,"issue":issue,"review":review}),
      "issue-code-review-membership-v1",
    )?;
    let visibility = self.reader.visibility(context, project).await?;
    Ok((
      crate::issue_activity::observation_scope_for(
        context,
        visibility.view(),
        serde_json::json!({"issue_scope":issue_scope}),
        "issue-code-review-repository-visibility-v1",
      ),
      visibility,
    ))
  }
  async fn availability(
    &self,
    context: &ManagementRequestContext,
    project: ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> IssueCodeReviewAvailability {
    if self.reader.authorize(context, project, selection).await.is_ok() {
      IssueCodeReviewAvailability::Available
    } else {
      IssueCodeReviewAvailability::Blocked
    }
  }
  /// Uses existing common authorization and trusted time; introduces no poller or executor.
  pub fn new(
    store: Arc<S>,
    reader: Arc<dyn IssueCodeReviewObservations>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      store,
      reader,
      policy,
      clock,
      executions: None,
    }
  }
}
#[async_trait::async_trait]
impl<S: IssueCodeReviewStore> ManagementCommandUseCase<AttachIssueCodeReviewCommand> for IssueCodeReviewHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: AttachIssueCodeReviewCommand,
  ) -> Result<IssueCodeReviewOutcome, ApplicationError> {
    self.attach(context, grant, command, false).await
  }
}
#[async_trait::async_trait]
impl<S: IssueCodeReviewStore> ManagementCommandUseCase<AttachIssueCodeReviewDeliveryCommand>
  for IssueCodeReviewHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: AttachIssueCodeReviewDeliveryCommand,
  ) -> Result<IssueCodeReviewOutcome, ApplicationError> {
    self.attach(context, grant, command.attachment, true).await
  }
}
impl<S: IssueCodeReviewStore> IssueCodeReviewHandlers<S> {
  async fn attach(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    command: AttachIssueCodeReviewCommand,
    delivery: bool,
  ) -> Result<IssueCodeReviewOutcome, ApplicationError> {
    let intent = &command.intent;
    crate::issue::visible_issue(grant, intent.issue_id)?;
    crate::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      context,
      ManagementResourceKind::Issue,
      intent.issue_id,
    )
    .await?;
    let issue = self.store.issue(intent.project_id, intent.issue_id, None).await?;
    self
      .visible_repository(context, intent.project_id, &intent.selection)
      .await?;
    if let Some(original) = self
      .store
      .issue_code_review_replay(&context.security_scope().to_store(), intent, &command.idempotency_key)
      .await?
    {
      if delivery
        != matches!(
          original.association.provenance,
          IssueCodeReviewProvenance::Delivery { .. }
        )
      {
        return Err(ApplicationError::invalid());
      }
      return Ok(original);
    }
    if issue.version != intent.expected_version {
      return Err(
        octacity_server_store::StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::Issue,
        }
        .into(),
      );
    }
    self
      .reader
      .authorize(context, intent.project_id, &intent.selection)
      .await?;
    let publication = if let Some(binding) = &intent.binding {
      if binding.association.project_id != intent.project_id || binding.association.issue_id != intent.issue_id {
        return Err(ApplicationError::invalid());
      }
      self
        .reader
        .authorize_candidate(context, &intent.selection, &binding.candidate)
        .await?;
      self
        .executions
        .as_ref()
        .ok_or_else(ApplicationError::capability_unavailable)?
        .verify(context, binding, &intent.selection, delivery)
        .await?
    } else if delivery {
      return Err(ApplicationError::invalid());
    } else {
      None
    };
    if delivery != publication.is_some() {
      return Err(ApplicationError::invalid());
    }
    if let Some(publication) = &publication {
      self
        .reader
        .authorize_publication(context, intent.project_id, &intent.selection, &publication.target)
        .await?;
    }
    let observation = self.reader.read(context, intent.project_id, &intent.selection).await?;
    self
      .reader
      .authorize(context, intent.project_id, &intent.selection)
      .await?;
    if observation.selection != intent.selection
      || intent
        .binding
        .as_ref()
        .is_some_and(|b| observation.head.as_ref() != Some(b.candidate.candidate_revision()))
    {
      return Err(ApplicationError::invalid());
    }
    let provenance = if let Some(publication) = publication {
      IssueCodeReviewProvenance::Delivery {
        receipt: publication.receipt,
      }
    } else {
      IssueCodeReviewProvenance::Explicit {
        actor: crate::management_security::audited_mutation(context, ())?
          .audit()
          .actor()
          .clone(),
      }
    };
    Ok(
      self
        .store
        .attach_issue_code_review(crate::management_security::audited_mutation(
          context,
          AttachIssueCodeReview {
            intent: command.intent,
            observation,
            provenance,
            idempotency_key: command.idempotency_key,
            attached_at: self.clock.now()?,
          },
        )?)
        .await?,
    )
  }
}
#[async_trait::async_trait]
impl<S: IssueCodeReviewStore> ManagementQueryUseCase<ListIssueCodeReviewsQuery> for IssueCodeReviewHandlers<S> {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ListIssueCodeReviewsQuery,
  ) -> Result<IssueCodeReviewPage, ApplicationError> {
    crate::issue::visible_issue(grant, query.issue_id)?;
    let (scope, visibility) = self
      .read_scope(context, grant, query.project_id, query.issue_id, None)
      .await?;
    if query.after.as_ref().is_some_and(|c| c.scope != scope) {
      return Err(ApplicationError::invalid());
    }
    let page = self
      .store
      .issue_code_reviews(ReadIssueCodeReviews {
        project_id: query.project_id,
        issue_id: query.issue_id,
        scope: context.security_scope().to_store(),
        boundary: query.after.as_ref().map(|c| c.boundary),
        after: query.after.map(|c| c.after),
        limit: query.limit,
        visibility,
      })
      .await?;
    let mut items = Vec::with_capacity(page.items.len());
    for row in page.items {
      let availability = self
        .availability(context, query.project_id, &row.association.selection)
        .await;
      items.push(IssueCodeReviewProjection {
        association: row.association,
        observation: row.observation.map(metadata_only),
        availability,
      });
    }
    Ok(IssueCodeReviewPage {
      items,
      next_cursor: page.next.map(|after| {
        crate::opaque_cursor::encode(&Cursor {
          scope,
          boundary: page.boundary,
          after,
        })
      }),
      observation: "issue-code-review-membership-v1",
    })
  }
}

fn metadata_only(mut observation: IssueCodeReviewObservation) -> IssueCodeReviewObservation {
  observation.check_details = None;
  observation
}
