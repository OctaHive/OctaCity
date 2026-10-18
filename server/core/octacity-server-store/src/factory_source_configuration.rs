//! Credential-free management intent over the existing immutable input/group journal.
use crate::*;
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};

/// Maximum visible input/group definitions returned in one management page.
pub const MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE: u16 = 64;

/// One reviewed automatic-input creation or replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceInputIntent {
  /// Stable input identity; ordinary edits never allocate another admission scope.
  pub id: WorkSourceInputId,
  /// Current revision, absent only at creation.
  pub expected_version: Option<WorkSourceInputVersion>,
  /// Exact target, code Repository and installed source/reporter settings.
  pub definition: WorkSourceInputDraft,
}
/// Exact member definition frozen by an exclusive group.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceMemberRef {
  /// Stable member identity.
  pub id: WorkSourceInputId,
  /// Reviewed immutable input version.
  pub version: WorkSourceInputVersion,
}
/// One reviewed exclusive group creation or replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceGroupIntent {
  /// Stable group identity.
  pub id: WorkSourceGroupId,
  /// Current group revision, absent only at creation.
  pub expected_version: Option<WorkSourceGroupVersion>,
  /// Exact selected members; no name or provider label dispatch.
  pub members: Vec<WorkSourceMemberRef>,
}
/// Bounded atomic management intent; no task import, refresh or Issue identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceConfigurationIntent {
  /// Owner of every input, group, target and code Repository.
  pub project_id: ProjectId,
  /// At most 64 separately identified pipeline inputs.
  pub inputs: Vec<WorkSourceInputIntent>,
  /// Optional exclusive selection over current immutable inputs.
  pub group: Option<WorkSourceGroupIntent>,
}

/// Safe exact receipt, retained after later input replacement or grant revocation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkSourceConfigurationOutcome {
  /// New publication or exact historical replay.
  pub disposition: MutationDisposition,
  /// Original immutable input and group definitions.
  pub publication: PublishWorkSources,
  /// Original trusted publication time.
  pub published_at: octacity_server_domain::Timestamp,
}
/// Scoped probe that does not resolve current dependencies or renew authority.
#[derive(Clone, Debug)]
pub struct ReplayWorkSourceConfiguration {
  /// Original credential-free client intent.
  pub intent: WorkSourceConfigurationIntent,
  /// Stable caller replay key.
  pub idempotency_key: IdempotencyKey,
}
/// Resolved atomic publication; the store verifies it still represents the reviewed intent.
#[derive(Clone, Debug)]
pub struct PublishWorkSourceConfiguration {
  /// Original selections, distinct from resolver output.
  pub intent: WorkSourceConfigurationIntent,
  /// Resolved immutable definitions to commit together.
  pub publication: PublishWorkSources,
  /// Replay key partitioned by management scope.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative first acceptance time.
  pub published_at: octacity_server_domain::Timestamp,
}
impl PublishWorkSourceConfiguration {
  /// Rejects substituted resolver output and legacy authority at the managed boundary.
  pub fn validate(&self) -> Result<(), StoreError> {
    let intent = &self.intent;
    let publication = &self.publication;
    publication.validate()?;
    if intent.project_id != publication.project_id || intent.inputs.len() != publication.inputs.len() {
      return Err(source_conflict());
    }
    for (selected, resolved) in intent.inputs.iter().zip(&publication.inputs) {
      if selected.id != resolved.input.id()
        || selected.expected_version != resolved.expected_version
        || selected.definition != *resolved.input.definition()
        || selected.definition.connection.is_none()
      {
        return Err(source_conflict());
      }
    }
    match (&intent.group, &publication.group) {
      (None, None) => {}
      (Some(selected), Some(resolved))
        if selected.id == resolved.group.id()
          && selected.expected_version == resolved.expected_version
          && selected.members
            == resolved
              .group
              .members()
              .iter()
              .map(|m| WorkSourceMemberRef {
                id: m.id(),
                version: m.version(),
              })
              .collect::<Vec<_>>() => {}
      _ => return Err(source_conflict()),
    }
    Ok(())
  }
  /// Builds the retained receipt without re-reading mutable heads.
  pub fn outcome(&self) -> WorkSourceConfigurationOutcome {
    WorkSourceConfigurationOutcome {
      disposition: MutationDisposition::Applied,
      publication: self.publication.clone(),
      published_at: self.published_at,
    }
  }
}
/// Management journal over the existing Source store; publication also retains audit/outbox facts.
#[async_trait::async_trait]
pub trait FactorySourceConfigurationStore: FactorySourceStore {
  /// Lists visible current definitions; visibility precedes ordering, limits and cursors.
  async fn list_work_source_configurations(
    &self,
    _request: ListWorkSourceConfigurations,
  ) -> Result<WorkSourceConfigurationPage, StoreError> {
    Err(StoreError::Unavailable)
  }
  /// Returns an original exact receipt, independent of mutable dependency resolution.
  async fn replay_work_source_configuration(
    &self,
    request: ManagementMutation<ReplayWorkSourceConfiguration>,
  ) -> Result<Option<WorkSourceConfigurationOutcome>, StoreError>;
  /// Commits inputs, group, replay, audit and outbox together or none of them.
  async fn publish_work_source_configuration(
    &self,
    request: ManagementMutation<PublishWorkSourceConfiguration>,
  ) -> Result<WorkSourceConfigurationOutcome, StoreError>;
}

