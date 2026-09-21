use std::{collections::BTreeSet, str::FromStr};

use octacity_server_application::{
  ArtifactPolicy, CacheNamespace, CachePolicy, ConcurrencyPolicy, IdentityProfileName, PolicyCategory, PolicyDirective,
  PolicyResolutionError, ProjectPolicyDefinition, ProjectPolicyLayer, RetentionPolicy, RuntimeClass, SecretProfileName,
  resolve_project_policy,
};
use octacity_server_domain::{PoolId, ProjectId, ProjectPolicyVersion, RepositoryId};

fn project(suffix: u8) -> ProjectId {
  ProjectId::from_str(&format!("00000000-0000-0000-0000-{suffix:012}")).unwrap()
}

fn pool(suffix: u8) -> PoolId {
  PoolId::from_str(&format!("10000000-0000-0000-0000-{suffix:012}")).unwrap()
}

fn repository(suffix: u8) -> RepositoryId {
  RepositoryId::from_str(&format!("20000000-0000-0000-0000-{suffix:012}")).unwrap()
}

fn secret(value: &str) -> SecretProfileName {
  SecretProfileName::new(value).unwrap()
}

fn identity(value: &str) -> IdentityProfileName {
  IdentityProfileName::new(value).unwrap()
}

fn namespace(value: &str) -> CacheNamespace {
  CacheNamespace::new(value).unwrap()
}

fn root_policy() -> ProjectPolicyLayer {
  ProjectPolicyLayer {
    project_id: project(1),
    parent_id: None,
    version: ProjectPolicyVersion::INITIAL,
    definition: ProjectPolicyDefinition {
      pools: PolicyDirective::Replace(BTreeSet::from([pool(1), pool(2)])),
      repositories: PolicyDirective::Replace(BTreeSet::from([repository(1), repository(2)])),
      secret_profiles: PolicyDirective::Replace(BTreeSet::from([secret("ci"), secret("release")])),
      identity_profiles: PolicyDirective::Replace(BTreeSet::from([identity("reader"), identity("publisher")])),
      runtimes: PolicyDirective::Replace(BTreeSet::from([
        RuntimeClass::Native,
        RuntimeClass::OciProcess,
        RuntimeClass::OciHypervisor,
      ])),
      cache: PolicyDirective::Replace(CachePolicy {
        namespaces: BTreeSet::from([namespace("project/main"), namespace("project/release")]),
        read: true,
        write: true,
        max_bytes: 1_000,
      }),
      artifacts: PolicyDirective::Replace(ArtifactPolicy {
        artifact_count: 10,
        artifact_bytes: 1_000,
        report_count: 8,
        report_bytes: 800,
        single_output_bytes: 500,
      }),
      concurrency: PolicyDirective::Replace(ConcurrencyPolicy {
        active_builds: 10,
        active_jobs: 20,
      }),
      retention: PolicyDirective::Replace(RetentionPolicy {
        build_seconds: 1_000,
        log_seconds: 900,
        artifact_seconds: 800,
        cache_seconds: 700,
      }),
    },
  }
}

fn inheriting_child(id: ProjectId, parent_id: ProjectId) -> ProjectPolicyLayer {
  ProjectPolicyLayer {
    project_id: id,
    parent_id: Some(parent_id),
    version: ProjectPolicyVersion::INITIAL,
    definition: ProjectPolicyDefinition {
      pools: PolicyDirective::Inherit,
      repositories: PolicyDirective::Inherit,
      secret_profiles: PolicyDirective::Inherit,
      identity_profiles: PolicyDirective::Inherit,
      runtimes: PolicyDirective::Inherit,
      cache: PolicyDirective::Inherit,
      artifacts: PolicyDirective::Inherit,
      concurrency: PolicyDirective::Inherit,
      retention: PolicyDirective::Inherit,
    },
  }
}

#[test]
fn policy_layer_keeps_the_flat_persisted_document_shape() {
  let mut value = serde_json::to_value(root_policy()).unwrap();

  assert!(value.get("definition").is_none());
  assert!(value.get("pools").is_some());
  assert_eq!(
    serde_json::from_value::<ProjectPolicyLayer>(value.clone()).unwrap(),
    root_policy()
  );

  value.as_object_mut().unwrap().insert("unknown".into(), true.into());
  assert!(serde_json::from_value::<ProjectPolicyLayer>(value).is_err());
}

