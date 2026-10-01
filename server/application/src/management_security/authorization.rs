use std::{collections::BTreeSet, fmt};

use super::{ManagementSecurityError, is_canonical_text};

/// Maximum UTF-8 bytes in an opaque management resource identity.
pub const MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES: usize = 256;
/// Maximum resources in one restricted visibility grant.
pub const MAX_MANAGEMENT_VISIBILITY_RESOURCES: usize = octacity_server_store::MAX_READ_VISIBILITY_IDENTITIES;

/// Stable management capability independent of HTTP methods and route names.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementAction {
  /// Observe one resource or its current state.
  View,
  /// Search a bounded resource projection.
  Search,
  /// Create a new mutable or versioned resource.
  Create,
  /// Change mutable resource state or placement.
  Update,
  /// Publish a new immutable resource version.
  Publish,
  /// Delete a resource allowed to be removed.
  Delete,
  /// Start a Build or another bounded operation.
  Execute,
  /// Cancel active work.
  Cancel,
  /// Retry terminal or recoverable work.
  Retry,
  /// Download protected immutable bytes.
  Download,
  /// Perform a resource-specific administrative operation.
  Administer,
}

impl ManagementAction {
  /// Complete closed action vocabulary used by coverage and policy tests.
  pub const ALL: [Self; 11] = [
    Self::View,
    Self::Search,
    Self::Create,
    Self::Update,
    Self::Publish,
    Self::Delete,
    Self::Execute,
    Self::Cancel,
    Self::Retry,
    Self::Download,
    Self::Administer,
  ];
}

/// Stable kind of resource protected by management authorization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementResourceKind {
  /// Running management control plane and its safe deployment metadata.
  ControlPlane,
  /// Hierarchical Project.
  Project,
  /// Versioned Project policy.
  ProjectPolicy,
  /// Immutable Pipeline definition and versions.
  Pipeline,
  /// Versioned source Repository definition.
  Repository,
  /// Versioned Build Configuration.
  BuildConfiguration,
  /// Manual, scheduled, external, or internal Trigger definition.
  Trigger,
  /// Durable schedule definition.
  Schedule,
  /// Immutable Build request and state.
  Build,
  /// One Build execution Attempt.
  Attempt,
  /// One materialized Pipeline Job.
  Job,
  /// Ordered Job event stream.
  JobEvent,
  /// Indexed Build log projection.
  BuildLog,
  /// Logical Build Artifact or report.
  Artifact,
  /// Remote cache session diagnostic.
  CacheSession,
  /// Static Agent Pool.
  AgentPool,
  /// Registered Agent.
  Agent,
  /// Single-use Agent enrollment credential.
  AgentEnrollment,
  /// Provider-backed webhook integration.
  WebhookIntegration,
  /// Build Result retention state or hold.
  Retention,
  /// Immutable structured audit fact.
  AuditFact,
}

impl ManagementResourceKind {
  /// Complete closed resource-kind vocabulary used by coverage and policy tests.
  pub const ALL: [Self; 21] = [
    Self::ControlPlane,
    Self::Project,
    Self::ProjectPolicy,
    Self::Pipeline,
    Self::Repository,
    Self::BuildConfiguration,
    Self::Trigger,
    Self::Schedule,
    Self::Build,
    Self::Attempt,
    Self::Job,
    Self::JobEvent,
    Self::BuildLog,
    Self::Artifact,
    Self::CacheSession,
    Self::AgentPool,
    Self::Agent,
    Self::AgentEnrollment,
    Self::WebhookIntegration,
    Self::Retention,
    Self::AuditFact,
  ];
}

/// Structural resource pattern declared by one management operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementResourcePattern {
  /// A top-level collection.
  Collection,
  /// One resource with a known identity.
  Instance,
  /// A collection owned by the named resource kind.
  OwnedCollection(ManagementResourceKind),
  /// Either a top-level collection or a collection owned by the named kind.
  CollectionOrOwnedCollection(ManagementResourceKind),
}

