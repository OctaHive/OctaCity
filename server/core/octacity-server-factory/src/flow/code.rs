//! Explicit code selection is independent of Work correlation and Context documents.
use crate::{FactoryError, FactoryKey, FactoryRepositoryRef, MountMode};
use octacity_server_domain::RepositorySourceSelection;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum source declarations in one immutable Flow Definition.
pub const MAX_FLOW_CODE_SOURCES: usize = 64;

/// Available exact Project repository definitions. Availability alone grants no access.
pub type FlowCodeSources = BTreeMap<FactoryKey, FlowCodeSource>;

/// Exact available source with a reviewed selection rule and maximum attachment scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowCodeSource {
  /// Existing Project Repository owner; no new registry or credential value.
  pub repository: FactoryRepositoryRef,
  /// Allowed source expression, resolved only when a step actually uses it.
  pub selection: RepositorySourceSelection,
  /// Maximum access a step may request; availability itself grants nothing.
  pub access: MountMode,
  /// Maximum code visibility a step may select.
  pub scope: FlowCodeScope,
}

/// One literal repository path or read-only line view; no host path or glob expansion.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowCodeSelection {
  /// A complete file or directory subtree under the selected source.
  Path {
    /// Canonical existing repository-relative path grammar.
    path: crate::FactoryRepositoryPath,
  },
  /// A bounded read-only view of exact source lines.
  Range {
    /// Exact file path; a range never covers descendant paths.
    path: crate::FactoryRepositoryPath,
    /// Existing one-based inclusive line invariant, without synthetic VCS identities.
    lines: crate::FactoryRepositoryLineRange,
  },
}
impl FlowCodeSelection {
  fn is_within(&self, ceiling: &Self) -> bool {
    match (self, ceiling) {
      (Self::Path { path } | Self::Range { path, .. }, Self::Path { path: parent }) => path.is_within(parent),
      (
        Self::Range { path, lines },
        Self::Range {
          path: parent,
          lines: allowed,
        },
      ) => path == parent && lines.is_within(allowed),
      (Self::Path { .. }, Self::Range { .. }) => false,
    }
  }
}

/// Reviewed visibility within a selected repository, independent of host paths.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowCodeScope {
  /// The selected repository resource, subject to independent execution policy.
  Repository,
  /// Explicit bounded union of literal paths and read-only line views.
  Selected {
    /// Canonical selections; an empty set cannot imply the whole source.
    entries: BTreeSet<FlowCodeSelection>,
  },
}
#[derive(Deserialize)]
#[serde(
  remote = "FlowCodeScope",
  tag = "kind",
  rename_all = "snake_case",
  deny_unknown_fields
)]
enum FlowCodeScopeWire {
  Repository,
  Selected { entries: BTreeSet<FlowCodeSelection> },
}
impl<'de> Deserialize<'de> for FlowCodeScope {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    let scope = FlowCodeScopeWire::deserialize(deserializer)?;
    scope.validate().map_err(D::Error::custom)?;
    Ok(scope)
  }
}
impl FlowCodeScope {
  fn validate(&self) -> Result<(), FactoryError> {
    if let Self::Selected { entries } = self
      && (entries.is_empty() || entries.len() > crate::MAX_PERMISSION_ENTRIES_PER_CATEGORY)
    {
      return Err(FactoryError::InvalidConfiguration {
        field: "Flow code scope",
      });
    }
    Ok(())
  }
  /// Whether every requested item is contained in an explicitly declared source ceiling.
  #[must_use]
  pub fn is_within(&self, ceiling: &Self) -> bool {
    match (self, ceiling) {
      (_, Self::Repository) => true,
      (Self::Repository, Self::Selected { .. }) => false,
      (Self::Selected { entries }, Self::Selected { entries: allowed }) => entries
        .iter()
        .all(|entry| allowed.iter().any(|parent| entry.is_within(parent))),
    }
  }
  fn has_ranges(&self) -> bool {
    matches!(self, Self::Selected { entries } if entries.iter().any(|entry| matches!(entry, FlowCodeSelection::Range { .. })))
  }
}

/// Exact revision relation selected for one step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowCodeRevision {
  /// Use the immutable base frozen by admission for this declared source.
  AdmittedBase,
  /// Explicit immutable revision, additionally subject to the Repository owner's exact-selection policy.
  ExactRevision {
    /// Literal immutable revision; never a branch expression resampled on retry.
    revision: octacity_server_domain::ImmutableRevision,
  },
  /// Candidate independently retained with an accepted producing result, never model prose.
  AcceptedCandidate {
    /// Explicit producing step in the same exact Flow definition.
    node: FactoryKey,
    /// Finite accepted outcome whose owner facts must contain the candidate.
    outcome: FactoryKey,
  },
}

/// Reviewed code choice for one step; absence retains historical Flow encoding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowCodeBinding {
  /// No code resource is attached to this step.
  None,
  /// Attach one explicitly declared primary source under its access ceiling.
  Source {
    /// Key in the containing definition's reviewed availability map.
    source: FactoryKey,
    /// Requested access; it cannot widen the declaration or execution policy.
    access: MountMode,
    /// Requested visibility inside this source.
    scope: FlowCodeScope,
    /// Exact retained revision relation; mutable refs are never resampled per retry.
    revision: FlowCodeRevision,
  },
}

