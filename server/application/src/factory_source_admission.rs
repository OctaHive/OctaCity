//! Trusted installed-source ingress into the common authoritative admission path.

use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_factory::{
  ExternalWorkIdentity, WorkSourceCapability, WorkSourceCapabilityRole, WorkSourceInput, WorkSourceInputId,
  WorkSourceInputVersion,
};
use octacity_server_store::{FactoryAdmissionIdentity, FactoryAdmissionStore, PublishedFactoryAdmission};

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

/// Installed-source ingress with mandatory independently supplied authorization.
pub struct FactorySourceAdmissionHandlers<S> {
  admission: FactoryAdmissionHandlers<S>,
  store: Arc<S>,
  clock: Arc<dyn crate::FactoryExecutionClock>,
  authorization: Arc<dyn FactorySourceAuthorizer>,
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
    }
  }

  /// Installs trusted owner time; production defaults to the system execution clock.
  #[must_use]
  pub fn with_clock(mut self, clock: Arc<dyn crate::FactoryExecutionClock>) -> Self {
    self.clock = clock;
    self
  }

  /// Recovers the original admitted Work without preparing a candidate or calling VCS.
  /// Absence reserves nothing; a subsequent new intent must still cross atomic admission.
  pub async fn recover(
    &self,
    command: RecoverSourceFactoryWorkCommand,
  ) -> Result<Option<PublishedFactoryAdmission>, ApplicationError> {
    let authorized = self
      .authorization
      .authorize(command.input_id, command.input_version)
      .await?;
    validate_authorization(&authorized, command.input_id, command.input_version, &command.item)?;
    let identity = FactoryAdmissionIdentity {
      source_scope: crate::factory_admission::installed_source_scope(
        &authorized.context,
        authorized.input.project_id(),
        &authorized.capability.binding.installation,
      )?,
      external_identity: source_item_identity(&authorized, &command.item)?,
    };
    self
      .admission
      .recover_source(&authorized.context, authorized.input.project_id(), &identity)
      .await
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
      || command.intent.repository_id != definition.repository.id
      || command.intent.repository_version != definition.repository.version
    {
      return Err(ApplicationError::invalid());
    }
    self
      .admission
      .authorize_source_intent(&authorized.context, &command.intent)
      .await?;
    command.intent.external_identity = source_item_identity(&authorized, &command.item)?;
    let probe = crate::factory_admission::source_admission_probe(
      &authorized.context,
      &command.intent,
      &authorized.capability.binding.installation,
    )?;
    self
      .admission
      .admit_intent(&authorized.context, command.intent, probe, Some(&authorized.input))
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
        probe,
        repository_version: intent.repository_version,
        work: original.work,
        run: original.run,
        flow: original.flow,
        admitted_at: original.admitted_at,
      }
    } else {
      self
        .admission
        .prepare_fresh_intent(&authorized.context, intent.clone(), probe, Some(&preparation.input))
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
    self
      .store
      .admit_prepared_source_work(
        guard,
        crate::management_security::audited_mutation(&authorized.context, request)?,
      )
      .await
      .map(Into::into)
      .map_err(Into::into)
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