/// Static action and resource contract declared by a management request type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementAuthorizationMapping {
  action: ManagementAction,
  resource_kind: ManagementResourceKind,
  resource_pattern: ManagementResourcePattern,
}

impl ManagementAuthorizationMapping {
  /// Declares a top-level collection mapping.
  #[must_use]
  pub const fn collection(action: ManagementAction, resource_kind: ManagementResourceKind) -> Self {
    Self::new(action, resource_kind, ManagementResourcePattern::Collection)
  }

  /// Declares an instance mapping.
  #[must_use]
  pub const fn instance(action: ManagementAction, resource_kind: ManagementResourceKind) -> Self {
    Self::new(action, resource_kind, ManagementResourcePattern::Instance)
  }

  /// Declares an owned-collection mapping.
  #[must_use]
  pub const fn owned_collection(
    action: ManagementAction,
    resource_kind: ManagementResourceKind,
    owner_kind: ManagementResourceKind,
  ) -> Self {
    Self::new(
      action,
      resource_kind,
      ManagementResourcePattern::OwnedCollection(owner_kind),
    )
  }

  /// Declares a collection that may be top-level or nested below one owner kind.
  #[must_use]
  pub const fn collection_or_owned_collection(
    action: ManagementAction,
    resource_kind: ManagementResourceKind,
    owner_kind: ManagementResourceKind,
  ) -> Self {
    Self::new(
      action,
      resource_kind,
      ManagementResourcePattern::CollectionOrOwnedCollection(owner_kind),
    )
  }

  const fn new(
    action: ManagementAction,
    resource_kind: ManagementResourceKind,
    resource_pattern: ManagementResourcePattern,
  ) -> Self {
    Self {
      action,
      resource_kind,
      resource_pattern,
    }
  }

  /// Returns the protected capability.
  #[must_use]
  pub const fn action(self) -> ManagementAction {
    self.action
  }

  /// Returns the protected resource kind.
  #[must_use]
  pub const fn resource_kind(self) -> ManagementResourceKind {
    self.resource_kind
  }

  /// Returns the required structural resource pattern.
  #[must_use]
  pub const fn resource_pattern(self) -> ManagementResourcePattern {
    self.resource_pattern
  }

  /// Returns whether this declaration names a supported resource shape.
  #[must_use]
  pub const fn is_supported(self) -> bool {
    match self.resource_pattern {
      ManagementResourcePattern::Collection => supports_collection(self.resource_kind),
      ManagementResourcePattern::Instance => supports_instance(self.resource_kind),
      ManagementResourcePattern::OwnedCollection(owner_kind) => {
        supports_owned_collection(self.resource_kind, owner_kind)
      }
      ManagementResourcePattern::CollectionOrOwnedCollection(owner_kind) => {
        supports_collection(self.resource_kind) && supports_owned_collection(self.resource_kind, owner_kind)
      }
    }
  }

  pub(crate) fn accepts(self, resource: &ManagementResource) -> bool {
    if resource.kind != self.resource_kind {
      return false;
    }
    match (self.resource_pattern, &resource.scope) {
      (ManagementResourcePattern::Collection, ManagementResourceScope::Collection)
      | (ManagementResourcePattern::Instance, ManagementResourceScope::Instance(_))
      | (ManagementResourcePattern::CollectionOrOwnedCollection(_), ManagementResourceScope::Collection) => true,
      (
        ManagementResourcePattern::OwnedCollection(expected)
        | ManagementResourcePattern::CollectionOrOwnedCollection(expected),
        ManagementResourceScope::OwnedCollection { owner_kind, .. },
      ) => expected == *owner_kind,
      _ => false,
    }
  }
}

/// Bounded opaque identity retained with a typed management resource kind.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementResourceIdentity(String);

impl ManagementResourceIdentity {
  /// Constructs a canonical non-empty resource identity.
  pub fn new(value: impl Into<String>) -> Result<Self, ManagementSecurityError> {
    let value = value.into();
    if !is_canonical_text(&value, MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES) {
      return Err(ManagementSecurityError::InvalidResourceIdentity);
    }
    Ok(Self(value))
  }