/// Stable draft location rejected by the shared code-binding validator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowCodeViolation {
  /// Operator-owned step key, independent of canonical sort order.
  pub node: FactoryKey,
  /// Field relative to that step, without hidden dependency data.
  pub field: &'static str,
}

fn invalid(node: &super::FlowNodeDefinition, field: &'static str) -> FactoryError {
  FactoryError::InvalidCodeBinding {
    context: Box::new(FlowCodeViolation {
      node: node.key().clone(),
      field,
    }),
  }
}

pub(super) fn validate(
  sources: Option<&FlowCodeSources>,
  nodes: &[super::FlowNodeDefinition],
) -> Result<(), FactoryError> {
  if sources.is_some_and(|sources| sources.len() > MAX_FLOW_CODE_SOURCES) {
    return Err(FactoryError::InvalidConfiguration {
      field: "Flow code sources",
    });
  }
  for source in sources.into_iter().flat_map(|sources| sources.values()) {
    source.scope.validate()?;
  }
  for node in nodes {
    match node.code() {
      None if sources.is_none() => (), // Historical Work-backed node semantics.
      None => return Err(invalid(node, "/code")),
      Some(FlowCodeBinding::None) => (),
      Some(FlowCodeBinding::Source {
        source,
        access,
        scope,
        revision,
      }) => {
        let declared = sources
          .and_then(|sources| sources.get(source))
          .ok_or_else(|| invalid(node, "/code/source"))?;
        if let (FlowCodeRevision::ExactRevision { revision }, RepositorySourceSelection::ExactRevision(allowed)) =
          (revision, &declared.selection)
          && revision != allowed
        {
          return Err(invalid(node, "/code/revision"));
        }
        if let FlowCodeRevision::AcceptedCandidate {
          node: producer,
          outcome,
        } = revision
        {
          let selected = nodes
            .iter()
            .find(|candidate| candidate.key() == producer)
            .filter(|candidate| candidate.key() != node.key() && candidate.outcome(outcome).is_some())
            .and_then(|candidate| match candidate.code() {
              Some(FlowCodeBinding::Source { source, .. }) => sources.and_then(|sources| sources.get(source)),
              _ => None,
            });
          if selected.is_none_or(|selected| selected.repository != declared.repository) {
            return Err(invalid(node, "/code/revision"));
          }
        }
        scope.validate()?;
        if *access == MountMode::ReadWrite && (declared.access != MountMode::ReadWrite || scope.has_ranges()) {
          return Err(invalid(node, "/code/access"));
        }
        if !scope.is_within(&declared.scope) {
          return Err(invalid(node, "/code/scope"));
        }
      }
    }
  }
  Ok(())
}

/// One exact source revision frozen before dependent execution; availability is not a mount.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowCodePin {
  /// Exact containing Flow version, so aliases in nested definitions cannot collide.
  pub definition: super::FlowDefinitionRef,
  /// Reviewed declaration key within that definition.
  pub source: FactoryKey,
  /// Exact repository and revision, independent of Work correlation.
  pub subject: crate::ExactSubject,
}

/// Requires exactly the used source set; callers cannot inject extra pins or swap repositories.
pub(super) fn validate_pins(
  closure: &super::PinnedFlowDefinitionClosure,
  pins: &[FlowCodePin],
  project: Option<octacity_server_domain::ProjectId>,
) -> Result<(), FactoryError> {
  let invalid = || FactoryError::InvalidReference {
    relationship: "admitted Flow code pins",
  };
  if pins
    .windows(2)
    .any(|pair| (pair[0].definition, &pair[0].source) >= (pair[1].definition, &pair[1].source))
  {
    return Err(invalid());
  }
  let mut expected = BTreeSet::new();
  for definition in closure.definitions() {
    for node in definition.nodes() {
      if let Some(FlowCodeBinding::Source { source, revision, .. }) = node.code()
        && matches!(revision, FlowCodeRevision::AdmittedBase)
      {
        expected.insert((definition.reference(), source.clone()));
      }
    }
  }
  if expected.len() != pins.len() {
    return Err(invalid());
  }
  for pin in pins {
    if !expected.remove(&(pin.definition, pin.source.clone())) {
      return Err(invalid());
    }
    let source = closure
      .definition(pin.definition)
      .and_then(|definition| definition.code_sources())
      .and_then(|sources| sources.get(&pin.source))
      .ok_or_else(invalid)?;
    if project.is_some_and(|project| source.repository.project_id != project)
      || pin.subject.project_id() != source.repository.project_id
      || source.repository.id != pin.subject.repository_id()
      || matches!(&source.selection, RepositorySourceSelection::ExactRevision(revision) if revision != pin.subject.base_revision())
    {
      return Err(invalid());
    }
  }
  Ok(())
}
