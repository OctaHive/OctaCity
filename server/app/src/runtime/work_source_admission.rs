//! Current fenced selection, retained preparation and ordinary source admission.
use super::process::execute_source;
use super::*;
pub(super) struct SourceCandidateBridge<S, B> {
  pub(super) managed_connections: Option<ManagedIntegrationConnections>,
  pub(super) store: Arc<S>,
  pub(super) bytes: Arc<B>,
  pub(super) revisions: Arc<dyn RevisionResolver>,
  pub(super) authorization: Arc<dyn ManagementAuthorizationPolicy>,
  pub(super) registry: Arc<WorkAdapterRegistry>,
  pub(super) profiles: Vec<FactorySourceProfile>,
  pub(super) clock: Arc<dyn FactoryExecutionClock>,
  pub(super) cancellation: CancellationToken,
  pub(super) settings: FactorySourceWorkerSettings,
}
#[async_trait::async_trait]
impl<S: WorkSourceArtifactStore + 'static, B: ArtifactBytePublisher + 'static> FactorySourceCandidateProcessor
  for SourceCandidateBridge<S, B>
{
  async fn settle(&self, claim: WorkSourceScanClaim, snapshot: WorkSourceSnapshot) -> Result<(), ApplicationError> {
    require_running(&self.cancellation)?;
    let members = self.store.source_scan_members(claim.clone(), self.clock.now()?).await?;
    let profiles = profiles_for(&self.profiles, &members, self.managed_connections.as_ref())?;
    let mut matches = Vec::new();
    for input in members.iter().filter(|v| v.definition().enabled) {
      let profile = profiles
        .iter()
        .find(|p| p.project_id == input.project_id() && p.binding == input.definition().source)
        .ok_or_else(ApplicationError::capability_unavailable)?;
      let provider_matches = snapshot
        .selection_matches
        .contains(&selection_digest(&profile.selection)?)
        || (members.len() == 1 && snapshot.selection_matches.is_empty());
      if provider_matches
        && input
          .definition()
          .predicate
          .as_ref()
          .is_none_or(|predicate| predicate.mismatches(&snapshot.state, &snapshot.labels).is_empty())
      {
        matches.push(input.id());
      }
    }
    let selected = self
      .store
      .select_source_input(claim.clone(), self.clock.now()?, snapshot.item.clone(), matches)
      .await?;
    let WorkSourceSelection::Selected(input) = selected else {
      return Ok(());
    };
    let authorizer = self.authority(claim.clone(), &input)?;
    let admission = FactorySourceAdmissionHandlers::new(
      self.store.clone(),
      self.revisions.clone(),
      self.authorization.clone(),
      authorizer,
    )
    .with_clock(self.clock.clone());
    if admission
      .recover(RecoverSourceFactoryWorkCommand {
        input_id: input.id(),
        input_version: input.version(),
        item: snapshot.item.clone(),
      })
      .await?
      .is_some()
    {
      return Ok(());
    }
    require_running(&self.cancellation)?;
    let retained = self
      .store
      .reserve_source_preparation(ReserveWorkSourcePreparation {
        claim: claim.clone(),
        input_id: input.id(),
        item: snapshot.item.clone(),
        snapshot: serde_json::to_vec(&snapshot).map_err(|_| ApplicationError::invalid())?,
        now: self.clock.now()?,
      })
      .await?;
    self.finish_preparation(claim, &input, retained).await
  }
}
impl<S: WorkSourceArtifactStore + 'static, B: ArtifactBytePublisher + 'static> SourceCandidateBridge<S, B> {
  fn authority(
    &self,
    claim: WorkSourceScanClaim,
    input: &WorkSourceInput,
  ) -> Result<Arc<SourceAuthority<S>>, ApplicationError> {
    let profile = self
      .profiles
      .iter()
      .find(|p| p.project_id == input.project_id() && p.binding == input.definition().source)
      .ok_or_else(ApplicationError::capability_unavailable)?;
    Ok(Arc::new(SourceAuthority {
      store: self.store.clone(),
      registry: self.registry.clone(),
      profile: profile
        .clone()
        .for_definition(input.definition(), self.managed_connections.as_ref())?,
      claim: claim.clone(),
      clock: self.clock.clone(),
      settings: self.settings,
      cancellation: self.cancellation.clone(),
    }))
  }
  pub(super) async fn finish_preparation(
    &self,
    claim: WorkSourceScanClaim,
    input: &WorkSourceInput,
    retained: WorkSourcePreparation,
  ) -> Result<(), ApplicationError> {
    require_running(&self.cancellation)?;
    let materialization = self
      .store
      .source_materialization(input.project_id(), retained.id)
      .await?;
    // Expiry is a retained disposition, never permission to renew or remap the original item.
    if retained.snapshot.is_empty()
      || materialization
        .as_ref()
        .is_some_and(|m| m.cleaned_at.is_some() || m.rejection.is_some())
    {
      return Ok(());
    }
    let now = self.clock.now()?;
    if now >= retained.snapshot_expires_at || materialization.as_ref().is_some_and(|m| now >= m.expires_at) {
      return Err(ApplicationError::SourceDependency {
        class: WorkSourceFailure::RetentionPending,
        retry_after_millis: None,
      });
    }
    let authorizer = self.authority(claim.clone(), input)?;
    if retained.intent.is_none() {
      authorizer.authorize(input.id(), input.version()).await?;
      let original = self
        .profiles
        .iter()
        .find(|p| p.project_id == retained.input.project_id() && p.binding == retained.input.definition().source)
        .ok_or_else(ApplicationError::capability_unavailable)?;
      let preparer = FactoryWorkSourcePreparer::new(self.store.clone(), self.bytes.clone(), self.clock.clone());
      if matches!(
        preparer
          .prepare(claim.clone(), retained.id, original.preparation.clone())
          .await?,
        SourcePreparationOutcome::Rejected(_)
      ) {
        return Ok(());
      }
    }
    require_running(&self.cancellation)?;
    let admission = FactorySourceAdmissionHandlers::new(
      self.store.clone(),
      self.revisions.clone(),
      self.authorization.clone(),
      authorizer,
    )
    .with_clock(self.clock.clone());
    admission
      .admit_prepared(AdmitPreparedSourceFactoryWorkCommand {
        input_id: input.id(),
        input_version: input.version(),
        guard: AdmitPreparedWorkSource {
          claim,
          preparation_id: retained.id,
          now: self.clock.now()?,
        },
      })
      .await?;
    Ok(())
  }
}