  /// Borrows the opaque identity for typed adapter conversion.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Debug for ManagementResourceIdentity {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("ManagementResourceIdentity(<redacted>)")
  }
}

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum ManagementResourceScope {
  Collection,
  Instance(ManagementResourceIdentity),
  OwnedCollection {
    owner_kind: ManagementResourceKind,
    owner_identity: ManagementResourceIdentity,
  },
}

/// Typed resource description evaluated by management authorization policy.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagementResource {
  kind: ManagementResourceKind,
  scope: ManagementResourceScope,
}

/// Result of validating a concrete management resource description.
pub type ManagementResourceResult = Result<ManagementResource, ManagementSecurityError>;

impl ManagementResource {
  /// Describes a top-level collection without fabricating a resource identity.
  pub fn collection(kind: ManagementResourceKind) -> Result<Self, ManagementSecurityError> {
    if !supports_collection(kind) {
      return Err(ManagementSecurityError::UnsupportedResourceShape);
    }
    Ok(Self {
      kind,
      scope: ManagementResourceScope::Collection,
    })
  }

  /// Describes one resource whose opaque identity is already known.
  pub fn instance(
    kind: ManagementResourceKind,
    identity: ManagementResourceIdentity,
  ) -> Result<Self, ManagementSecurityError> {
    if !supports_instance(kind) {
      return Err(ManagementSecurityError::UnsupportedResourceShape);
    }
    Ok(Self {
      kind,
      scope: ManagementResourceScope::Instance(identity),
    })
  }

  /// Describes a collection owned by another typed resource.
  pub fn owned_collection(
    kind: ManagementResourceKind,
    owner_kind: ManagementResourceKind,
    owner_identity: ManagementResourceIdentity,
  ) -> Result<Self, ManagementSecurityError> {
    if !supports_owned_collection(kind, owner_kind) {
      return Err(ManagementSecurityError::UnsupportedResourceShape);
    }
    Ok(Self {
      kind,
      scope: ManagementResourceScope::OwnedCollection {
        owner_kind,
        owner_identity,
      },
    })
  }

  /// Returns the protected resource kind.
  #[must_use]
  pub const fn kind(&self) -> ManagementResourceKind {
    self.kind
  }

  /// Returns whether this description protects a top-level collection.
  #[must_use]
  pub const fn is_collection(&self) -> bool {
    matches!(self.scope, ManagementResourceScope::Collection)
  }

  /// Borrows the resource identity for an instance description.
  #[must_use]
  pub fn identity(&self) -> Option<&ManagementResourceIdentity> {
    match &self.scope {
      ManagementResourceScope::Instance(identity) => Some(identity),
      ManagementResourceScope::Collection | ManagementResourceScope::OwnedCollection { .. } => None,
    }
  }

  /// Returns the owner kind and identity for an owned collection.
  #[must_use]
  pub fn owner(&self) -> Option<(ManagementResourceKind, &ManagementResourceIdentity)> {
    match &self.scope {
      ManagementResourceScope::OwnedCollection {
        owner_kind,
        owner_identity,
      } => Some((*owner_kind, owner_identity)),
      ManagementResourceScope::Collection | ManagementResourceScope::Instance(_) => None,
    }
  }
}

const fn supports_collection(kind: ManagementResourceKind) -> bool {
  matches!(
    kind,
    ManagementResourceKind::ControlPlane
      | ManagementResourceKind::Project
      | ManagementResourceKind::Trigger
      | ManagementResourceKind::AgentPool
      | ManagementResourceKind::Agent
      | ManagementResourceKind::AuditFact
  )
}

