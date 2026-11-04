//! Trusted installed-source ingress into the common authoritative admission path.

use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::RepositorySourceSelection;
use octacity_server_factory::{
  ExactSubject, ExternalWorkIdentity, WorkSourceCapability, WorkSourceCapabilityRole, WorkSourceInput,
  WorkSourceInputId, WorkSourceInputVersion,
};
use octacity_server_store::{
  FactoryAdmissionIdentity, FactoryAdmissionStore, IssueExecutionObservation, PublishedFactoryAdmission,
  WorkSourcePredecessorCursor, WorkSourcePredecessorPosition, WorkSourceWaitReason,
};

use crate::factory::admission::factory_admission::SourceAdmissionSelection;
use crate::{
  ApplicationError, FactoryAdmissionHandlers, FactoryAdmissionOutcome, FactoryWorkAdmissionIntent,
  ManagementAuthorizationPolicy, ManagementRequestContext, RevisionResolver,
};

/// Provider-neutral immutable ticket container and item identities.
pub type FactorySourceItem = octacity_server_store::WorkSourceItem;

/// Read-only observation lookup before preparing new artifacts or resolving a base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverSourceFactoryWorkCommand {
  /// Stable authorized input identity.
  pub input_id: WorkSourceInputId,
  /// Current immutable input used to authorize this observation.
  pub input_version: WorkSourceInputVersion,
  /// Native immutable container/item identity, without mutable observation content.
  pub item: FactorySourceItem,
}

/// Normalized source intent without a claimed actor, security scope or source namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitSourceFactoryWorkCommand {
  /// Stable operator-published input requested by the server worker.
  pub input_id: WorkSourceInputId,
  /// Exact immutable input revision used for this observation.
  pub input_version: WorkSourceInputVersion,
  /// Native item identity independently of prepared Work artifacts.
  pub item: FactorySourceItem,
  /// Bounded ordinary Work intent, subject to the installed input's exact targets.
  /// Its external identity is replaced by the application-derived input/item identity.
  pub intent: FactoryWorkAdmissionIntent,
}

/// Submit only an already frozen preparation under current installed input authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitPreparedSourceFactoryWorkCommand {
  /// Current selected stable input.
  pub input_id: WorkSourceInputId,
  /// Current installed revision authorizing this observation, including recovery.
  pub input_version: WorkSourceInputVersion,
  /// Exact original preparation and live scan ownership; no caller-supplied Work data.
  pub guard: octacity_server_store::AdmitPreparedWorkSource,
}

/// Trusted installed facts resolved by server authorization, never provider wire data.
/// The Repository pin is SHA-256 of the published definition's normalized JSON bytes.
#[derive(Clone, Debug)]
pub struct FactorySourceAuthorization {
  /// Authorized immutable input for the bound source installation.
  pub input: WorkSourceInput,
  /// Independently installed Project-scoped Source declaration.
  pub capability: WorkSourceCapability,
  /// Native immutable container id verified against the exact installed resource definition.
  pub resource_identity: ExternalWorkIdentity,
  /// Accepted installation-owner attribution and policy context.
  pub context: ManagementRequestContext,
}

/// Trusted server port bound to a verified installed source, not chosen by a ticket.
///
/// Implementations must resolve and authorize the input for that installation and
/// supply its accepted owner context. Provider claims cannot construct this port
/// or select its actor/security scope. There is no implicit allow-all resolver.
/// Verification of the live process/grant and current input availability belongs
/// to the concrete implementation on every invocation, including replay.
/// Native container ids must match the pinned resource definition. Security scope
/// must stay stable across credential rotation and cannot come from a token or ticket.
#[async_trait]
pub trait FactorySourceAuthorizer: Send + Sync {
  /// Resolves the exact permitted input or returns a safe denial/unavailability.
  async fn authorize(
    &self,
    input_id: WorkSourceInputId,
    version: WorkSourceInputVersion,
  ) -> Result<FactorySourceAuthorization, ApplicationError>;
}

/// One verified predecessor and the original implementation inputs for that ticket.
#[derive(Clone)]
pub struct FactorySourceHandoff {
  pub(crate) record: octacity_server_factory::FlowNodeRecord,
  pub(crate) artifacts: octacity_server_factory::WorkArtifacts,
  pub(crate) issue_text: Option<octacity_server_store::ProjectTextDocuments>,
  issue_text_digest: Option<octacity_server_factory::FactoryDigest>,
  pub(crate) resolved_subject: Option<ExactSubject>,
  selection: Option<SourceHandoffSelection>,
}