#[test]
fn inherit_preserves_every_parent_category_and_source_version() {
  let root = root_policy();
  let mut child = inheriting_child(project(2), root.project_id);
  child.version = ProjectPolicyVersion::new(7).unwrap();

  let resolved = resolve_project_policy(&[root.clone(), child]).unwrap();

  let root_only = resolve_project_policy(&[root]).unwrap();
  assert_eq!(resolved.policy, root_only.policy);
  assert_eq!(resolved.sources.len(), 2);
  assert_eq!(resolved.sources[1].project_id, project(2));
  assert_eq!(resolved.sources[1].version.get(), 7);
}

#[test]
fn narrow_intersects_grants_and_lowers_every_ceiling() {
  let root = root_policy();
  let mut child = inheriting_child(project(2), root.project_id);
  child.definition.pools = PolicyDirective::Narrow(BTreeSet::from([pool(2), pool(3)]));
  child.definition.repositories = PolicyDirective::Narrow(BTreeSet::from([repository(2), repository(3)]));
  child.definition.secret_profiles = PolicyDirective::Narrow(BTreeSet::from([secret("release"), secret("unknown")]));
  child.definition.identity_profiles =
    PolicyDirective::Narrow(BTreeSet::from([identity("reader"), identity("unknown")]));
  child.definition.runtimes = PolicyDirective::Narrow(BTreeSet::from([RuntimeClass::OciProcess]));
  child.definition.cache = PolicyDirective::Narrow(CachePolicy {
    namespaces: BTreeSet::from([namespace("project/release"), namespace("foreign")]),
    read: false,
    write: true,
    max_bytes: 600,
  });
  child.definition.artifacts = PolicyDirective::Narrow(ArtifactPolicy {
    artifact_count: 7,
    artifact_bytes: 900,
    report_count: 6,
    report_bytes: 700,
    single_output_bytes: 400,
  });
  child.definition.concurrency = PolicyDirective::Narrow(ConcurrencyPolicy {
    active_builds: 8,
    active_jobs: 12,
  });
  child.definition.retention = PolicyDirective::Narrow(RetentionPolicy {
    build_seconds: 900,
    log_seconds: 800,
    artifact_seconds: 700,
    cache_seconds: 600,
  });

  let policy = resolve_project_policy(&[root, child]).unwrap().policy;

  assert_eq!(policy.pools, BTreeSet::from([pool(2)]));
  assert_eq!(policy.repositories, BTreeSet::from([repository(2)]));
  assert_eq!(policy.secret_profiles, BTreeSet::from([secret("release")]));
  assert_eq!(policy.identity_profiles, BTreeSet::from([identity("reader")]));
  assert_eq!(policy.runtimes, BTreeSet::from([RuntimeClass::OciProcess]));
  assert_eq!(policy.cache.namespaces, BTreeSet::from([namespace("project/release")]));
  assert!(!policy.cache.read);
  assert!(policy.cache.write);
  assert_eq!(policy.cache.max_bytes, 600);
  assert_eq!(policy.artifacts.artifact_count, 7);
  assert_eq!(policy.artifacts.single_output_bytes, 400);
  assert_eq!(policy.concurrency.active_builds, 8);
  assert_eq!(policy.concurrency.active_jobs, 12);
  assert_eq!(policy.retention.build_seconds, 900);
  assert_eq!(policy.retention.cache_seconds, 600);
}

