use crate::{
  FactoryAccessProfile, FactoryAccessProfileDefinition, FactoryAccessProfileId, FactoryAccessProfileVersion,
  FactoryAdmissionOrigin, FactoryKey, FactoryLaunchPolicy, FactoryLaunchPolicyDefinition, FactoryLaunchPolicyId,
  FactoryLaunchPolicyVersion, FactoryPath, FactoryPermissionDraft, FactoryPermissionSet, FactoryResourceLimits,
  FactoryText, MountMode, MountPermission, NetworkHost,
};

#[test]
fn project_policy_launch_rules_use_explicit_origin_flags_including_deny_both() {
  for (manual, automatic, expected) in [
    (false, false, [false, false]),
    (true, false, [true, false]),
    (false, true, [false, true]),
    (true, true, [true, true]),
  ] {
    let definition = FactoryLaunchPolicyDefinition {
      name: FactoryText::new("Standard").unwrap(),
      allow_manual: manual,
      allow_automatic_source: automatic,
    };
    assert_eq!(definition.allows(FactoryAdmissionOrigin::Manual), expected[0]);
    assert_eq!(definition.allows(FactoryAdmissionOrigin::AutomaticSource), expected[1]);
  }
}

#[test]
fn project_policy_access_publications_verify_versions_and_permissions_on_restore() {
  let id = "30000000-0000-4000-8000-000000000001".parse().unwrap();
  let project = "20000000-0000-4000-8000-000000000001".parse().unwrap();
  let definition = FactoryAccessProfileDefinition {
    name: FactoryText::new("No execution").unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
  };
  let first = FactoryAccessProfile::publish(id, project, definition.clone()).unwrap();
  let next = first
    .replace(FactoryAccessProfileVersion::new(2).unwrap(), definition.clone())
    .unwrap();
  assert_eq!(next.reference().id, id);
  assert_eq!(next.reference().project_id, project);
  assert_eq!(next.reference().version.get(), 2);
  assert_ne!(first.reference().digest, next.reference().digest);
  assert_eq!(first.reference().version.get(), 1);
  assert!(
    first
      .replace(FactoryAccessProfileVersion::INITIAL, definition.clone())
      .is_err()
  );
  assert!(
    first
      .replace(FactoryAccessProfileVersion::new(3).unwrap(), definition)
      .is_err()
  );
  let retained = serde_json::to_value(&first).unwrap();
  assert_eq!(
    serde_json::from_value::<FactoryAccessProfile>(retained.clone()).unwrap(),
    first
  );
  for (path, replacement) in [
    (
      "/reference/id",
      serde_json::json!("30000000-0000-4000-8000-000000000002"),
    ),
    (
      "/reference/project_id",
      serde_json::json!("20000000-0000-4000-8000-000000000002"),
    ),
    ("/reference/version", serde_json::json!(2)),
    ("/reference/digest", serde_json::json!("00".repeat(32))),
    ("/definition/name", serde_json::json!("Changed profile")),
    (
      "/definition/permissions/network_hosts",
      serde_json::json!(["api.example.com"]),
    ),
  ] {
    let mut changed = retained.clone();
    *changed.pointer_mut(path).unwrap() = replacement;
    assert!(
      serde_json::from_value::<FactoryAccessProfile>(changed).is_err(),
      "accepted tampered {path}"
    );
  }
}