#[derive(Clone)]
struct SourceHandoffSelection {
  input: WorkSourceInput,
  item: FactorySourceItem,
  scope: octacity_server_store::ManagementSecurityScope,
  source: RepositorySourceSelection,
}

impl FactorySourceHandoff {
  /// Combines a trusted accepted record with its original Work artifacts.
  pub fn new(
    record: octacity_server_factory::FlowNodeRecord,
    artifacts: octacity_server_factory::WorkArtifacts,
  ) -> Self {
    Self {
      record,
      artifacts,
      issue_text: None,
      issue_text_digest: None,
      resolved_subject: None,
      selection: None,
    }
  }

  /// Attests to the title and description of the verified predecessor's Issue revision.
  /// Prepared Source admission compares this evidence with the frozen observation.
  #[must_use]
  pub fn with_issue_text_evidence(mut self, text: octacity_server_store::WorkSourceIssueText<'_>) -> Self {
    self.issue_text_digest = Some(text.digest());
    self
  }

  /// Checks whether the in-flight proof still belongs to this exact preparation.
  pub fn matches_preparation(
    &self,
    authorized: &FactorySourceAuthorization,
    preparation: &octacity_server_store::WorkSourcePreparation,
    source: &RepositorySourceSelection,
  ) -> bool {
    self.selection.as_ref().is_some_and(|selection| {
      selection.input == preparation.input
        && selection.item == preparation.item
        && selection.scope == authorized.context.security_scope().to_store()
        && &selection.source == source
    })
  }
  /// Exact accepted record identity for preparation and recovery.
  pub fn record_digest(&self) -> Result<octacity_server_factory::FactoryDigest, ApplicationError> {
    self.record.digest().map_err(|_| ApplicationError::invalid())
  }

  /// Checks the downstream Work against the original implementation inputs.
  pub fn matches_artifacts(&self, artifacts: &octacity_server_factory::WorkArtifacts) -> bool {
    let references = [artifacts.task(), artifacts.acceptance()];
    if let Some(text) = &self.issue_text {
      return references
        .into_iter()
        .zip([text.task(), text.acceptance()])
        .all(|(artifact, bytes)| {
          artifact.encoded_size() == bytes.len() as u64
            && artifact.content_digest() == octacity_server_factory::FactoryDigest::content_sha256(bytes)
        });
    }
    references
      .into_iter()
      .zip([self.artifacts.task(), self.artifacts.acceptance()])
      .all(|(left, right)| {
        left.encoded_size() == right.encoded_size() && left.content_digest() == right.content_digest()
      })
  }
}

pub(crate) fn stale_source_handoff() -> ApplicationError {
  crate::FactoryValidationDiagnostics::unavailable_with_code("/incoming_result", "incoming_result_stale")
}

// A Source search shares its scan lease with the provider and other candidates.
pub(crate) const MAX_PREDECESSOR_SEARCH_PAGE: u16 = 16;

const PREDECESSOR_WAIT_CODES: &[(&str, WorkSourceWaitReason)] = &[
  ("missing_incoming_result", WorkSourceWaitReason::Missing),
  ("incoming_result_not_accepted", WorkSourceWaitReason::NotAccepted),
  ("incoming_result_stale", WorkSourceWaitReason::Stale),
  ("incoming_result_inaccessible", WorkSourceWaitReason::Inaccessible),
  ("incoming_result_incompatible", WorkSourceWaitReason::Incompatible),
  ("incoming_result_wrong_issue", WorkSourceWaitReason::WrongIssue),
  ("incoming_result_wrong_subject", WorkSourceWaitReason::WrongSubject),
  (
    "incoming_result_missing_implementation_acceptance",
    WorkSourceWaitReason::MissingImplementationAcceptance,
  ),
];

/// Classifies only the safe predecessor diagnostics that can be retried for one candidate.
pub fn source_predecessor_wait(
  error: &ApplicationError,
) -> Option<(WorkSourceWaitReason, Option<WorkSourcePredecessorCursor>)> {
  if let ApplicationError::SourcePredecessorSearchPending(cursor) = error {
    return Some((WorkSourceWaitReason::Searching, Some(*cursor)));
  }
  let ApplicationError::FactoryValidation(diagnostics) = error else {
    return None;
  };
  let [issue] = diagnostics.errors.as_slice() else {
    return None;
  };
  if issue.field != "/incoming_result" {
    return None;
  }
  PREDECESSOR_WAIT_CODES
    .iter()
    .find(|(code, _)| *code == issue.code)
    .map(|(_, reason)| (*reason, None))
}

