//! Deployment-owned Source profiles; parsing never invokes an adapter or opens a database.
use super::RuntimeAssemblyError;
use octacity_server_application::{
  IssueReporterDeliveryPolicy, MAX_FACTORY_EXECUTOR_BINDINGS, SourcePollingPolicy, SourcePreparationSettings,
  WorkSourceMapping,
};
use octacity_server_factory::{
  FactoryKey, MAX_WORK_SOURCE_PREDICATE_LABELS, NetworkHost, WorkClassification, WorkSourceBinding,
  WorkSourcePredicate, WorkSourceReportingPolicy, WorkSourceStatusMapping,
};
use octacity_server_store::{InstalledIntegration, IntegrationPurpose, RepositorySourceSelection};
use octacity_server_work_source::{InstalledWorkAdapter, WorkAdapterRegistry};
use octacity_work_source_protocol::{ConfigurationReference, CredentialClass};
use serde::{Deserialize, Serialize};
use std::path::Path;

const MAX_SOURCE_PROFILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceProfiles {
  version: u16,
  #[serde(default)]
  pub(super) readers: Vec<SourceReadDeclaration>,
  #[serde(default)]
  pub(super) reporters: Vec<ReporterDeclaration>,
}

impl SourceProfiles {
  pub(super) async fn load(
    path: &Path,
    installations: &[InstalledIntegration],
    registry: &WorkAdapterRegistry,
  ) -> Result<Self, RuntimeAssemblyError> {
    let bytes = super::assembly::read_private_bytes("Source profiles", path, MAX_SOURCE_PROFILE_BYTES).await?;
    let profiles: Self = serde_json::from_slice(&bytes).map_err(|_| RuntimeAssemblyError::InvalidSourceProfiles)?;
    if profiles.version != 1 || profiles.readers.len() + profiles.reporters.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(RuntimeAssemblyError::InvalidSourceProfiles);
    }
    for (index, reader) in profiles.readers.iter().enumerate() {
      if profiles.readers[..index]
        .iter()
        .any(|p| p.identity == reader.identity && p.version == reader.version)
      {
        return Err(RuntimeAssemblyError::InvalidSourceProfiles);
      }
      reader.validate(installations, registry)?;
    }
    for (index, reporter) in profiles.reporters.iter().enumerate() {
      if profiles.reporters[..index]
        .iter()
        .any(|p| p.identity == reporter.identity && p.version == reporter.version)
      {
        return Err(RuntimeAssemblyError::InvalidSourceProfiles);
      }
      reporter.validate(installations, registry)?;
    }
    Ok(profiles)
  }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceReadDeclaration {
  pub(super) identity: FactoryKey,
  pub(super) version: FactoryKey,
  pub(super) read: super::IntegrationCheckProfile,
  pub(super) selection: ConfigurationReference,
  pub(super) readable: WorkSourceReportingPolicy,
  polling: PollingDeclaration,
  #[serde(default)]
  pub(super) preparation: Option<PreparationDeclaration>,
  #[serde(default)]
  pub(super) selection_labels: Option<WorkSourcePredicate>,
  #[serde(default)]
  pub(super) predicate_states: Option<Vec<FactoryKey>>,
}
impl SourceReadDeclaration {
  pub(super) fn native_binding(&self) -> WorkSourceBinding {
    WorkSourceBinding {
      installation: self.read.installation.clone(),
      capability: self.read.capability.capability.clone(),
      configuration: self.read.capability.configuration.clone(),
      resource: self.read.resource_reference.clone(),
    }
  }
  /// Full reviewed settings form the immutable input profile; the native connection stays its parent.
  pub(super) fn profile(
    &self,
    project_id: octacity_server_domain::ProjectId,
    context: octacity_server_application::ManagementRequestContext,
  ) -> Result<super::WorkSourceReadProfile, octacity_server_application::ApplicationError> {
    use octacity_server_factory::FactoryDigest;
    let bytes = serde_json::to_vec(self).map_err(|_| octacity_server_application::ApplicationError::invalid())?;
    Ok(self.read_profile(
      project_id,
      context,
      FactoryDigest::sha256("octacity.source.installed-profile.v1", &[&bytes]),
    ))
  }
  /// Independent task reads retain only read settings; Factory preparation changes
  /// cannot replace their installed identity or invalidate a task's original read pin.
  pub(super) fn observation_profile(
    &self,
    project_id: octacity_server_domain::ProjectId,
    context: octacity_server_application::ManagementRequestContext,
  ) -> Result<super::WorkSourceReadProfile, octacity_server_application::ApplicationError> {
    let bytes = serde_json::to_vec(&(
      &self.identity,
      &self.version,
      &self.read,
      &self.selection,
      &self.readable,
      &self.polling,
      &self.selection_labels,
      &self.predicate_states,
    ))
    .map_err(|_| octacity_server_application::ApplicationError::invalid())?;
    Ok(self.read_profile(
      project_id,
      context,
      octacity_server_factory::FactoryDigest::sha256("octacity.source.observation-profile.v1", &[&bytes]),
    ))
  }
  fn read_profile(
    &self,
    project_id: octacity_server_domain::ProjectId,
    context: octacity_server_application::ManagementRequestContext,
    digest: octacity_server_factory::FactoryDigest,
  ) -> super::WorkSourceReadProfile {
    let mut binding = self.native_binding();
    binding.configuration =
      octacity_server_factory::ImmutableReference::new(self.identity.clone(), self.version.clone(), digest);
    super::WorkSourceReadProfile {
      managed_access: None,
      project_id,
      binding,
      context,
      plugin: self.read.plugin.clone(),
      provider_configuration: self.read.provider_configuration,
      resource: self.read.resource.clone(),
      selection: self.selection.clone(),
      network_hosts: self.read.network_hosts.clone(),
      polling: self.polling.policy(),
    }
  }
  fn validate(
    &self,
    installations: &[InstalledIntegration],
    registry: &WorkAdapterRegistry,
  ) -> Result<(), RuntimeAssemblyError> {
    let invalid = || RuntimeAssemblyError::InvalidSourceProfiles;
    validate_access(&self.read, IntegrationPurpose::TaskRead, installations, registry)?;
    self.selection.validate().map_err(|_| invalid())?;
    self.readable.validate().map_err(|_| invalid())?;
    if let Some(selection) = &self.selection_labels {
      selection.validate().map_err(|_| invalid())?;
      if selection.state.is_none() {
        return Err(invalid());
      }
    }
    if let Some(states) = &self.predicate_states {
      let selection = self.selection_labels.as_ref().ok_or_else(invalid)?;
      if !selection.all_of.is_empty()
        || !selection.any_of.is_empty()
        || !selection.none_of.is_empty()
        || states.len() > MAX_WORK_SOURCE_PREDICATE_LABELS
        || states.iter().collect::<std::collections::BTreeSet<_>>().len() != states.len()
      {
        return Err(invalid());
      }
    }
    if let Some(preparation) = &self.preparation {
      let settings = preparation.settings();
      settings.mapping.validate().map_err(|_| invalid())?;
      if settings.mapping.source != self.native_binding() {
        return Err(invalid());
      }
    }
    self.polling.policy().validate().map_err(|_| invalid())
  }
}