#[test]
fn project_policy_access_profiles_reuse_permission_narrowing_without_silent_clamping() {
  let ceiling = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    network_hosts: vec![
      NetworkHost::new("api.example.com").unwrap(),
      NetworkHost::new("other.example.com").unwrap(),
    ],
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace").unwrap(),
      MountMode::ReadOnly,
    )],
    secret_profiles: vec![FactoryKey::new("model").unwrap()],
    resources: FactoryResourceLimits::new(1000, 4096, 8192, 8, 600_000).unwrap(),
    ..Default::default()
  })
  .unwrap();
  let selected = FactoryPermissionDraft {
    network_hosts: vec![NetworkHost::new("api.example.com").unwrap()],
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace/source").unwrap(),
      MountMode::ReadOnly,
    )],
    secret_profiles: vec![FactoryKey::new("model").unwrap()],
    resources: FactoryResourceLimits::new(500, 2048, 4096, 4, 300_000).unwrap(),
    ..Default::default()
  };
  let definition = FactoryAccessProfileDefinition {
    name: FactoryText::new("Read selected source").unwrap(),
    permissions: FactoryPermissionSet::try_new(selected.clone()).unwrap(),
  };
  definition.validate_within(&ceiling).unwrap();
  let profile = FactoryAccessProfile::publish(
    "30000000-0000-4000-8000-000000000001"
      .parse::<FactoryAccessProfileId>()
      .unwrap(),
    "20000000-0000-4000-8000-000000000001".parse().unwrap(),
    definition.clone(),
  )
  .unwrap();
  assert_eq!(profile.definition().permissions, definition.permissions);
  assert_eq!(
    profile
      .definition()
      .permissions
      .network_hosts()
      .map(NetworkHost::as_str)
      .collect::<Vec<_>>(),
    ["api.example.com"]
  );

  for broader in [
    FactoryPermissionDraft {
      network_hosts: vec![NetworkHost::new("foreign.example.com").unwrap()],
      ..selected.clone()
    },
    FactoryPermissionDraft {
      mounts: vec![MountPermission::new(
        FactoryPath::new("/workspace/source").unwrap(),
        MountMode::ReadWrite,
      )],
      ..selected.clone()
    },
    FactoryPermissionDraft {
      secret_profiles: vec![FactoryKey::new("other-secret").unwrap()],
      ..selected.clone()
    },
    FactoryPermissionDraft {
      resources: FactoryResourceLimits::new(1001, 2048, 4096, 4, 300_000).unwrap(),
      ..selected
    },
  ] {
    let proposed = FactoryAccessProfileDefinition {
      name: definition.name.clone(),
      permissions: FactoryPermissionSet::try_new(broader).unwrap(),
    };
    let original = proposed.permissions.clone();
    assert!(proposed.validate_within(&ceiling).is_err());
    assert_eq!(
      proposed.permissions, original,
      "validation must not clamp a rejected request"
    );
  }
  FactoryAccessProfileDefinition {
    name: definition.name,
    permissions: FactoryPermissionSet::deny_all(),
  }
  .validate_within(&FactoryPermissionSet::deny_all())
  .unwrap();
}

#[test]
fn project_policy_launch_publications_pin_identity_versions_and_verify_retained_content() {
  let id = "10000000-0000-4000-8000-000000000001"
    .parse::<FactoryLaunchPolicyId>()
    .unwrap();
  let project = "20000000-0000-4000-8000-000000000001".parse().unwrap();
  let definition = FactoryLaunchPolicyDefinition {
    name: FactoryText::new("Operator policy").unwrap(),
    allow_manual: true,
    allow_automatic_source: false,
  };
  let first = FactoryLaunchPolicy::publish(id, project, definition.clone()).unwrap();
  assert_eq!(first.reference().id, id);
  assert_eq!(first.reference().project_id, project);
  assert_eq!(first.reference().version.get(), 1);
  let next = first
    .replace(FactoryLaunchPolicyVersion::new(2).unwrap(), definition.clone())
    .unwrap();
  assert_eq!(next.reference().id, id);
  assert_eq!(next.reference().project_id, project);
  assert_ne!(next.reference().digest, first.reference().digest);
  assert_eq!(first.reference().version.get(), 1);
  assert!(
    first
      .replace(FactoryLaunchPolicyVersion::INITIAL, definition.clone())
      .is_err()
  );
  assert!(
    first
      .replace(FactoryLaunchPolicyVersion::new(3).unwrap(), definition)
      .is_err()
  );

  let retained = serde_json::to_value(&first).unwrap();
  assert_eq!(
    serde_json::from_value::<FactoryLaunchPolicy>(retained.clone()).unwrap(),
    first
  );
  for (path, replacement) in [
    (
      "/reference/id",
      serde_json::json!("10000000-0000-4000-8000-000000000002"),
    ),
    (
      "/reference/project_id",
      serde_json::json!("20000000-0000-4000-8000-000000000002"),
    ),
    ("/reference/version", serde_json::json!(2)),
    ("/reference/digest", serde_json::json!("00".repeat(32))),
    ("/definition/name", serde_json::json!("Changed policy")),
    ("/definition/allow_manual", serde_json::json!(false)),
    ("/definition/allow_automatic_source", serde_json::json!(true)),
  ] {
    let mut changed = retained.clone();
    *changed.pointer_mut(path).unwrap() = replacement;
    assert!(
      serde_json::from_value::<FactoryLaunchPolicy>(changed).is_err(),
      "accepted tampered {path}"
    );
  }
}

