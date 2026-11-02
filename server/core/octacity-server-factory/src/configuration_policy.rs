//! Explicit Project selections retain their typed pins; legacy aliases keep their wire bytes.
use serde::{Deserialize, Serialize};

use crate::{FactoryKey, ImmutableReference};

/// An unresolved legacy installation alias or an exact Project-owned policy selection.
/// The object form is deliberately unreadable as an alias by older binaries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum FactoryPolicySelection<R> {
  /// Exact Project-owned revision; never an implicit latest/default lookup.
  Project {
    /// Exact owned policy reference.
    project_policy: R,
  },
  /// Original alias representation, preserved for existing clients and intent digests.
  Legacy(FactoryKey),
}

impl<R> From<FactoryKey> for FactoryPolicySelection<R> {
  fn from(alias: FactoryKey) -> Self {
    Self::Legacy(alias)
  }
}

impl<R> FactoryPolicySelection<R> {
  /// Borrows the exact Project selection, if this is the new representation.
  pub const fn project_policy(&self) -> Option<&R> {
    match self {
      Self::Project { project_policy } => Some(project_policy),
      Self::Legacy(_) => None,
    }
  }
}

/// A published exact legacy reference or a separately typed Project policy pin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum FactoryPolicyBinding<R> {
  /// New Project-owned representation; names and defaults carry no authority.
  Project {
    /// Exact owned policy reference.
    project_policy: R,
  },
  /// Unmodified historical installation reference.
  Legacy(ImmutableReference),
}

impl<R> FactoryPolicyBinding<R> {
  /// Original complete reference, used only for explicit compatibility mapping.
  pub const fn legacy(&self) -> Option<&ImmutableReference> {
    match self {
      Self::Legacy(reference) => Some(reference),
      Self::Project { .. } => None,
    }
  }
  /// Exact Project-owned revision, never replaced by a newer default.
  pub const fn project_policy(&self) -> Option<&R> {
    match self {
      Self::Project { project_policy } => Some(project_policy),
      Self::Legacy(_) => None,
    }
  }
}