#[derive(Clone, Copy)]
enum PredecessorSearch {
  Exhaustive,
  Page(Option<WorkSourcePredecessorCursor>),
}

/// The authorized target and optional installed code selection for this observation.
pub struct FactorySourcePredecessorTarget<'a> {
  /// Exact published code Repository.
  pub repository: &'a octacity_server_store::PublishedRepository,
  /// Exact published Flow accepting the result.
  pub flow: &'a octacity_server_factory::FactoryFlowConfiguration,
  source: Option<&'a RepositorySourceSelection>,
  revisions: &'a dyn RevisionResolver,
  expected_record: Option<octacity_server_factory::FactoryDigest>,
  search: PredecessorSearch,
}
impl<'a> FactorySourcePredecessorTarget<'a> {
  /// Uses an exact published Repository and Flow, with an optional explicit code selection.
  pub fn new(
    repository: &'a octacity_server_store::PublishedRepository,
    flow: &'a octacity_server_factory::FactoryFlowConfiguration,
    source: Option<&'a RepositorySourceSelection>,
    revisions: &'a dyn RevisionResolver,
  ) -> Self {
    Self {
      repository,
      flow,
      source,
      revisions,
      expected_record: None,
      search: PredecessorSearch::Exhaustive,
    }
  }

  /// Runs one bounded slice from a private continuation retained by the Source worker.
  #[must_use]
  pub fn with_search_cursor(mut self, cursor: Option<WorkSourcePredecessorCursor>) -> Self {
    self.search = PredecessorSearch::Page(cursor);
    self
  }
}

/// Server-owned resolution of a linked ticket's exact accepted result.
#[async_trait]
pub trait FactorySourcePredecessors: Send + Sync {
  /// Resolves the handoff from the installed source's own ticket identity.
  /// Include Issue text evidence for results used by prepared Source admission.
  async fn resolve(
    &self,
    authorized: &FactorySourceAuthorization,
    item: &FactorySourceItem,
    target: FactorySourcePredecessorTarget<'_>,
  ) -> Result<FactorySourceHandoff, ApplicationError>;
}