const fn supports_instance(kind: ManagementResourceKind) -> bool {
  matches!(
    kind,
    ManagementResourceKind::Project
      | ManagementResourceKind::Pipeline
      | ManagementResourceKind::Repository
      | ManagementResourceKind::BuildConfiguration
      | ManagementResourceKind::Trigger
      | ManagementResourceKind::Schedule
      | ManagementResourceKind::Build
      | ManagementResourceKind::Attempt
      | ManagementResourceKind::Job
      | ManagementResourceKind::Artifact
      | ManagementResourceKind::CacheSession
      | ManagementResourceKind::AgentPool
      | ManagementResourceKind::Agent
      | ManagementResourceKind::AuditFact
      | ManagementResourceKind::WebhookIntegration
      | ManagementResourceKind::Retention
  )
}

const fn supports_owned_collection(kind: ManagementResourceKind, owner_kind: ManagementResourceKind) -> bool {
  matches!(
    (kind, owner_kind),
    (ManagementResourceKind::Project, ManagementResourceKind::Project)
      | (ManagementResourceKind::ProjectPolicy, ManagementResourceKind::Project)
      | (ManagementResourceKind::Pipeline, ManagementResourceKind::Project)
      | (ManagementResourceKind::Repository, ManagementResourceKind::Project)
      | (
        ManagementResourceKind::BuildConfiguration,
        ManagementResourceKind::Project
      )
      | (ManagementResourceKind::Trigger, ManagementResourceKind::Project)
      | (
        ManagementResourceKind::Trigger,
        ManagementResourceKind::BuildConfiguration
      )
      | (
        ManagementResourceKind::Schedule,
        ManagementResourceKind::BuildConfiguration
      )
      | (ManagementResourceKind::Build, ManagementResourceKind::Project)
      | (ManagementResourceKind::JobEvent, ManagementResourceKind::Job)
      | (ManagementResourceKind::BuildLog, ManagementResourceKind::Project)
      | (ManagementResourceKind::Artifact, ManagementResourceKind::Build)
      | (ManagementResourceKind::CacheSession, ManagementResourceKind::Build)
      | (ManagementResourceKind::Agent, ManagementResourceKind::AgentPool)
      | (
        ManagementResourceKind::AgentEnrollment,
        ManagementResourceKind::AgentPool
      )
      | (
        ManagementResourceKind::WebhookIntegration,
        ManagementResourceKind::Repository
      )
      | (ManagementResourceKind::Retention, ManagementResourceKind::Build)
  )
}

pub(crate) fn resource_identity(
  value: impl fmt::Display,
) -> Result<ManagementResourceIdentity, ManagementSecurityError> {
  ManagementResourceIdentity::new(value.to_string())
}

pub(crate) fn instance_resource(
  kind: ManagementResourceKind,
  identity: impl fmt::Display,
) -> Result<ManagementResource, ManagementSecurityError> {
  ManagementResource::instance(kind, resource_identity(identity)?)
}

pub(crate) fn owned_collection_resource(
  kind: ManagementResourceKind,
  owner_kind: ManagementResourceKind,
  owner_identity: impl fmt::Display,
) -> Result<ManagementResource, ManagementSecurityError> {
  ManagementResource::owned_collection(kind, owner_kind, resource_identity(owner_identity)?)
}

impl fmt::Debug for ManagementResource {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let scope = match self.scope {
      ManagementResourceScope::Collection => "collection",
      ManagementResourceScope::Instance(_) => "instance:<redacted>",
      ManagementResourceScope::OwnedCollection { .. } => "owned_collection:<redacted>",
    };
    formatter
      .debug_struct("ManagementResource")
      .field("kind", &self.kind)
      .field("scope", &scope)
      .finish()
  }
}

#[derive(Clone, Eq, PartialEq)]
enum Visibility {
  All,
  None,
  Restricted(BTreeSet<ManagementResource>),
}

/// Stable classification of an authorization-derived read visibility scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ManagementVisibilityKind {
  /// Every resource matching the application query is visible.
  All,
  /// No resource is visible, producing a valid empty result.
  None,
  /// Only the explicitly bounded resource set is visible.
  Restricted,
}

