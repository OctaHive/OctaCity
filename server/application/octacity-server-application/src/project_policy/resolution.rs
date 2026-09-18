use std::collections::BTreeSet;

use octacity_server_domain::ProjectId;
use thiserror::Error;

use super::value::{
  ArtifactPolicy, CachePolicy, ConcurrencyPolicy, EffectiveProjectPolicy, PolicyDirective, PolicySource, ProjectPolicy,
  ProjectPolicyLayer, RetentionPolicy,
};

/// Stable Project-policy category used in classified resolution failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyCategory {
  /// Agent Pool allowlist.
  Pools,
  /// Repository allowlist.
  Repositories,
  /// Logical secret-profile allowlist.
  SecretProfiles,
  /// Logical workload-identity-profile allowlist.
  IdentityProfiles,
  /// Runtime and isolation allowlist.
  Runtime,
  /// Cache authority and quota.
  Cache,
  /// Artifact and report limits.
  Artifacts,
  /// Active Build and Job limits.
  Concurrency,
  /// Build Result retention limits.
  Retention,
}

impl std::fmt::Display for PolicyCategory {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(match self {
      Self::Pools => "pools",
      Self::Repositories => "repositories",
      Self::SecretProfiles => "secret_profiles",
      Self::IdentityProfiles => "identity_profiles",
      Self::Runtime => "runtime",
      Self::Cache => "cache",
      Self::Artifacts => "artifacts",
      Self::Concurrency => "concurrency",
      Self::Retention => "retention",
    })
  }
}

/// Classified failure from root-to-leaf Project-policy resolution.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PolicyResolutionError {
  /// A Project policy cannot be resolved without a root layer.
  #[error("project policy lineage is empty")]
  EmptyLineage,
  /// The first layer claims to have a parent and therefore is not a root.
  #[error("root project {project_id} unexpectedly names a parent")]
  RootHasParent {
    /// Invalid root Project.
    project_id: ProjectId,
  },
  /// A later layer does not directly descend from the preceding layer.
  #[error("project {project_id} does not directly descend from {expected_parent}")]
  DisconnectedLineage {
    /// Layer whose parent link is invalid.
    project_id: ProjectId,
    /// Project that must be its direct parent.
    expected_parent: ProjectId,
  },
  /// The same Project occurs more than once in the supplied lineage.
  #[error("project {project_id} occurs more than once in the policy lineage")]
  DuplicateProject {
    /// Repeated Project identity.
    project_id: ProjectId,
  },
  /// A root category used `inherit` or `narrow` despite having no parent.
  #[error("root project {project_id} must replace the {category} policy")]
  RootMustReplace {
    /// Root Project with an incomplete policy.
    project_id: ProjectId,
    /// Category that was not explicitly established.
    category: PolicyCategory,
  },
  /// A descendant replacement attempted to grant more than its parent.
  #[error("project {project_id} replacement broadens the protected parent {category} policy")]
  BroadenedGrant {
    /// Descendant Project that attempted the broader grant.
    project_id: ProjectId,
    /// Protected category that would be broadened.
    category: PolicyCategory,
  },
  /// One supplied policy value violates its category's domain invariants.
  #[error("project {project_id} supplies an invalid {category} policy")]
  InvalidValue {
    /// Project whose layer contains the invalid value.
    project_id: ProjectId,
    /// Invalid policy category.
    category: PolicyCategory,
  },
}

trait ProtectedPolicy: Clone {
  fn narrow(parent: &Self, restriction: &Self) -> Self;
  fn no_broader_than(&self, parent: &Self) -> bool;
}

impl<T> ProtectedPolicy for BTreeSet<T>
where
  T: Clone + Ord,
{
  fn narrow(parent: &Self, restriction: &Self) -> Self {
    parent.intersection(restriction).cloned().collect()
  }

  fn no_broader_than(&self, parent: &Self) -> bool {
    self.is_subset(parent)
  }
}

impl ProtectedPolicy for CachePolicy {
  fn narrow(parent: &Self, restriction: &Self) -> Self {
    Self {
      namespaces: BTreeSet::narrow(&parent.namespaces, &restriction.namespaces),
      read: parent.read && restriction.read,
      write: parent.write && restriction.write,
      max_bytes: parent.max_bytes.min(restriction.max_bytes),
    }
  }

  fn no_broader_than(&self, parent: &Self) -> bool {
    self.namespaces.is_subset(&parent.namespaces)
      && (!self.read || parent.read)
      && (!self.write || parent.write)
      && self.max_bytes <= parent.max_bytes
  }
}

macro_rules! protected_ceilings {
  ($type:ty, $($field:ident),+ $(,)?) => {
    impl ProtectedPolicy for $type {
      fn narrow(parent: &Self, restriction: &Self) -> Self {
        Self {
          $($field: parent.$field.min(restriction.$field)),+
        }
      }

      fn no_broader_than(&self, parent: &Self) -> bool {
        $(self.$field <= parent.$field)&&+
      }
    }
  };
}

protected_ceilings!(
  ArtifactPolicy,
  artifact_count,
  artifact_bytes,
  report_count,
  report_bytes,
  single_output_bytes,
);
protected_ceilings!(ConcurrencyPolicy, active_builds, active_jobs);
protected_ceilings!(
  RetentionPolicy,
  build_seconds,
  log_seconds,
  artifact_seconds,
  cache_seconds,
);

