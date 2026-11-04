//! Reusable connection metadata. Installed declarations are trusted facts, never grants.
use crate::{StoreError, StoreInputError, StoreOperation};
use octacity_server_domain::IntegrationName;
use octacity_server_factory::{ExternalWorkIdentity, FactoryDigest, FactoryKey, ImmutableReference, WorkSourceBinding};
use octacity_server_secrets::LogicalSecretReference;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// The closed purpose vocabulary bounds validation work before duplicate checks.
const MAX_PURPOSES: usize = 4;

/// Independently authorized purposes; remote token permission confers none of them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationPurpose {
  /// Discover external task input.
  TaskRead,
  /// Project explicitly accepted task effects.
  TaskUpdate,
  /// Read immutable repository content.
  RepositoryRead,
  /// Publish configured delivery effects.
  Delivery,
}
/// One installed capability and its private logical credential reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationCapability {
  /// Declared purpose, independent of Project grants.
  pub purpose: IntegrationPurpose,
  /// Exact installed executable capability.
  pub capability: ImmutableReference,
  /// Exact installed selector/mapping or provider configuration.
  pub configuration: ImmutableReference,
  /// Logical reference only; no secret bytes or filesystem path is accepted here.
  pub credential: LogicalSecretReference,
}
/// A native provider container, distinct from an OctaCity Project/code Repository.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationResource {
  /// Exact operator resource definition.
  pub reference: ImmutableReference,
  /// Stable opaque native container identity.
  pub native_id: ExternalWorkIdentity,
  /// Provider-owned display/lookup context, never an admission key.
  pub locator: ExternalWorkIdentity,
  /// Capabilities actually installed for this resource.
  pub capabilities: Vec<IntegrationCapability>,
}
/// Trusted declaration supplied by the verified installation composition.
/// Receiving this metadata does not prove current remote access or start any worker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledIntegration {
  /// Original installation namespace, preserved during explicit adoption.
  pub installation: ImmutableReference,
  /// Open provider identity; no built-in GitHub/GitLab/Jira discriminator.
  pub provider: FactoryKey,
  /// Exact verified adapter/executable pin.
  pub adapter: ImmutableReference,
  /// Explicit operator-approved service endpoints, without URLs from provider data.
  pub endpoints: Vec<String>,
  /// Authentication mechanisms advertised by this installed adapter.
  pub authentication: Vec<FactoryKey>,
  /// Bounded installed native resources and independent capabilities.
  pub resources: Vec<IntegrationResource>,
}
/// Public selection without credentials, provider payloads or executable paths.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationResourceSelection {
  /// Exact installed native resource definition.
  pub reference: ImmutableReference,
  /// Explicit subset of installed capabilities; this is not a Project grant.
  pub purposes: Vec<IntegrationPurpose>,
}
/// Immutable publication intent; pipeline routing and credentials belong elsewhere.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationConnectionDraft {
  /// Human-readable reusable name, never a replay identity.
  pub name: IntegrationName,
  /// Exact installed declaration to resolve.
  pub installation: ImmutableReference,
  /// Canonical service endpoint selected from the trusted ceiling.
  pub endpoint: String,
  /// One actually installed authentication mechanism.
  pub authentication: FactoryKey,
  /// Explicit bounded native-resource and purpose selections.
  pub resources: Vec<IntegrationResourceSelection>,
}
/// Frozen safe metadata. It confers no Project access, readiness or input activation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationConnectionDefinition {
  /// Stable administrative display name.
  pub name: IntegrationName,
  /// Open installed provider identity.
  pub provider: FactoryKey,
  /// Original installation identity.
  pub installation: ImmutableReference,
  /// Exact verified adapter pin.
  pub adapter: ImmutableReference,
  /// Original approved endpoint.
  pub endpoint: String,
  /// Selected installed authentication mechanism.
  pub authentication: FactoryKey,
  /// Frozen native resources and purpose-specific logical references.
  pub resources: Vec<IntegrationResource>,
}
impl IntegrationConnectionDefinition {
  /// Resolves public choices against trusted installed facts, without provider/credential I/O.
  pub fn resolve(draft: &IntegrationConnectionDraft, installed: &InstalledIntegration) -> Result<Self, StoreError> {
    installed.validate()?;
    if draft.installation != installed.installation
      || !installed.endpoints.contains(&draft.endpoint)
      || !installed.authentication.contains(&draft.authentication)
      || draft.resources.is_empty()
      || draft.resources.len() > MAX_INTEGRATION_RESOURCES
    {
      return Err(invalid_connection());
    }
    let mut seen = BTreeSet::new();
    let resources = draft
      .resources
      .iter()
      .map(|selected| {
        if !seen.insert(&selected.reference)
          || selected.purposes.is_empty()
          || selected.purposes.len() > MAX_PURPOSES
          || selected.purposes.iter().collect::<BTreeSet<_>>().len() != selected.purposes.len()
        {
          return Err(invalid_connection());
        }
        let resource = installed
          .resources
          .iter()
          .find(|resource| resource.reference == selected.reference)
          .ok_or_else(invalid_connection)?;
        if selected
          .purposes
          .iter()
          .any(|purpose| !resource.capabilities.iter().any(|entry| entry.purpose == *purpose))
        {
          return Err(invalid_connection());
        }
        let mut resource = resource.clone();
        resource
          .capabilities
          .retain(|capability| selected.purposes.contains(&capability.purpose));
        Ok(resource)
      })
      .collect::<Result<_, StoreError>>()?;
    Ok(Self {
      name: draft.name.clone(),
      provider: installed.provider.clone(),
      installation: installed.installation.clone(),
      adapter: installed.adapter.clone(),
      endpoint: draft.endpoint.clone(),
      authentication: draft.authentication.clone(),
      resources,
    })
  }
  /// Computes the canonical identity of the frozen metadata, also checked on durable reads.
  pub fn content_digest(&self) -> Result<FactoryDigest, StoreError> {
    let bytes = serde_json::to_vec(self).map_err(|_| StoreError::Unavailable)?;
    Ok(FactoryDigest::sha256("octacity.integration-connection.v1", &[&bytes]))
  }
  /// Recovers an existing task binding exactly, without modifying its identity or authorizing use.
  pub fn work_binding(&self, resource: &ImmutableReference, purpose: IntegrationPurpose) -> Option<WorkSourceBinding> {
    if !matches!(purpose, IntegrationPurpose::TaskRead | IntegrationPurpose::TaskUpdate) {
      return None;
    }
    let resource = self.resources.iter().find(|entry| entry.reference == *resource)?;
    let capability = resource.capabilities.iter().find(|entry| entry.purpose == purpose)?;
    Some(WorkSourceBinding {
      installation: self.installation.clone(),
      capability: capability.capability.clone(),
      configuration: capability.configuration.clone(),
      resource: resource.reference.clone(),
    })
  }
}
pub(crate) fn invalid_connection() -> StoreError {
  StoreError::invalid(
    StoreOperation::PublishIntegrationConnection,
    StoreInputError::InvalidIntegrationConnection,
  )
}