#[test]
fn project_policy_definitions_preserve_existing_value_and_permission_bounds() {
  assert!(FactoryLaunchPolicyVersion::new(0).is_err());
  assert!(FactoryAccessProfileVersion::new(0).is_err());
  for invalid_id in ["", "00000000-0000-0000-0000-000000000000", "not-an-id"] {
    assert!(invalid_id.parse::<FactoryLaunchPolicyId>().is_err());
    assert!(invalid_id.parse::<FactoryAccessProfileId>().is_err());
  }
  let launch = serde_json::json!({"name": "Manual", "allow_manual": true, "allow_automatic_source": false});
  let mut missing_flag = launch.clone();
  missing_flag.as_object_mut().unwrap().remove("allow_automatic_source");
  assert!(serde_json::from_value::<FactoryLaunchPolicyDefinition>(missing_flag).is_err());
  for name in ["".to_owned(), "x".repeat(crate::MAX_FACTORY_TEXT_BYTES + 1)] {
    let mut invalid = launch.clone();
    invalid["name"] = serde_json::json!(name);
    assert!(serde_json::from_value::<FactoryLaunchPolicyDefinition>(invalid).is_err());
  }
  let mut unknown = launch;
  unknown["allow_unrestricted"] = serde_json::json!(true);
  assert!(serde_json::from_value::<FactoryLaunchPolicyDefinition>(unknown).is_err());

  let definition = FactoryAccessProfileDefinition {
    name: FactoryText::new("Bounded profile").unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
  };
  let mut invalid = serde_json::to_value(&definition).unwrap();
  invalid["permissions"]["network_hosts"] = serde_json::json!(["https://api.example.com/path"]);
  assert!(serde_json::from_value::<FactoryAccessProfileDefinition>(invalid).is_err());
}

#[test]
fn project_policy_access_publication_is_canonical_and_bounded_as_a_whole() {
  let id = "30000000-0000-4000-8000-000000000001".parse().unwrap();
  let project = "20000000-0000-4000-8000-000000000001".parse().unwrap();
  let profile = |hosts| {
    FactoryAccessProfile::publish(
      id,
      project,
      FactoryAccessProfileDefinition {
        name: FactoryText::new("Read services").unwrap(),
        permissions: FactoryPermissionSet::try_new(FactoryPermissionDraft {
          network_hosts: hosts,
          ..Default::default()
        })
        .unwrap(),
      },
    )
    .unwrap()
  };
  let a = NetworkHost::new("a.example.com").unwrap();
  let b = NetworkHost::new("b.example.com").unwrap();
  assert_eq!(profile(vec![a.clone(), b.clone()]), profile(vec![b, a]));

  // Every individual grant is legal; only the complete policy exceeds the publication bound.
  let mounts = (0..crate::MAX_PERMISSION_ENTRIES_PER_CATEGORY)
    .map(|index| {
      let prefix = format!("/source-{index:03}/");
      let path = format!(
        "{prefix}{}",
        "x".repeat(crate::MAX_PERMISSION_PATH_BYTES - prefix.len())
      );
      MountPermission::new(FactoryPath::new(path).unwrap(), MountMode::ReadOnly)
    })
    .collect();
  let oversized = FactoryAccessProfileDefinition {
    name: FactoryText::new("Oversized profile").unwrap(),
    permissions: FactoryPermissionSet::try_new(FactoryPermissionDraft {
      mounts,
      ..Default::default()
    })
    .unwrap(),
  };
  assert!(serde_json::to_vec(&oversized).unwrap().len() > crate::MAX_FACTORY_POLICY_DEFINITION_BYTES);
  assert!(matches!(
    FactoryAccessProfile::publish(id, project, oversized),
    Err(crate::FactoryError::InvalidConfiguration {
      field: "project policy definition bytes"
    })
  ));
}