/// Reads accepted Issue lineage using the source installation's own security scope.
pub struct IssueSourcePredecessorReader<I, R> {
  issues: Arc<I>,
  runs: Arc<R>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
}
impl<I, R> IssueSourcePredecessorReader<I, R> {
  /// Composes the existing Issue link, execution history and Run readers.
  pub fn new(issues: Arc<I>, runs: Arc<R>, policy: Arc<dyn ManagementAuthorizationPolicy>) -> Self {
    Self { issues, runs, policy }
  }
}
#[async_trait]
impl<I, R> FactorySourcePredecessors for IssueSourcePredecessorReader<I, R>
where
  I: octacity_server_store::IssueSourceStore + octacity_server_store::IssueExecutionStore,
  R: octacity_server_store::FactoryRunStore,
{
  async fn resolve(
    &self,
    authorized: &FactorySourceAuthorization,
    item: &FactorySourceItem,
    target: FactorySourcePredecessorTarget<'_>,
  ) -> Result<FactorySourceHandoff, ApplicationError> {
    let project = authorized.input.project_id();
    let scope = authorized.context.security_scope().to_store();
    let binding = &authorized.capability.binding;
    let native = octacity_server_store::source_native_correlation(
      octacity_server_store::source_native_issue(project, binding.installation.identity(), item),
      octacity_server_store::source_security_partition(&scope),
    );
    let missing =
      || crate::FactoryValidationDiagnostics::unavailable_with_code("/incoming_result", "missing_incoming_result");
    let link = self
      .issues
      .native_issue_source_link(project, &scope, native)
      .await?
      .ok_or_else(missing)?;
    crate::factory::authoring::factory_configuration_choices::require_visible_reference(
      self.policy.as_ref(),
      &authorized.context,
      crate::ManagementResourceKind::Issue,
      link.issue_id,
    )
    .await
    .map_err(|_| {
      crate::FactoryValidationDiagnostics::unavailable_with_code("/incoming_result", "incoming_result_inaccessible")
    })?;
    use WorkSourcePredecessorPosition::{Manual, Native};
    let mut after = None;
    let mut observation: Option<IssueExecutionObservation> = None;
    let mut native_after = None;
    let mut manual_done = false;
    let mut rejected = WorkSourceWaitReason::Missing;
    if let PredecessorSearch::Page(Some(cursor)) = target.search
      && cursor.issue == link.issue_id
      && self.issues.issue(project, link.issue_id, None).await?.version == cursor.observation.issue_version
    {
      observation = Some(cursor.observation);
      rejected = cursor.last_reason;
      match cursor.position {
        Manual(position) => after = Some(position),
        Native(position) => {
          native_after = Some(position);
          manual_done = true;
        }
      }
    }
    let paged = matches!(target.search, PredecessorSearch::Page(_));
    let limit = if paged {
      MAX_PREDECESSOR_SEARCH_PAGE
    } else {
      octacity_server_store::MAX_ISSUE_HISTORY_PAGE_SIZE
    };
    let mut resolved = None;
    loop {
      let native_page = manual_done;
      let (version, candidates, next) = if native_page {
        let items = self
          .issues
          .native_issue_work(project, link.issue_id, native_after, limit)
          .await?;
        let boundary = observation.ok_or_else(ApplicationError::invalid)?;
        native_after = items.last().map(|last| last.work_id);
        let next = native_after.filter(|_| items.len() == usize::from(limit)).map(Native);
        (boundary.issue_version, items, next)
      } else {
        let page = self
          .issues
          .issue_executions(project, link.issue_id, after, observation, limit)
          .await?;
        after = page.next;
        manual_done = after.is_none();
        observation = Some(page.observation);
        (
          page.observation.issue_version,
          page.items.into_iter().filter_map(|item| item.association).collect(),
          after.map(Manual),
        )
      };
      for association in candidates {
        match crate::issues::issue_execution::resolve_associated_issue_predecessor(
          self.issues.as_ref(),
          self.runs.as_ref(),
          self.policy.as_ref(),
          &authorized.context,
          crate::issues::issue_execution::IssuePredecessorRequest {
            project,
            issue: link.issue_id,
            expected_version: version,
            repository: target.repository.id,
            run: association.run_id,
          },
          target.flow,
        )
        .await
        {
          Ok(mut handoff) => {
            if let Some(expected) = target.expected_record
              && handoff.record_digest()? != expected
            {
              rejected = WorkSourceWaitReason::Stale;
              continue;
            }
            if let Some(source) = target.source {
              if resolved.is_none() {
                resolved = Some(
                  crate::factory::admission::factory_admission::resolve_repository_subject(
                    target.revisions,
                    source,
                    target.repository,
                  )
                  .await?,
                );
              }
              if resolved
                .as_ref()
                .is_some_and(|subject| subject != handoff.record.input().subject())
              {
                rejected = WorkSourceWaitReason::WrongSubject;
                continue;
              }
              handoff.resolved_subject = resolved;
            }
            return Ok(handoff);
          }
          Err(error) => {
            let Some((reason, _)) = source_predecessor_wait(&error) else {
              return Err(error);
            };
            rejected = reason;
          }
        }
      }
      if let (true, Some(position)) = (paged, next) {
        return Err(ApplicationError::SourcePredecessorSearchPending(
          WorkSourcePredecessorCursor {
            issue: link.issue_id,
            observation: observation.ok_or_else(ApplicationError::invalid)?,
            position,
            last_reason: rejected,
          },
        ));
      }
      if native_page && next.is_none() {
        let code = PREDECESSOR_WAIT_CODES
          .iter()
          .find(|(_, reason)| *reason == rejected)
          .map_or("missing_incoming_result", |(code, _)| *code);
        return Err(crate::FactoryValidationDiagnostics::unavailable_with_code(
          "/incoming_result",
          code,
        ));
      }
    }
  }
}

/// Installed-source ingress with mandatory independently supplied authorization.
pub struct FactorySourceAdmissionHandlers<S> {
  admission: FactoryAdmissionHandlers<S>,
  store: Arc<S>,
  clock: Arc<dyn crate::FactoryExecutionClock>,
  authorization: Arc<dyn FactorySourceAuthorizer>,
  predecessors: Option<Arc<dyn FactorySourcePredecessors>>,
}

