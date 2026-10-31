//! Provider-neutral input publication; provider configuration stays behind exact references.

use octacity_server_domain::{ProjectId, RepositoryId, RepositoryVersion};
use serde::{Deserialize, Serialize};

use crate::{
  FactoryConfiguration, FactoryConfigurationRef, FactoryDigest, FactoryError, FlowDefinitionRef, ImmutableReference,
  WorkSourceInputId, WorkSourceInputVersion,
};

/// Exact operator-installed capability, configuration and provider resource selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceBinding {
  /// Immutable installation identity; never a token or credential value.
  pub installation: ImmutableReference,
  /// Independently advertised Source or Reporter capability version.
  pub capability: ImmutableReference,
  /// Immutable adapter-validated selector, mapping and policy configuration.
  pub configuration: ImmutableReference,
  /// Exact ticket container, independent of the target code Repository.
  pub resource: ImmutableReference,
}

/// Exact Project-owned Repository definition shared by Factory resource bindings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRepositoryRef {
  /// Owning OctaCity Project, not a provider project identifier.
  pub project_id: ProjectId,
  /// Code Repository identity, regardless of the ticket provider.
  pub id: RepositoryId,
  /// Immutable Repository definition version.
  pub version: RepositoryVersion,
  /// Content identity of the immutable Repository definition.
  pub definition_digest: FactoryDigest,
}

/// Backward-compatible name for a Work Source's exact code Repository reference.
pub type WorkSourceRepositoryRef = FactoryRepositoryRef;

/// Independently installed ticket capability roles; provider names are open identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkSourceCapabilityRole {
  /// Discover and normalize candidates for ordinary Work admission.
  Source,
  /// Project accepted configured effects without lifecycle authority.
  Reporter,
}

/// Trusted, Project-scoped declaration resolved from a verified installation.
///
/// This is not deserializable provider input or an execution/credential grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkSourceCapability {
  /// Project permitted to select this installed configuration.
  pub project_id: ProjectId,
  /// Independently verified capability role.
  pub role: WorkSourceCapabilityRole,
  /// Exact installed resource and adapter-validated configuration.
  pub binding: WorkSourceBinding,
}

/// Complete provider-neutral definition submitted for immutable input publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceInputDraft {
  /// Optional immutable status effects, accepted only with explicit installed editing support.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reporting: Option<crate::WorkSourceStatusMapping>,
  /// Optional immutable narrowing over normalized Source data, supported only by advertised profiles.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub predicate: Option<crate::WorkSourcePredicate>,
  /// Exact managed connection and reviewed Project access; absent for legacy installed inputs.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub connection: Option<WorkSourceConnectionRef>,
  /// Exact Factory Configuration; its published closure selects the root Flow.
  pub target: FactoryConfigurationRef,
  /// Exact code Repository selected independently of the ticket container.
  pub repository: WorkSourceRepositoryRef,
  /// Installed Source and immutable provider-owned input settings.
  pub source: WorkSourceBinding,
  /// Optional separately installed Reporter and immutable effect mapping.
  pub reporter: Option<WorkSourceBinding>,
  /// Explicit operator opt-in to future discovery/admission workers.
  #[serde(default)]
  pub enabled: bool,
}

/// Managed authority is separate from the stable input and native task identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceConnectionRef {
  /// Reusable connection selected without credentials.
  pub id: octacity_server_domain::IntegrationId,
  /// Exact metadata revision, never implicitly retargeted.
  pub version: octacity_server_domain::IntegrationVersion,
  /// Reviewed Project grant revision, rechecked at publication.
  pub access_version: octacity_server_domain::IntegrationVersion,
  /// Installed connection profile, separate from this input's selector/mapping settings.
  pub source_configuration: ImmutableReference,
  /// Separately selected writer profile; absent when no Reporter is selected.
  pub reporter_configuration: Option<ImmutableReference>,
}

/// Selected immutable resources supplied by authorized application resolution.
///
/// Publication is pure validation and cannot enumerate providers or resolve secrets.
pub struct WorkSourceInputContext<'a> {
  /// Selected published Factory Configuration.
  pub target: &'a FactoryConfiguration,
  /// Selected published code Repository reference.
  pub repository: &'a WorkSourceRepositoryRef,
  /// Installed Source, or absence when that capability is unavailable.
  pub source: Option<&'a WorkSourceCapability>,
  /// Separately installed Reporter, if one was selected.
  pub reporter: Option<&'a WorkSourceCapability>,
}