/// Exhaustive borrowed view of an authorization-derived read visibility scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagementVisibilityView<'a> {
  /// Every resource matching the application query is visible.
  All,
  /// No resource is visible, producing a valid empty result.
  None,
  /// Only the explicitly bounded resource set is visible.
  Restricted(&'a BTreeSet<ManagementResource>),
}

/// Authorization-derived visibility applied by read adapters before result shaping.
///
/// Read adapters apply this constraint before ordering, cursor comparison,
/// pagination, counts, snippets, freshness metadata, and provider-side limits.
#[derive(Clone, Eq, PartialEq)]
pub struct ManagementVisibility(Visibility);

impl ManagementVisibility {
  /// Constructs unrestricted visibility for the trusted-network policy.
  #[must_use]
  pub const fn all() -> Self {
    Self(Visibility::All)
  }

  /// Constructs an explicit empty visibility scope.
  #[must_use]
  pub const fn none() -> Self {
    Self(Visibility::None)
  }

  /// Constructs a bounded non-empty resource visibility set.
  pub fn restricted(resources: impl IntoIterator<Item = ManagementResource>) -> Result<Self, ManagementSecurityError> {
    let mut restricted = BTreeSet::new();
    for resource in resources {
      if !restricted.insert(resource) {
        return Err(ManagementSecurityError::DuplicateVisibilityResource);
      }
      if restricted.len() > MAX_MANAGEMENT_VISIBILITY_RESOURCES {
        return Err(ManagementSecurityError::VisibilityTooLarge);
      }
    }
    if restricted.is_empty() {
      return Err(ManagementSecurityError::EmptyVisibility);
    }
    Ok(Self(Visibility::Restricted(restricted)))
  }

  /// Returns the stable visibility classification.
  #[must_use]
  pub const fn kind(&self) -> ManagementVisibilityKind {
    match self.0 {
      Visibility::All => ManagementVisibilityKind::All,
      Visibility::None => ManagementVisibilityKind::None,
      Visibility::Restricted(_) => ManagementVisibilityKind::Restricted,
    }
  }

  /// Borrows an exhaustive view that preserves the distinction between unrestricted and empty visibility.
  #[must_use]
  pub const fn view(&self) -> ManagementVisibilityView<'_> {
    match &self.0 {
      Visibility::All => ManagementVisibilityView::All,
      Visibility::None => ManagementVisibilityView::None,
      Visibility::Restricted(resources) => ManagementVisibilityView::Restricted(resources),
    }
  }
}

impl fmt::Debug for ManagementVisibility {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let mut debug = formatter.debug_struct("ManagementVisibility");
    debug.field("kind", &self.kind());
    if let Visibility::Restricted(resources) = &self.0 {
      debug.field("resource_count", &resources.len());
    }
    debug.finish()
  }
}

/// Successful policy decision passed to a management application handler.
#[derive(Clone, Eq, PartialEq)]
pub struct ManagementAuthorizationGrant {
  visibility: ManagementVisibility,
}

impl ManagementAuthorizationGrant {
  /// Constructs a grant carrying the policy-selected read visibility.
  #[must_use]
  pub const fn new(visibility: ManagementVisibility) -> Self {
    Self { visibility }
  }

  /// Borrows the visibility that read adapters must apply before result shaping.
  #[must_use]
  pub const fn visibility(&self) -> &ManagementVisibility {
    &self.visibility
  }
}

impl fmt::Debug for ManagementAuthorizationGrant {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("ManagementAuthorizationGrant")
      .field("visibility", &self.visibility)
      .finish()
  }
}

/// Opaque policy denial that intentionally exposes no policy reason or resource details.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ManagementAuthorizationDenial(());

impl ManagementAuthorizationDenial {
  /// Constructs the stable forbidden decision.
  #[must_use]
  pub const fn forbidden() -> Self {
    Self(())
  }
}

impl fmt::Debug for ManagementAuthorizationDenial {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("ManagementAuthorizationDenial")
  }
}

impl fmt::Display for ManagementAuthorizationDenial {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("management request is forbidden")
  }
}

impl std::error::Error for ManagementAuthorizationDenial {}