// The installation scope is shared by Source reads and separately authorized Reporter writes.
fn validate_access<'a>(
  profile: &super::IntegrationCheckProfile,
  purpose: IntegrationPurpose,
  installations: &[InstalledIntegration],
  registry: &'a WorkAdapterRegistry,
) -> Result<&'a InstalledWorkAdapter, RuntimeAssemblyError> {
  let invalid = || RuntimeAssemblyError::InvalidSourceProfiles;
  if profile.capability.purpose != purpose
    || profile.network_hosts.is_empty()
    || profile.network_hosts.len() > 256
    || profile
      .network_hosts
      .iter()
      .collect::<std::collections::BTreeSet<_>>()
      .len()
      != profile.network_hosts.len()
  {
    return Err(invalid());
  }
  for host in &profile.network_hosts {
    NetworkHost::new(host.clone()).map_err(|_| invalid())?;
  }
  let mut matching = installations.iter().filter(|i| i.installation == profile.installation);
  let installed = matching.next().ok_or_else(invalid)?;
  if matching.next().is_some() {
    return Err(invalid());
  }
  installed.validate().map_err(|_| invalid())?;
  let resource = installed
    .resources
    .iter()
    .find(|r| r.reference == profile.resource_reference)
    .ok_or_else(invalid)?;
  if installed.adapter != profile.plugin
    || resource.native_id.as_str() != profile.resource.resource_id
    || !resource.capabilities.contains(&profile.capability)
  {
    return Err(invalid());
  }
  let adapter = registry
    .resolve(profile.plugin.identity().as_str(), &profile.plugin.digest().to_string())
    .map_err(|_| invalid())?;
  let (class, configuration) = match purpose {
    IntegrationPurpose::TaskRead => (
      CredentialClass::WorkSourceRead,
      &adapter
        .manifest
        .capabilities
        .source
        .as_ref()
        .ok_or_else(invalid)?
        .configuration_digest,
    ),
    IntegrationPurpose::TaskUpdate => (
      CredentialClass::WorkReporterWrite,
      &adapter
        .manifest
        .capabilities
        .reporter
        .as_ref()
        .ok_or_else(invalid)?
        .configuration_digest,
    ),
    _ => return Err(invalid()),
  };
  profile.resource.validate(class).map_err(|_| invalid())?;
  if *configuration != profile.provider_configuration.to_string() {
    return Err(invalid());
  }
  Ok(adapter)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReporterDeclaration {
  pub(super) identity: FactoryKey,
  pub(super) version: FactoryKey,
  pub(super) write: super::IntegrationCheckProfile,
  pub(super) mapping: ConfigurationReference,
  pub(super) credential_profile: FactoryKey,
  #[serde(default)]
  pub(super) reporting: Option<ReportingDeclaration>,
  #[serde(default)]
  pub(super) issue_delivery: Option<IssueReporterDeliveryPolicy>,
}
impl ReporterDeclaration {
  pub(super) fn profile(
    &self,
    project_id: octacity_server_domain::ProjectId,
  ) -> Result<super::FactoryReporterProfile, octacity_server_application::ApplicationError> {
    super::FactoryReporterProfile::new(
      self.identity.clone(),
      self.version.clone(),
      super::FactoryReporterProfileDraft {
        project_id,
        installation: self.write.installation.clone(),
        capability: self.write.capability.capability.clone(),
        resource: self.write.resource_reference.clone(),
        plugin: self.write.plugin.clone(),
        provider_configuration: self.write.provider_configuration,
        access: self.write.resource.clone(),
        mapping: self.mapping.clone(),
        credential_profile: self.credential_profile.clone(),
        network_hosts: self.write.network_hosts.clone(),
        issue_delivery: self.issue_delivery,
        reporting_policy: self.reporting.as_ref().and_then(|r| r.editable.clone()),
      },
    )
  }
  fn validate(
    &self,
    installations: &[InstalledIntegration],
    registry: &WorkAdapterRegistry,
  ) -> Result<(), RuntimeAssemblyError> {
    let invalid = || RuntimeAssemblyError::InvalidSourceProfiles;
    let adapter = validate_access(&self.write, IntegrationPurpose::TaskUpdate, installations, registry)?;
    self.mapping.validate().map_err(|_| invalid())?;
    if let Some(reporting) = &self.reporting {
      reporting.mapping.validate().map_err(|_| invalid())?;
      if let Some(policy) = &reporting.editable {
        policy.validate_mapping(&reporting.mapping).map_err(|_| invalid())?;
        if !adapter
          .manifest
          .capabilities
          .reporter
          .as_ref()
          .is_some_and(|r| r.custom_effects)
        {
          return Err(invalid());
        }
      }
    }
    if let Some(delivery) = self.issue_delivery {
      delivery.validate().map_err(|_| invalid())?;
      if self.reporting.as_ref().and_then(|r| r.editable.as_ref()).is_none() {
        return Err(invalid());
      }
    }
    Ok(())
  }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReportingDeclaration {
  pub(super) mapping: WorkSourceStatusMapping,
  #[serde(default)]
  pub(super) editable: Option<WorkSourceReportingPolicy>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreparationDeclaration {
  mapping: WorkSourceMapping,
  source: RepositorySourceSelection,
  classification: WorkClassification,
}
impl PreparationDeclaration {
  pub(super) fn settings(&self) -> SourcePreparationSettings {
    SourcePreparationSettings {
      mapping: self.mapping.clone(),
      source: self.source.clone(),
      classification: self.classification.clone(),
    }
  }
}

// Deserialize deployment data separately from the application-owned trusted policy.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PollingDeclaration {
  page_items: u16,
  max_pages: u16,
  max_items: u16,
  max_bytes: u32,
  max_page_bytes: u32,
  overlap_millis: u64,
  reconciliation_interval_millis: u64,
  retention_millis: u64,
}
impl PollingDeclaration {
  fn policy(&self) -> SourcePollingPolicy {
    SourcePollingPolicy {
      page_items: self.page_items,
      max_pages: self.max_pages,
      max_items: self.max_items,
      max_bytes: self.max_bytes,
      max_page_bytes: self.max_page_bytes,
      overlap_millis: self.overlap_millis,
      reconciliation_interval_millis: self.reconciliation_interval_millis,
      retention_millis: self.retention_millis,
    }
  }
}