#[test]
fn narrower_replacements_are_exact_instead_of_intersections() {
  let root = root_policy();
  let mut child = inheriting_child(project(2), root.project_id);
  child.definition.pools = PolicyDirective::Replace(BTreeSet::from([pool(1)]));
  child.definition.repositories = PolicyDirective::Replace(BTreeSet::new());
  child.definition.secret_profiles = PolicyDirective::Replace(BTreeSet::from([secret("ci")]));
  child.definition.identity_profiles = PolicyDirective::Replace(BTreeSet::new());
  child.definition.runtimes = PolicyDirective::Replace(BTreeSet::from([RuntimeClass::Native]));
  child.definition.cache = PolicyDirective::Replace(CachePolicy {
    namespaces: BTreeSet::from([namespace("project/main")]),
    read: true,
    write: false,
    max_bytes: 500,
  });
  child.definition.artifacts = PolicyDirective::Replace(ArtifactPolicy {
    artifact_count: 1,
    artifact_bytes: 100,
    report_count: 0,
    report_bytes: 0,
    single_output_bytes: 100,
  });
  child.definition.concurrency = PolicyDirective::Replace(ConcurrencyPolicy {
    active_builds: 1,
    active_jobs: 2,
  });
  child.definition.retention = PolicyDirective::Replace(RetentionPolicy {
    build_seconds: 100,
    log_seconds: 90,
    artifact_seconds: 80,
    cache_seconds: 70,
  });

  let policy = resolve_project_policy(&[root, child]).unwrap().policy;
  assert_eq!(policy.pools, BTreeSet::from([pool(1)]));
  assert!(policy.repositories.is_empty());
  assert_eq!(policy.cache.max_bytes, 500);
  assert_eq!(policy.artifacts.report_count, 0);
  assert_eq!(policy.concurrency.active_jobs, 2);
  assert_eq!(policy.retention.log_seconds, 90);
}

#[test]
fn a_descendant_cannot_restore_a_grant_removed_by_an_ancestor() {
  let root = root_policy();
  let mut child = inheriting_child(project(2), root.project_id);
  child.definition.pools = PolicyDirective::Narrow(BTreeSet::from([pool(1)]));

  let mut grandchild = inheriting_child(project(3), child.project_id);
  grandchild.definition.pools = PolicyDirective::Replace(BTreeSet::from([pool(1), pool(2)]));
  assert_eq!(
    resolve_project_policy(&[root.clone(), child.clone(), grandchild]).unwrap_err(),
    PolicyResolutionError::BroadenedGrant {
      project_id: project(3),
      category: PolicyCategory::Pools,
    }
  );

  let mut intersecting_grandchild = inheriting_child(project(3), child.project_id);
  intersecting_grandchild.definition.pools = PolicyDirective::Narrow(BTreeSet::from([pool(1), pool(2)]));
  let resolved = resolve_project_policy(&[root, child, intersecting_grandchild]).unwrap();
  assert_eq!(resolved.policy.pools, BTreeSet::from([pool(1)]));
}