struct SourceAuthority<S> {
  store: Arc<S>,
  registry: Arc<WorkAdapterRegistry>,
  profile: FactorySourceProfile,
  claim: WorkSourceScanClaim,
  clock: Arc<dyn FactoryExecutionClock>,
  settings: FactorySourceWorkerSettings,
  cancellation: CancellationToken,
}
#[async_trait::async_trait]
impl<S: FactorySourceStore> FactorySourceAuthorizer for SourceAuthority<S> {
  async fn authorize(
    &self,
    id: WorkSourceInputId,
    version: WorkSourceInputVersion,
  ) -> Result<FactorySourceAuthorization, ApplicationError> {
    let members = self
      .store
      .source_scan_members(self.claim.clone(), self.clock.now()?)
      .await?;
    let input = members
      .into_iter()
      .find(|v| {
        v.id() == id
          && v.version() == version
          && v.definition().enabled
          && v.project_id() == self.profile.project_id
          && v.definition().source == self.profile.binding
      })
      .ok_or_else(ApplicationError::invalid)?;
    self.profile.authorize_access().await?;
    use octacity_work_source_protocol::{Request, source};
    require_running(&self.cancellation)?;
    let wire = Request::Source(source::Request {
      protocol_version: source::PROTOCOL_VERSION,
      request_id: format!("source-access-{}", uuid::Uuid::new_v4()),
      command: source::Command::Authorize(source::AccessIntent {
        scan_id: FactoryDigest::sha256("octacity.source-access.v1", &[self.claim.owner.as_str().as_bytes()])
          .to_string(),
        configuration_digest: self.profile.provider_configuration.to_string(),
        resource: self.profile.resource.clone(),
        network_hosts: self.profile.network_hosts.clone(),
      }),
    });
    let source::Outcome::Authorized(_) = execute_source(
      &self.registry,
      &self.profile,
      wire,
      self.settings.operation_timeout,
      self.settings.cancellation_grace,
      self.cancellation.clone(),
    )
    .await?
    else {
      return Err(ApplicationError::capability_unavailable());
    };
    Ok(FactorySourceAuthorization {
      input,
      capability: WorkSourceCapability {
        project_id: self.profile.project_id,
        role: WorkSourceCapabilityRole::Source,
        binding: self.profile.binding.clone(),
      },
      resource_identity: ExternalWorkIdentity::new(self.profile.resource.resource_id.clone())
        .map_err(|_| ApplicationError::invalid())?,
      context: self.profile.context.clone(),
    })
  }
}