impl<S: FactoryAdmissionStore + 'static> FactorySourceAdmissionHandlers<S> {
  /// Composes the ordinary admission ports and the verified installation authorizer.
  #[must_use]
  pub fn new(
    store: Arc<S>,
    revisions: Arc<dyn RevisionResolver>,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    authorization: Arc<dyn FactorySourceAuthorizer>,
  ) -> Self {
    Self {
      admission: FactoryAdmissionHandlers::with_policy(store.clone(), revisions, policy),
      store,
      clock: Arc::new(crate::SystemFactoryExecutionClock),
      authorization,
      predecessors: None,
    }
  }

  /// Resolves declared step sources through the ordinary admission catalog.
  pub fn with_repositories(mut self, repositories: Arc<dyn octacity_server_store::ConfigurationStore>) -> Self {
    self.admission = self.admission.with_repositories(repositories);
    self
  }

  /// Installs trusted owner time; production defaults to the system execution clock.
  #[must_use]
  pub fn with_clock(mut self, clock: Arc<dyn crate::FactoryExecutionClock>) -> Self {
    self.clock = clock;
    self
  }

  /// Uses the installed Issue lineage reader for configurations that require a predecessor.
  #[must_use]
  pub fn with_predecessors(mut self, predecessors: Arc<dyn FactorySourcePredecessors>) -> Self {
    self.predecessors = Some(predecessors);
    self
  }

  /// Resumes against the original target while checking the currently selected Source owner.
  pub async fn prepared_source_handoff(
    &self,
    authorized: &FactorySourceAuthorization,
    claim: &octacity_server_store::WorkSourceScanClaim,
    preparation: &octacity_server_store::WorkSourcePreparation,
    source: &RepositorySourceSelection,
    expected_record: Option<octacity_server_factory::FactoryDigest>,
  ) -> Result<Option<FactorySourceHandoff>, ApplicationError> {
    preparation.require_selected_owner(claim, &authorized.input)?;
    let handoff = self
      .source_handoff_for_input(
        authorized,
        &preparation.item,
        &preparation.input,
        Some(source),
        expected_record,
        PredecessorSearch::Page(preparation.predecessor_cursor),
      )
      .await?;
    if let Some(handoff) = &handoff
      && preparation
        .observed_issue_text
        .is_some_and(|digest| handoff.issue_text_digest != Some(digest))
    {
      return Err(stale_source_handoff());
    }
    Ok(handoff)
  }

  async fn source_handoff_for_input(
    &self,
    authorized: &FactorySourceAuthorization,
    item: &FactorySourceItem,
    input: &WorkSourceInput,
    source: Option<&RepositorySourceSelection>,
    expected_record: Option<octacity_server_factory::FactoryDigest>,
    search: PredecessorSearch,
  ) -> Result<Option<FactorySourceHandoff>, ApplicationError> {
    validate_authorization(authorized, authorized.input.id(), authorized.input.version(), item)?;
    let selected = self.admission.require_source_launch(&authorized.context, input).await?;
    let flow = selected.configuration.configuration.flow();
    if flow.accepted_predecessor.is_some()
      && let Some(predecessors) = &self.predecessors
    {
      let mut handoff = predecessors
        .resolve(
          authorized,
          item,
          FactorySourcePredecessorTarget {
            expected_record,
            search,
            ..FactorySourcePredecessorTarget::new(
              selected.repository.as_ref().ok_or_else(ApplicationError::invalid)?,
              flow,
              source,
              self.admission.revisions.as_ref(),
            )
          },
        )
        .await?;
      handoff.selection = source.map(|source| SourceHandoffSelection {
        input: input.clone(),
        item: item.clone(),
        scope: authorized.context.security_scope().to_store(),
        source: source.clone(),
      });
      return Ok(Some(handoff));
    }
    crate::factory::admission::factory_admission::reject_missing_incoming_result(flow)?;
    Ok(None)
  }

  /// Recovers the original admitted Work without preparing a candidate or calling VCS.
  /// Absence reserves nothing; a subsequent new intent must still cross atomic admission.
  pub async fn recover(
    &self,
    command: RecoverSourceFactoryWorkCommand,
  ) -> Result<Option<PublishedFactoryAdmission>, ApplicationError> {
    let (authorized, original) = self.inspect_source(&command).await?;
    if original.is_none() {
      self
        .source_handoff_for_input(
          &authorized,
          &command.item,
          &authorized.input,
          None,
          None,
          PredecessorSearch::Exhaustive,
        )
        .await?;
    }
    Ok(original)
  }

  /// Returns the verified scope for the first reservation, or None if already admitted.
  /// Mutable predecessor and source resolution must follow that reservation.
  pub async fn source_reservation_scope(
    &self,
    command: RecoverSourceFactoryWorkCommand,
  ) -> Result<Option<octacity_server_store::ManagementSecurityScope>, ApplicationError> {
    let (authorized, original) = self.inspect_source(&command).await?;
    if original.is_some() {
      return Ok(None);
    }
    self
      .admission
      .require_source_launch(&authorized.context, &authorized.input)
      .await?;
    Ok(Some(authorized.context.security_scope().to_store()))
  }

  async fn inspect_source(
    &self,
    command: &RecoverSourceFactoryWorkCommand,
  ) -> Result<(FactorySourceAuthorization, Option<PublishedFactoryAdmission>), ApplicationError> {
    let authorized = self
      .authorization
      .authorize(command.input_id, command.input_version)
      .await?;
    validate_authorization(&authorized, command.input_id, command.input_version, &command.item)?;
    let identity = FactoryAdmissionIdentity {
      source_scope: crate::factory::admission::factory_admission::installed_source_scope(
        &authorized.context,
        authorized.input.project_id(),
        &authorized.capability.binding.installation,
      )?,
      external_identity: source_item_identity(&authorized, &command.item)?,
    };
    let original = self
      .admission
      .recover_source(&authorized.context, authorized.input.project_id(), &identity)
      .await?;
    Ok((authorized, original))
  }

  /// Authorizes installed-source intake before any admission store or VCS access.
  pub async fn admit(
    &self,
    mut command: AdmitSourceFactoryWorkCommand,
  ) -> Result<FactoryAdmissionOutcome, ApplicationError> {
    let authorized = self
      .authorization
      .authorize(command.input_id, command.input_version)
      .await?;
    validate_authorization(&authorized, command.input_id, command.input_version, &command.item)?;
    let definition = authorized.input.definition();
    if command.intent.project_id != authorized.input.project_id()
      || command.intent.configuration_id != definition.target.id()
      || command.intent.configuration_version != definition.target.version()
      || command.intent.repository_id != Some(definition.repository.id)
      || command.intent.repository_version != Some(definition.repository.version)
    {
      return Err(ApplicationError::invalid());
    }
    self
      .admission
      .authorize_source_intent(&authorized.context, &command.intent)
      .await?;
    command.intent.external_identity = source_item_identity(&authorized, &command.item)?;
    let probe = crate::factory::admission::factory_admission::source_admission_probe(
      &authorized.context,
      &command.intent,
      &authorized.capability.binding.installation,
    )?;
    let handoff = if self.store.replay_factory_admission(&probe).await?.is_none() {
      self
        .source_handoff_for_input(
          &authorized,
          &command.item,
          &authorized.input,
          Some(command.intent.source.repository()?),
          None,
          PredecessorSearch::Exhaustive,
        )
        .await?
    } else {
      None
    };
    self
      .admission
      .admit_intent(
        &authorized.context,
        command.intent,
        probe,
        Some(SourceAdmissionSelection {
          input: &authorized.input,
          handoff: handoff.as_ref(),
        }),
        octacity_server_factory::FactoryAdmissionOrigin::AutomaticSource,
      )
      .await
  }
}

