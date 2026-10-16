//! Explicit, provider-neutral exclusive selection over pinned input definitions.

use octacity_server_domain::ProjectId;
use serde::Serialize;
use std::collections::BTreeSet;

use crate::{FactoryError, WorkSourceGroupId, WorkSourceGroupVersion, WorkSourceInput, WorkSourceInputId};

/// Local maximum number of frozen inputs evaluated in one exclusive selection pass.
pub const MAX_WORK_SOURCE_GROUP_MEMBERS: usize = 64;

/// Immutable Project-owned exclusive group with frozen member input definitions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkSourceGroup {
  id: WorkSourceGroupId,
  version: WorkSourceGroupVersion,
  project_id: ProjectId,
  members: Vec<WorkSourceInput>,
}

/// Selection disposition only; none of these outcomes admits Work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkSourceSelection {
  /// No member selected this observation.
  Filtered,
  /// Exactly one frozen input is eligible for ordinary admission.
  Selected(Box<WorkSourceInput>),
  /// Multiple matching stable input identities; no target may be submitted.
  Ambiguous(Vec<WorkSourceInputId>),
}

/// Pending pass, which must be checked against the authoritative current group.
pub struct WorkSourceSelectionPass {
  group: WorkSourceGroup,
  matched: Vec<WorkSourceInput>,
}

impl WorkSourceGroup {
  /// Publishes a group from already validated immutable input definitions.
  pub fn publish(
    id: WorkSourceGroupId,
    project_id: ProjectId,
    members: Vec<WorkSourceInput>,
  ) -> Result<Self, FactoryError> {
    if members.is_empty() || members.len() > MAX_WORK_SOURCE_GROUP_MEMBERS {
      return Err(FactoryError::InvalidConfiguration {
        field: "source group members",
      });
    }
    let mut seen = BTreeSet::new();
    let source = &members[0].definition().source;
    for member in &members {
      if member.project_id() != project_id {
        return Err(FactoryError::InvalidConfiguration {
          field: "source group member project",
        });
      }
      if !seen.insert(member.id()) {
        return Err(FactoryError::InvalidConfiguration {
          field: "source group duplicate member",
        });
      }
      let selected = &member.definition().source;
      if selected.installation != source.installation
        || selected.capability != source.capability
        || selected.resource != source.resource
      {
        return Err(FactoryError::InvalidConfiguration {
          field: "source group observation scope",
        });
      }
    }
    Ok(Self {
      id,
      version: WorkSourceGroupVersion::INITIAL,
      project_id,
      members,
    })
  }

  /// Publishes only the next immutable revision, preserving the group identity.
  /// Atomic publication and current-version fencing belong to the store.
  pub fn replace(&self, version: WorkSourceGroupVersion, members: Vec<WorkSourceInput>) -> Result<Self, FactoryError> {
    if self.version.get().checked_add(1) != Some(version.get()) {
      return Err(FactoryError::InvalidConfiguration {
        field: "source group replacement version",
      });
    }
    let mut replacement = Self::publish(self.id, self.project_id, members)?;
    replacement.version = version;
    Ok(replacement)
  }

  /// Returns the stable group identity.
  #[must_use]
  pub const fn id(&self) -> WorkSourceGroupId {
    self.id
  }

  /// Returns this immutable group revision.
  #[must_use]
  pub const fn version(&self) -> WorkSourceGroupVersion {
    self.version
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Borrows the exact frozen member input definitions.
  #[must_use]
  pub fn members(&self) -> &[WorkSourceInput] {
    &self.members
  }

  /// Evaluates all pinned members against one borrowed provider-owned observation.
  /// The trusted adapter resolves each member's exact selector configuration.
  pub fn select<C>(
    &self,
    candidate: &C,
    mut matches: impl FnMut(&WorkSourceInput, &C) -> bool,
  ) -> WorkSourceSelectionPass {
    let matched = self
      .members
      .iter()
      .filter(|member| member.definition().enabled && matches(member, candidate))
      .cloned()
      .collect();
    WorkSourceSelectionPass {
      group: self.clone(),
      matched,
    }
  }
}

impl WorkSourceSelectionPass {
  /// Returns a disposition only while the pinned group is still current.
  /// The caller must resolve `current` under its store transaction and scan fence.
  pub fn finish(self, current: &WorkSourceGroup) -> Result<WorkSourceSelection, FactoryError> {
    if self.group != *current {
      return Err(FactoryError::InvalidConfiguration {
        field: "source group current version",
      });
    }
    if self.matched.len() > 1 {
      return Ok(WorkSourceSelection::Ambiguous(
        self.matched.iter().map(WorkSourceInput::id).collect(),
      ));
    }
    Ok(match self.matched.into_iter().next() {
      Some(member) => WorkSourceSelection::Selected(Box::new(member)),
      None => WorkSourceSelection::Filtered,
    })
  }
}