/// Hard resource ceiling per installed declaration and immutable connection version.
pub const MAX_INTEGRATION_RESOURCES: usize = 64;
/// Maximum approved endpoint choices advertised by one installation.
pub const MAX_INTEGRATION_ENDPOINTS: usize = 16;
/// Maximum authentication mechanisms advertised by one installation.
pub const MAX_INTEGRATION_AUTHENTICATION_METHODS: usize = 16;
/// Maximum canonical endpoint length, including any approved base path.
pub const MAX_INTEGRATION_ENDPOINT_BYTES: usize = 2048;
impl InstalledIntegration {
  /// Validates the trusted declaration before any public selection or publication.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.endpoints.is_empty()
      || self.endpoints.len() > MAX_INTEGRATION_ENDPOINTS
      || self.authentication.is_empty()
      || self.authentication.len() > MAX_INTEGRATION_AUTHENTICATION_METHODS
      || self.authentication.iter().collect::<BTreeSet<_>>().len() != self.authentication.len()
      || self.endpoints.iter().collect::<BTreeSet<_>>().len() != self.endpoints.len()
      || self.resources.is_empty()
      || self.resources.len() > MAX_INTEGRATION_RESOURCES
    {
      return Err(invalid_connection());
    }
    for endpoint in &self.endpoints {
      validate_endpoint(endpoint)?;
    }
    let mut resources = BTreeSet::new();
    let mut native = BTreeSet::new();
    for resource in &self.resources {
      if !resources.insert(&resource.reference)
        || !native.insert(resource.native_id.as_str())
        || resource.capabilities.is_empty()
        || resource.capabilities.len() > MAX_PURPOSES
        || resource
          .capabilities
          .iter()
          .map(|entry| entry.purpose)
          .collect::<BTreeSet<_>>()
          .len()
          != resource.capabilities.len()
      {
        return Err(invalid_connection());
      }
    }
    Ok(())
  }
}
fn validate_endpoint(endpoint: &str) -> Result<(), StoreError> {
  if endpoint.len() > MAX_INTEGRATION_ENDPOINT_BYTES {
    return Err(invalid_connection());
  }
  let parsed = url::Url::parse(endpoint).map_err(|_| invalid_connection())?;
  if parsed.as_str() != endpoint
    || parsed.scheme() != "https"
    || parsed.host_str().is_none()
    || !parsed.username().is_empty()
    || parsed.password().is_some()
    || parsed.query().is_some()
    || parsed.fragment().is_some()
  {
    return Err(invalid_connection());
  }
  Ok(())
}