impl<S: octacity_server_store::FactorySourceStore + 'static> FactorySourceAdmissionHandlers<S> {
  /// Uses ordinary admission gates and atomically retains its original Work/Run correlation.
  /// Replay uses the frozen receipt before VCS access; the store still checks the live scan fence.
  pub async fn admit_prepared(
    &self,
    command: AdmitPreparedSourceFactoryWorkCommand,
  ) -> Result<FactoryAdmissionOutcome, ApplicationError> {
    self.admit_prepared_with_handoff(command, None).await
  }

  /// Reuses a proof selected during this worker operation after checking its exact binding.
  /// The authoritative admission store still revalidates the Issue before committing.
  pub async fn admit_prepared_with_handoff(
    &self,
    command: AdmitPreparedSourceFactoryWorkCommand,
    handoff: Option<FactorySourceHandoff>,
  ) -> Result<FactoryAdmissionOutcome, ApplicationError> {
    let authorized = self
      .authorization
      .authorize(command.input_id, command.input_version)
      .await?;
    if command.guard.claim.project_id != authorized.input.project_id() {
      return Err(ApplicationError::invalid());
    }
    self
      .admission
      .authorize_source_project(&authorized.context, authorized.input.project_id())
      .await?;
    let preparation = self
      .store
      .source_preparation(authorized.input.project_id(), command.guard.preparation_id)
      .await?;
    validate_authorization(&authorized, command.input_id, command.input_version, &preparation.item)?;
    preparation.require_selected_owner(&command.guard.claim, &authorized.input)?;
    self
      .admission
      .authorize_source_preparation(&authorized.context, &preparation)
      .await?;
    let intent = preparation.intent.as_ref().ok_or_else(ApplicationError::invalid)?;
    self
      .admission
      .authorize_source_intent(&authorized.context, intent)
      .await?;
    let probe = preparation.admission_probe(authorized.context.security_scope().to_store())?;
    let request = if let Some(replay) = self.store.replay_factory_admission(&probe).await? {
      let original = replay.admission;
      octacity_server_store::AdmitFactoryWork {
        incoming: None,
        probe,
        repository_version: intent.repository_version,
        work: original.work,
        run: original.run,
        flow: original.flow,
        admitted_at: original.admitted_at,
      }
    } else {
      let source = intent.source.repository()?;
      let settings: Option<crate::SourcePreparationSettings> = preparation
        .policy
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|_| ApplicationError::invalid())?;
      let expected = settings.and_then(|settings| settings.predecessor_digest);
      let handoff = match handoff {
        Some(proof) if proof.matches_preparation(&authorized, &preparation, source) => Some(proof),
        _ => {
          self
            .prepared_source_handoff(&authorized, &command.guard.claim, &preparation, source, expected)
            .await?
        }
      };
      let predecessor = handoff.as_ref().map(FactorySourceHandoff::record_digest).transpose()?;
      if expected != predecessor {
        return Err(stale_source_handoff());
      }

      self
        .admission
        .prepare_fresh_intent(
          &authorized.context,
          intent.clone(),
          probe,
          Some(SourceAdmissionSelection {
            input: &preparation.input,
            handoff: handoff.as_ref(),
          }),
          octacity_server_factory::FactoryAdmissionOrigin::AutomaticSource,
        )
        .await?
    };
    // Revision resolution may yield after the initial grant/visibility checks.
    // Neither retained Source bytes nor an earlier grant authorize a revoked input.
    let current = self
      .authorization
      .authorize(command.input_id, command.input_version)
      .await?;
    validate_authorization(&current, command.input_id, command.input_version, &preparation.item)?;
    preparation.require_selected_owner(&command.guard.claim, &current.input)?;
    self
      .admission
      .authorize_source_preparation(&authorized.context, &preparation)
      .await?;
    self
      .admission
      .authorize_source_intent(&authorized.context, intent)
      .await?;
    let mut guard = command.guard;
    guard.now = guard.now.max(self.clock.now()?);
    let has_predecessor = request
      .incoming
      .as_ref()
      .is_some_and(|incoming| incoming.predecessor().is_some());
    self
      .store
      .admit_prepared_source_work(
        guard,
        crate::management_security::audited_mutation(&authorized.context, request)?,
      )
      .await
      .map(Into::into)
      .map_err(|error| match error {
        octacity_server_store::StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::Issue,
        } if has_predecessor => stale_source_handoff(),
        other => other.into(),
      })
  }
}

fn validate_authorization(
  authorized: &FactorySourceAuthorization,
  input_id: WorkSourceInputId,
  version: WorkSourceInputVersion,
  item: &FactorySourceItem,
) -> Result<(), ApplicationError> {
  if authorized.input.id() != input_id
    || authorized.input.version() != version
    || !authorized.input.definition().enabled
    || authorized.capability.project_id != authorized.input.project_id()
    || authorized.capability.role != WorkSourceCapabilityRole::Source
    || authorized.capability.binding != authorized.input.definition().source
    || item.resource != authorized.resource_identity
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}

fn source_item_identity(
  authorized: &FactorySourceAuthorization,
  item: &FactorySourceItem,
) -> Result<ExternalWorkIdentity, ApplicationError> {
  Ok(
    octacity_server_store::source_admission_identity(
      &authorized.input,
      item,
      authorized.context.security_scope().to_store(),
    )
    .external_identity,
  )
}