/// Immutable Project-owned input version; replacement retains the stable identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "RetainedWorkSourceInput")]
pub struct WorkSourceInput {
  id: WorkSourceInputId,
  version: WorkSourceInputVersion,
  project_id: ProjectId,
  root_flow: FlowDefinitionRef,
  definition: WorkSourceInputDraft,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedWorkSourceInput {
  id: WorkSourceInputId,
  version: WorkSourceInputVersion,
  project_id: ProjectId,
  root_flow: FlowDefinitionRef,
  definition: WorkSourceInputDraft,
}

impl TryFrom<RetainedWorkSourceInput> for WorkSourceInput {
  type Error = FactoryError;

  fn try_from(value: RetainedWorkSourceInput) -> Result<Self, Self::Error> {
    let definition = &value.definition;
    if let Some(predicate) = &definition.predicate {
      predicate.validate()?;
    }
    validate_reporting(definition)?;
    if definition.target.project_id() != value.project_id
      || definition.repository.project_id != value.project_id
      || definition.reporter.as_ref().is_some_and(|reporter| {
        reporter.installation != definition.source.installation || reporter.resource != definition.source.resource
      })
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "retained source input",
      });
    }
    // Restoring a document is not capability authorization. Publication and admission
    // still resolve the exact target, repository and trusted installed capability.
    Ok(Self {
      id: value.id,
      version: value.version,
      project_id: value.project_id,
      root_flow: value.root_flow,
      definition: value.definition,
    })
  }
}

impl WorkSourceInput {
  /// Publishes the initial immutable definition over selected trusted resources.
  pub fn publish(
    id: WorkSourceInputId,
    project_id: ProjectId,
    definition: WorkSourceInputDraft,
    context: WorkSourceInputContext<'_>,
  ) -> Result<Self, FactoryError> {
    if let Some(predicate) = &definition.predicate {
      predicate.validate()?;
    }
    validate_reporting(&definition)?;
    if definition.target != *context.target.reference() || definition.target.project_id() != project_id {
      return Err(FactoryError::InvalidConfiguration {
        field: "source input target",
      });
    }
    if definition.repository != *context.repository || definition.repository.project_id != project_id {
      return Err(FactoryError::InvalidConfiguration {
        field: "source input repository",
      });
    }
    if definition.enabled && !context.target.is_enabled() {
      return Err(FactoryError::InvalidConfiguration {
        field: "source input enabled target",
      });
    }
    if !context.source.is_some_and(|source| {
      source.project_id == project_id
        && source.role == WorkSourceCapabilityRole::Source
        && source.binding == definition.source
    }) {
      return Err(FactoryError::InvalidConfiguration {
        field: "source input capability",
      });
    }
    if let Some(selected) = &definition.reporter
      && (selected.installation != definition.source.installation
        || selected.resource != definition.source.resource
        || !context.reporter.is_some_and(|reporter| {
          reporter.project_id == project_id
            && reporter.role == WorkSourceCapabilityRole::Reporter
            && reporter.binding == *selected
        }))
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "source input reporter",
      });
    }
    Ok(Self {
      id,
      version: WorkSourceInputVersion::INITIAL,
      project_id,
      root_flow: context.target.flow().closure.root(),
      definition,
    })
  }

  /// Publishes only the next version, retaining identity and Project ownership.
  /// Current-version fencing and atomic publication belong to the authoritative store.
  pub fn replace(
    &self,
    version: WorkSourceInputVersion,
    definition: WorkSourceInputDraft,
    context: WorkSourceInputContext<'_>,
  ) -> Result<Self, FactoryError> {
    if self.version.get().checked_add(1) != Some(version.get()) {
      return Err(FactoryError::InvalidConfiguration {
        field: "source input replacement version",
      });
    }
    let mut replacement = Self::publish(self.id, self.project_id, definition, context)?;
    replacement.version = version;
    Ok(replacement)
  }

  /// Returns the stable identity shared by all replacements.
  #[must_use]
  pub const fn id(&self) -> WorkSourceInputId {
    self.id
  }

  /// Returns this immutable version.
  #[must_use]
  pub const fn version(&self) -> WorkSourceInputVersion {
    self.version
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the exact root from the selected target's published closure.
  #[must_use]
  pub const fn root_flow(&self) -> FlowDefinitionRef {
    self.root_flow
  }

  /// Borrows the frozen source/target settings without permitting mutation.
  #[must_use]
  pub const fn definition(&self) -> &WorkSourceInputDraft {
    &self.definition
  }
}

fn validate_reporting(definition: &WorkSourceInputDraft) -> Result<(), FactoryError> {
  if let Some(mapping) = &definition.reporting {
    mapping.validate()?;
    if definition.reporter.is_none() {
      return Err(FactoryError::InvalidConfiguration {
        field: "source reporting mapping",
      });
    }
  }
  Ok(())
}