/// One immutable source configuration, without task content or execution state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "definition", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkSourceConfigurationRecord {
  /// One pipeline input version.
  Input(Box<WorkSourceInput>),
  /// One exclusive group with frozen member definitions.
  Group(WorkSourceGroup),
}
impl WorkSourceConfigurationRecord {
  /// Stable read position; this carries no scan claim or execution permission.
  pub fn target(&self) -> WorkSourceScanTarget {
    match self {
      Self::Input(input) => WorkSourceScanTarget::Input {
        id: input.id(),
        version: input.version(),
      },
      Self::Group(group) => WorkSourceScanTarget::Group {
        id: group.id(),
        version: group.version(),
      },
    }
  }
  /// Members whose dependency visibility must be checked before returning the record.
  pub fn inputs(&self) -> &[WorkSourceInput] {
    match self {
      Self::Input(input) => std::slice::from_ref(input.as_ref()),
      Self::Group(group) => group.members(),
    }
  }
}
/// Dependency scopes derived from current management policy, never from the client.
#[derive(Clone, Debug)]
pub struct WorkSourceConfigurationVisibility {
  /// Visible Factory target identities.
  pub configurations: FactoryConfigurationListVisibility,
  /// Independently visible code Repositories.
  pub repositories: RepositoryListVisibility,
  /// Visible managed connections; legacy installed inputs require no managed connection.
  pub connections: IntegrationConnectionListVisibility,
}
impl WorkSourceConfigurationVisibility {
  /// Checks every frozen dependency without looking up mutable aliases.
  pub fn allows(&self, input: &WorkSourceInput) -> bool {
    let definition = input.definition();
    self.configurations.allows(&definition.target.id())
      && self.repositories.allows(&definition.repository.id)
      && definition
        .connection
        .as_ref()
        .is_none_or(|c| self.connections.allows(&c.id))
  }
}
/// A bounded definition page; private snapshots, admission and polling cursors are absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkSourceConfigurationPage {
  /// Visible current inputs/groups, or retained input versions for connection dependency review.
  pub items: Vec<WorkSourceConfigurationRecord>,
  /// Last visible record when another visible record exists.
  pub next_cursor: Option<WorkSourceScanTarget>,
}
/// Store-owned read constraints, including dependency visibility before pagination.
#[derive(Clone, Debug)]
pub struct ListWorkSourceConfigurations {
  /// Connection dependency review includes every retained input version, not group duplicates.
  /// None retains ordinary current-definition discovery.
  pub connection_dependency: Option<octacity_server_domain::IntegrationId>,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exclusive kind/identity position; dependency review additionally orders by version.
  pub after: Option<WorkSourceScanTarget>,
  /// At most 64 returned configurations.
  pub limit: u16,
  /// Policy-derived dependency scopes.
  pub visibility: WorkSourceConfigurationVisibility,
}
impl ListWorkSourceConfigurations {
  /// Rechecks caller-independent read bounds at the store boundary.
  pub fn validate(&self) -> Result<(), StoreError> {
    if !(1..=MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE).contains(&self.limit) {
      return Err(source_conflict());
    }
    Ok(())
  }
}

/// Checks managed input authority from one store snapshot/transaction at commit.
pub fn validate_managed_work_source(
  input: &WorkSourceInput,
  metadata: &PublishedIntegrationConnection,
  access: &PublishedIntegrationProjectAccess,
  administration: &IntegrationAdministrationState,
) -> Result<(), StoreError> {
  let selected = input.definition().connection.as_ref().ok_or_else(source_conflict)?;
  if metadata.id != selected.id
    || metadata.version != selected.version
    || access.connection_id != selected.id
    || access.project_id != input.project_id()
    || access.connection_version != selected.version
    || access.version != selected.access_version
    || administration.id != selected.id
    || !administration.enabled
    || administration.active_version != Some(selected.version)
  {
    return Err(source_conflict());
  }
  let definition = input.definition();
  if definition.reporter.is_some() != selected.reporter_configuration.is_some() {
    return Err(source_conflict());
  }
  for (binding, purpose) in std::iter::once((&definition.source, IntegrationPurpose::TaskRead)).chain(
    definition
      .reporter
      .as_ref()
      .map(|reporter| (reporter, IntegrationPurpose::TaskUpdate)),
  ) {
    let resource = metadata
      .definition
      .resources
      .iter()
      .find(|r| r.reference == binding.resource)
      .ok_or_else(source_conflict)?;
    let capability = resource
      .capabilities
      .iter()
      .find(|c| c.purpose == purpose)
      .ok_or_else(source_conflict)?;
    let configuration = match purpose {
      IntegrationPurpose::TaskRead => &selected.source_configuration,
      IntegrationPurpose::TaskUpdate => selected.reporter_configuration.as_ref().ok_or_else(source_conflict)?,
      _ => return Err(source_conflict()),
    };
    if metadata.definition.installation != binding.installation
      || capability.capability != binding.capability
      || capability.configuration != *configuration
      || !access
        .resources
        .iter()
        .any(|r| r.reference == binding.resource && r.purposes.contains(&purpose))
      || !administration
        .credentials
        .iter()
        .any(|c| c.reference == capability.credential && c.configured)
    {
      return Err(source_conflict());
    }
  }
  Ok(())
}