fn root_value<T: Clone>(
  project_id: ProjectId,
  category: PolicyCategory,
  directive: &PolicyDirective<T>,
) -> Result<T, PolicyResolutionError> {
  match directive {
    PolicyDirective::Replace(value) => Ok(value.clone()),
    PolicyDirective::Inherit | PolicyDirective::Narrow(_) => {
      Err(PolicyResolutionError::RootMustReplace { project_id, category })
    }
  }
}

fn resolve_value<T: ProtectedPolicy>(
  project_id: ProjectId,
  category: PolicyCategory,
  parent: &T,
  directive: &PolicyDirective<T>,
) -> Result<T, PolicyResolutionError> {
  match directive {
    PolicyDirective::Inherit => Ok(parent.clone()),
    PolicyDirective::Narrow(restriction) => Ok(T::narrow(parent, restriction)),
    PolicyDirective::Replace(replacement) if replacement.no_broader_than(parent) => Ok(replacement.clone()),
    PolicyDirective::Replace(_) => Err(PolicyResolutionError::BroadenedGrant { project_id, category }),
  }
}

/// Resolves immutable Project policy layers supplied in root-to-leaf order.
///
/// A root must explicitly `replace` every category. Descendants may inherit,
/// replace with an equal or narrower value, or narrow by intersection and
/// component-wise minimum. Consequently no descendant can restore a grant or
/// raise a ceiling removed by an ancestor.
pub fn resolve_project_policy(layers: &[ProjectPolicyLayer]) -> Result<EffectiveProjectPolicy, PolicyResolutionError> {
  for layer in layers {
    let artifacts = match &layer.artifacts {
      PolicyDirective::Replace(value) | PolicyDirective::Narrow(value) => Some(value),
      PolicyDirective::Inherit => None,
    };
    if artifacts.is_some_and(|value| value.validate().is_err()) {
      return Err(PolicyResolutionError::InvalidValue {
        project_id: layer.project_id,
        category: PolicyCategory::Artifacts,
      });
    }
  }
  let root = layers.first().ok_or(PolicyResolutionError::EmptyLineage)?;
  if root.parent_id.is_some() {
    return Err(PolicyResolutionError::RootHasParent {
      project_id: root.project_id,
    });
  }

  let mut policy = ProjectPolicy {
    pools: root_value(root.project_id, PolicyCategory::Pools, &root.pools)?,
    repositories: root_value(root.project_id, PolicyCategory::Repositories, &root.repositories)?,
    secret_profiles: root_value(root.project_id, PolicyCategory::SecretProfiles, &root.secret_profiles)?,
    identity_profiles: root_value(
      root.project_id,
      PolicyCategory::IdentityProfiles,
      &root.identity_profiles,
    )?,
    runtimes: root_value(root.project_id, PolicyCategory::Runtime, &root.runtimes)?,
    cache: root_value(root.project_id, PolicyCategory::Cache, &root.cache)?,
    artifacts: root_value(root.project_id, PolicyCategory::Artifacts, &root.artifacts)?,
    concurrency: root_value(root.project_id, PolicyCategory::Concurrency, &root.concurrency)?,
    retention: root_value(root.project_id, PolicyCategory::Retention, &root.retention)?,
  };
  let mut sources = Vec::with_capacity(layers.len());
  let mut seen = BTreeSet::new();
  seen.insert(root.project_id);
  sources.push(PolicySource {
    project_id: root.project_id,
    version: root.version,
  });

  let mut parent_id = root.project_id;
  for layer in &layers[1..] {
    if !seen.insert(layer.project_id) {
      return Err(PolicyResolutionError::DuplicateProject {
        project_id: layer.project_id,
      });
    }
    if layer.parent_id != Some(parent_id) {
      return Err(PolicyResolutionError::DisconnectedLineage {
        project_id: layer.project_id,
        expected_parent: parent_id,
      });
    }

    policy = ProjectPolicy {
      pools: resolve_value(layer.project_id, PolicyCategory::Pools, &policy.pools, &layer.pools)?,
      repositories: resolve_value(
        layer.project_id,
        PolicyCategory::Repositories,
        &policy.repositories,
        &layer.repositories,
      )?,
      secret_profiles: resolve_value(
        layer.project_id,
        PolicyCategory::SecretProfiles,
        &policy.secret_profiles,
        &layer.secret_profiles,
      )?,
      identity_profiles: resolve_value(
        layer.project_id,
        PolicyCategory::IdentityProfiles,
        &policy.identity_profiles,
        &layer.identity_profiles,
      )?,
      runtimes: resolve_value(
        layer.project_id,
        PolicyCategory::Runtime,
        &policy.runtimes,
        &layer.runtimes,
      )?,
      cache: resolve_value(layer.project_id, PolicyCategory::Cache, &policy.cache, &layer.cache)?,
      artifacts: resolve_value(
        layer.project_id,
        PolicyCategory::Artifacts,
        &policy.artifacts,
        &layer.artifacts,
      )?,
      concurrency: resolve_value(
        layer.project_id,
        PolicyCategory::Concurrency,
        &policy.concurrency,
        &layer.concurrency,
      )?,
      retention: resolve_value(
        layer.project_id,
        PolicyCategory::Retention,
        &policy.retention,
        &layer.retention,
      )?,
    };
    sources.push(PolicySource {
      project_id: layer.project_id,
      version: layer.version,
    });
    parent_id = layer.project_id;
  }

  Ok(EffectiveProjectPolicy { sources, policy })
}