#[test]
fn every_protected_category_rejects_a_broader_replacement() {
  let root = root_policy();
  let mut runtime_root = root.clone();
  runtime_root.definition.runtimes = PolicyDirective::Replace(BTreeSet::from([RuntimeClass::Native]));
  let mut runtime_child = inheriting_child(project(2), runtime_root.project_id);
  runtime_child.definition.runtimes =
    PolicyDirective::Replace(BTreeSet::from([RuntimeClass::Native, RuntimeClass::OciProcess]));
  assert_eq!(
    resolve_project_policy(&[runtime_root, runtime_child]).unwrap_err(),
    PolicyResolutionError::BroadenedGrant {
      project_id: project(2),
      category: PolicyCategory::Runtime,
    }
  );

  let cases = [
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.pools = PolicyDirective::Replace(BTreeSet::from([pool(1), pool(2), pool(3)]));
      (PolicyCategory::Pools, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.repositories =
        PolicyDirective::Replace(BTreeSet::from([repository(1), repository(2), repository(3)]));
      (PolicyCategory::Repositories, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.secret_profiles =
        PolicyDirective::Replace(BTreeSet::from([secret("ci"), secret("release"), secret("extra")]));
      (PolicyCategory::SecretProfiles, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.identity_profiles = PolicyDirective::Replace(BTreeSet::from([
        identity("reader"),
        identity("publisher"),
        identity("admin"),
      ]));
      (PolicyCategory::IdentityProfiles, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.cache = PolicyDirective::Replace(CachePolicy {
        namespaces: BTreeSet::from([
          namespace("project/main"),
          namespace("project/release"),
          namespace("foreign"),
        ]),
        read: true,
        write: true,
        max_bytes: 1_000,
      });
      (PolicyCategory::Cache, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.artifacts = PolicyDirective::Replace(ArtifactPolicy {
        artifact_count: 11,
        artifact_bytes: 1_000,
        report_count: 8,
        report_bytes: 800,
        single_output_bytes: 500,
      });
      (PolicyCategory::Artifacts, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.concurrency = PolicyDirective::Replace(ConcurrencyPolicy {
        active_builds: 11,
        active_jobs: 20,
      });
      (PolicyCategory::Concurrency, child)
    },
    {
      let mut child = inheriting_child(project(2), root.project_id);
      child.definition.retention = PolicyDirective::Replace(RetentionPolicy {
        build_seconds: 1_001,
        log_seconds: 900,
        artifact_seconds: 800,
        cache_seconds: 700,
      });
      (PolicyCategory::Retention, child)
    },
  ];

  for (category, child) in cases {
    assert_eq!(
      resolve_project_policy(&[root.clone(), child]).unwrap_err(),
      PolicyResolutionError::BroadenedGrant {
        project_id: project(2),
        category,
      }
    );
  }
}

#[test]
fn root_requires_an_explicit_replacement_for_every_category() {
  let root = root_policy();
  let cases = [
    {
      let mut value = root.clone();
      value.definition.pools = PolicyDirective::Inherit;
      (PolicyCategory::Pools, value)
    },
    {
      let mut value = root.clone();
      value.definition.repositories = PolicyDirective::Inherit;
      (PolicyCategory::Repositories, value)
    },
    {
      let mut value = root.clone();
      value.definition.secret_profiles = PolicyDirective::Inherit;
      (PolicyCategory::SecretProfiles, value)
    },
    {
      let mut value = root.clone();
      value.definition.identity_profiles = PolicyDirective::Inherit;
      (PolicyCategory::IdentityProfiles, value)
    },
    {
      let mut value = root.clone();
      value.definition.runtimes = PolicyDirective::Inherit;
      (PolicyCategory::Runtime, value)
    },
    {
      let mut value = root.clone();
      value.definition.cache = PolicyDirective::Inherit;
      (PolicyCategory::Cache, value)
    },
    {
      let mut value = root.clone();
      value.definition.artifacts = PolicyDirective::Inherit;
      (PolicyCategory::Artifacts, value)
    },
    {
      let mut value = root.clone();
      value.definition.concurrency = PolicyDirective::Inherit;
      (PolicyCategory::Concurrency, value)
    },
    {
      let mut value = root.clone();
      value.definition.retention = PolicyDirective::Inherit;
      (PolicyCategory::Retention, value)
    },
  ];

  for (category, invalid_root) in cases {
    assert_eq!(
      resolve_project_policy(&[invalid_root]).unwrap_err(),
      PolicyResolutionError::RootMustReplace {
        project_id: project(1),
        category,
      }
    );
  }
}

#[test]
fn artifact_shape_is_validated_once_for_policy_and_configuration_consumers() {
  let mut root = root_policy();
  root.definition.artifacts = PolicyDirective::Replace(ArtifactPolicy {
    artifact_count: 1,
    artifact_bytes: 0,
    report_count: 0,
    report_bytes: 0,
    single_output_bytes: 1,
  });

  assert_eq!(
    resolve_project_policy(&[root]).unwrap_err(),
    PolicyResolutionError::InvalidValue {
      project_id: project(1),
      category: PolicyCategory::Artifacts,
    }
  );
}

#[test]
fn rejects_empty_disconnected_and_repeated_lineages() {
  assert_eq!(
    resolve_project_policy(&[]).unwrap_err(),
    PolicyResolutionError::EmptyLineage
  );

  let mut non_root = root_policy();
  non_root.parent_id = Some(project(9));
  assert_eq!(
    resolve_project_policy(&[non_root]).unwrap_err(),
    PolicyResolutionError::RootHasParent { project_id: project(1) }
  );

  let root = root_policy();
  let disconnected = inheriting_child(project(2), project(9));
  assert_eq!(
    resolve_project_policy(&[root.clone(), disconnected]).unwrap_err(),
    PolicyResolutionError::DisconnectedLineage {
      project_id: project(2),
      expected_parent: project(1),
    }
  );

  let repeated = inheriting_child(root.project_id, root.project_id);
  assert_eq!(
    resolve_project_policy(&[root, repeated]).unwrap_err(),
    PolicyResolutionError::DuplicateProject { project_id: project(1) }
  );
}

#[test]
fn logical_policy_references_are_bounded_before_resolution() {
  assert!(SecretProfileName::new("").is_err());
  assert!(IdentityProfileName::new(" leading").is_err());
  assert!(CacheNamespace::new("line\nbreak").is_err());
  assert!(CacheNamespace::new("x".repeat(octacity_server_application::MAX_POLICY_REFERENCE_BYTES + 1)).is_err());
}
