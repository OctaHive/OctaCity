//! Optional provider-neutral projection through the verified Reporter process and existing outbox.
use octacity_server_application::{
  ApplicationError, FactoryExecutionClock, FactoryWorkStatusProjector, FactoryWorkStatusRequest, WorkSourceSnapshot,
  validate_accepted_factory_status_request,
};
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::{FactoryRunStore, FactorySourceStore, ManagementSecurityScope};
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::{ConfigurationReference, ResourceAccess};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[path = "work_reporter_projection.rs"]
mod projection;

/// Trusted operator settings. Handles are private references, never provider token bytes.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryReporterProfileDraft {
  /// Permitted OctaCity Project.
  pub project_id: ProjectId,
  /// Exact installed integration identity retained by the source input.
  pub installation: ImmutableReference,
  /// Exact action/Reporter capability selected by the Flow.
  pub capability: ImmutableReference,
  /// Exact provider-neutral ticket container definition.
  pub resource: ImmutableReference,
  /// Content-verified executable; provider names remain open identities.
  pub plugin: ImmutableReference,
  /// Pinned provider profile, independent of token rotation.
  pub provider_configuration: FactoryDigest,
  /// Original native container and separately scoped opaque write grant.
  pub access: ResourceAccess,
  /// Pinned native effect mapping, interpreted only by the provider.
  pub mapping: ConfigurationReference,
  /// Required action secret-profile ceiling.
  pub credential_profile: FactoryKey,
  /// Actual provider destinations required within the action's network ceiling.
  pub network_hosts: Vec<String>,
}
/// Immutable reporting settings whose complete contents participate in the selected binding.
#[derive(Clone)]
pub struct FactoryReporterProfile {
  binding: WorkSourceBinding,
  draft: FactoryReporterProfileDraft,
}
impl FactoryReporterProfile {
  /// Freezes all installed selections without reading a credential or contacting a provider.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    draft: FactoryReporterProfileDraft,
  ) -> Result<Self, ApplicationError> {
    draft
      .access
      .validate(octacity_work_source_protocol::CredentialClass::WorkReporterWrite)
      .map_err(|_| ApplicationError::invalid())?;
    draft.mapping.validate().map_err(|_| ApplicationError::invalid())?;
    if draft.network_hosts.is_empty()
      || draft.network_hosts.len() > 256
      || draft
        .network_hosts
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != draft.network_hosts.len()
      || draft
        .network_hosts
        .iter()
        .any(|host| NetworkHost::new(host.clone()).is_err())
    {
      return Err(ApplicationError::invalid());
    }
    let bytes = serde_json::to_vec(&draft).map_err(|_| ApplicationError::invalid())?;
    let configuration = ImmutableReference::new(
      identity,
      version,
      FactoryDigest::sha256("octacity.factory.reporter-profile.v1", &[&bytes]),
    );
    Ok(Self {
      binding: WorkSourceBinding {
        installation: draft.installation.clone(),
        capability: draft.capability.clone(),
        configuration,
        resource: draft.resource.clone(),
      },
      draft,
    })
  }
  /// Exact binding to publish in a source input; replacing any profile fact changes its identity.
  pub fn binding(&self) -> &WorkSourceBinding {
    &self.binding
  }
}

/// Trusted status projector backed by the same authoritative source journal and verified adapter host.
pub struct HostedFactoryWorkStatusProjector<S> {
  store: Arc<S>,
  registry: Arc<WorkAdapterRegistry>,
  profiles: Vec<FactoryReporterProfile>,
  clock: Arc<dyn FactoryExecutionClock>,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
}
impl<S> HostedFactoryWorkStatusProjector<S> {
  /// Constructs explicit Reporter support without resolving grants or contacting a provider.
  pub fn new(
    store: Arc<S>,
    registry: Arc<WorkAdapterRegistry>,
    profiles: Vec<FactoryReporterProfile>,
    clock: Arc<dyn FactoryExecutionClock>,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, ApplicationError> {
    if timeout.is_zero()
      || grace.is_zero()
      || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles
        .iter()
        .enumerate()
        .any(|(index, profile)| profiles[..index].iter().any(|p| p.binding == profile.binding))
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      store,
      registry,
      profiles,
      clock,
      timeout,
      grace,
      cancellation,
    })
  }
}
