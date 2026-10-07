use crate::{
  CommandArgumentPattern, CommandPermission, FactoryDigest, FactoryError, FactoryKey, FactoryOutputPermissions,
  FactoryPath, FactoryPermissionDraft, FactoryPermissionSet, FactoryResourceLimits, FactoryText, ImmutableReference,
  LocalPermissionCeiling, MAX_COMMAND_ARGUMENTS, MAX_FACTORY_CPU_MILLIS, MountMode, MountPermission, NetworkHost,
  PermissionCategory, resolve_factory_permissions,
};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn text(value: &str) -> FactoryText {
  FactoryText::new(value).expect("fixture text is valid")
}

fn reference(identity: &str, digest_byte: u8) -> ImmutableReference {
  ImmutableReference::new(
    key(identity),
    key("1.0.0"),
    FactoryDigest::from_bytes([digest_byte; 32]),
  )
}

fn command(executable: &ImmutableReference, arguments: Vec<CommandArgumentPattern>) -> CommandPermission {
  CommandPermission::try_new(executable.clone(), arguments).expect("fixture command is valid")
}

fn output(kinds: &[&str], count: u32, bytes: u64) -> FactoryOutputPermissions {
  FactoryOutputPermissions::try_new(
    kinds.iter().map(|value| key(value)).collect(),
    count,
    bytes,
    count,
    bytes,
  )
  .expect("fixture output policy is valid")
}

fn resources(value: u32) -> FactoryResourceLimits {
  FactoryResourceLimits::new(value, u64::from(value), u64::from(value), value, u64::from(value))
    .expect("fixture resource policy is valid")
}

#[test]
fn deny_all_is_the_safe_default() {
  let permissions = FactoryPermissionSet::deny_all();

  assert_eq!(permissions.plugins().len(), 0);
  assert_eq!(permissions.executables().len(), 0);
  assert_eq!(permissions.tools().len(), 0);
  assert_eq!(permissions.commands().len(), 0);
  assert_eq!(permissions.max_descendants(), 0);
  assert_eq!(permissions.mounts().len(), 0);
  assert_eq!(permissions.network_hosts().len(), 0);
  assert_eq!(permissions.secret_profiles().len(), 0);
  assert_eq!(permissions.workload_identity_profiles().len(), 0);
  assert_eq!(permissions.resources(), FactoryResourceLimits::default());
  assert_eq!(permissions.outputs(), &FactoryOutputPermissions::default());
  assert_eq!(permissions.digest(), FactoryPermissionSet::deny_all().digest());
}

#[test]
fn every_permission_category_only_narrows_across_all_four_layers() {
  let plugin = reference("plugin.codex", 1);
  let executable = reference("executable.codex", 2);
  let tool = reference("tool.shell", 3);
  let project = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin.clone(), reference("plugin.extra", 4)],
    executables: vec![executable.clone(), reference("executable.extra", 5)],
    tools: vec![tool.clone(), reference("tool.extra", 6)],
    commands: vec![command(
      &executable,
      vec![CommandArgumentPattern::Any, CommandArgumentPattern::Any],
    )],
    max_descendants: 8,
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace").expect("path"),
      MountMode::ReadWrite,
    )],
    network_hosts: vec![
      NetworkHost::new("api.example.com").expect("host"),
      NetworkHost::new("extra.example.com").expect("host"),
    ],
    secret_profiles: vec![key("github"), key("extra")],
    workload_identity_profiles: vec![key("delivery"), key("extra")],
    resources: resources(800),
    outputs: output(&["changeset", "trace"], 8, 800),
  })
  .expect("project permissions are valid");
  let configuration = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin.clone()],
    executables: vec![executable.clone()],
    tools: vec![tool.clone()],
    commands: vec![command(
      &executable,
      vec![
        CommandArgumentPattern::Exact(text("--check")),
        CommandArgumentPattern::Any,
      ],
    )],
    max_descendants: 6,
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace/source").expect("path"),
      MountMode::ReadWrite,
    )],
    network_hosts: vec![NetworkHost::new("api.example.com").expect("host")],
    secret_profiles: vec![key("github")],
    workload_identity_profiles: vec![key("delivery")],
    resources: resources(600),
    outputs: output(&["changeset"], 6, 600),
  })
  .expect("configuration permissions are valid");
  let task = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin.clone()],
    executables: vec![executable.clone()],
    tools: vec![tool.clone()],
    commands: vec![command(
      &executable,
      vec![
        CommandArgumentPattern::Exact(text("--check")),
        CommandArgumentPattern::Exact(text("src")),
      ],
    )],
    max_descendants: 4,
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace/source").expect("path"),
      MountMode::ReadOnly,
    )],
    network_hosts: vec![NetworkHost::new("api.example.com").expect("host")],
    secret_profiles: vec![key("github")],
    workload_identity_profiles: vec![key("delivery")],
    resources: resources(400),
    outputs: output(&["changeset"], 4, 400),
  })
  .expect("task permissions are valid");
  let local_grants = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin],
    executables: vec![executable.clone()],
    tools: vec![tool],
    commands: vec![command(
      &executable,
      vec![CommandArgumentPattern::Any, CommandArgumentPattern::Exact(text("src"))],
    )],
    max_descendants: 2,
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace/source/repository").expect("path"),
      MountMode::ReadOnly,
    )],
    network_hosts: vec![NetworkHost::new("api.example.com").expect("host")],
    secret_profiles: vec![key("github")],
    workload_identity_profiles: vec![key("delivery")],
    resources: resources(200),
    outputs: output(&["changeset"], 2, 200),
  })
  .expect("local permissions are valid");

  let effective = resolve_factory_permissions(
    &project,
    &configuration,
    &task,
    &LocalPermissionCeiling::fully_enforced(local_grants.clone()),
  )
  .expect("all categories are enforceable");

  assert_eq!(effective.digest(), effective.clone().digest());
  assert_ne!(effective.digest(), task.digest());

  for enclosing in [&project, &configuration, &task, &local_grants] {
    assert!(effective.is_no_broader_than(enclosing));
  }
  assert_eq!(effective.plugins().len(), 1);
  assert_eq!(effective.executables().len(), 1);
  assert_eq!(effective.tools().len(), 1);
  let effective_command = effective.commands().next().expect("one command remains");
  assert_eq!(
    effective_command.arguments(),
    [
      CommandArgumentPattern::Exact(text("--check")),
      CommandArgumentPattern::Exact(text("src")),
    ]
  );
  assert_eq!(effective.max_descendants(), 2);
  let mount = effective.mounts().next().expect("one mount remains");
  assert_eq!(mount.root().as_str(), "/workspace/source/repository");
  assert_eq!(mount.mode(), MountMode::ReadOnly);
  assert_eq!(
    effective.network_hosts().next().expect("host").as_str(),
    "api.example.com"
  );
  assert_eq!(effective.secret_profiles().next(), Some(&key("github")));
  assert_eq!(effective.workload_identity_profiles().next(), Some(&key("delivery")));
  assert_eq!(effective.resources().cpu_millis(), 200);
  assert_eq!(effective.resources().memory_bytes(), 200);
  assert_eq!(effective.resources().disk_bytes(), 200);
  assert_eq!(effective.resources().process_count(), 200);
  assert_eq!(effective.resources().elapsed_millis(), 200);
  assert_eq!(effective.outputs().kinds().next(), Some(&key("changeset")));
  assert_eq!(effective.outputs().max_artifact_count(), 2);
  assert_eq!(effective.outputs().max_artifact_bytes(), 200);
  assert_eq!(effective.outputs().max_report_count(), 2);
  assert_eq!(effective.outputs().max_report_bytes(), 200);
  assert!(!project.is_no_broader_than(&effective));
}

#[test]
fn unknown_and_unenforceable_authority_fail_closed() {
  assert!(matches!(
    "future".parse::<PermissionCategory>(),
    Err(FactoryError::UnsupportedEnum { .. })
  ));
  assert!(matches!(
    "execute".parse::<MountMode>(),
    Err(FactoryError::UnsupportedEnum { .. })
  ));

  for missing in PermissionCategory::ALL {
    let enforceable = PermissionCategory::ALL
      .into_iter()
      .filter(|category| *category != missing)
      .collect();
    let local = LocalPermissionCeiling::try_new(FactoryPermissionSet::deny_all(), enforceable)
      .expect("unique capabilities are valid");
    assert_eq!(
      resolve_factory_permissions(
        &FactoryPermissionSet::deny_all(),
        &FactoryPermissionSet::deny_all(),
        &FactoryPermissionSet::deny_all(),
        &local,
      ),
      Err(FactoryError::UnenforceablePermission { category: missing })
    );
  }
}

#[test]
fn command_and_path_intersection_rejects_incompatible_or_lookalike_authority() {
  let executable = reference("executable.codex", 1);
  let left = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    executables: vec![executable.clone()],
    commands: vec![command(
      &executable,
      vec![CommandArgumentPattern::Exact(text("--safe"))],
    )],
    mounts: vec![MountPermission::new(
      FactoryPath::new("/work").expect("path"),
      MountMode::ReadWrite,
    )],
    ..FactoryPermissionDraft::default()
  })
  .expect("left permissions are valid");
  let right = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    executables: vec![executable.clone()],
    commands: vec![command(
      &executable,
      vec![CommandArgumentPattern::Exact(text("--unsafe"))],
    )],
    mounts: vec![MountPermission::new(
      FactoryPath::new("/workspace").expect("path"),
      MountMode::ReadOnly,
    )],
    ..FactoryPermissionDraft::default()
  })
  .expect("right permissions are valid");

  let effective = left.intersection(&right);
  assert_eq!(effective.commands().len(), 0);
  assert_eq!(effective.mounts().len(), 0);
  assert_eq!(left.intersection(&right), right.intersection(&left));
}

#[test]
fn permission_values_reject_ambiguous_and_unbounded_input() {
  for value in ["relative", "/workspace/../secret", "/workspace//source", "C:\\source"] {
    assert_eq!(
      FactoryPath::new(value),
      Err(FactoryError::InvalidPermission {
        category: PermissionCategory::FilesystemPaths,
      })
    );
  }
  for value in [
    "HTTPS://example.com",
    "*.example.com",
    "example.com:443",
    "example..com",
    "127.000.000.001",
    "2130706433",
  ] {
    assert_eq!(
      NetworkHost::new(value),
      Err(FactoryError::InvalidPermission {
        category: PermissionCategory::NetworkHosts,
      })
    );
  }
  assert_eq!(FactoryPath::new("/").expect("root is canonical").as_str(), "/");
  assert_eq!(
    NetworkHost::new("127.0.0.1").expect("canonical IP is valid").as_str(),
    "127.0.0.1"
  );
  assert_eq!(
    FactoryResourceLimits::new(MAX_FACTORY_CPU_MILLIS + 1, 0, 0, 0, 0),
    Err(FactoryError::InvalidPermission {
      category: PermissionCategory::Cpu,
    })
  );
  assert_eq!(
    CommandPermission::try_new(
      reference("executable.codex", 1),
      vec![CommandArgumentPattern::Any; MAX_COMMAND_ARGUMENTS + 1],
    ),
    Err(FactoryError::InvalidPermission {
      category: PermissionCategory::CommandArguments,
    })
  );
}

#[test]
fn permission_sets_reject_duplicates_and_commands_for_unlisted_executables() {
  let plugin = reference("plugin.codex", 1);
  assert_eq!(
    FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![plugin.clone(), plugin],
      ..FactoryPermissionDraft::default()
    }),
    Err(FactoryError::DuplicatePermission {
      category: PermissionCategory::PluginIdentity,
    })
  );

  assert_eq!(
    FactoryPermissionSet::try_new(FactoryPermissionDraft {
      mounts: vec![
        MountPermission::new(FactoryPath::new("/workspace").expect("path"), MountMode::ReadWrite),
        MountPermission::new(FactoryPath::new("/workspace/input").expect("path"), MountMode::ReadOnly,),
      ],
      ..FactoryPermissionDraft::default()
    }),
    Err(FactoryError::InvalidPermission {
      category: PermissionCategory::FilesystemPaths,
    })
  );

  let executable = reference("executable.codex", 2);
  assert_eq!(
    FactoryPermissionSet::try_new(FactoryPermissionDraft {
      commands: vec![command(&executable, vec![])],
      ..FactoryPermissionDraft::default()
    }),
    Err(FactoryError::InvalidPermission {
      category: PermissionCategory::CommandArguments,
    })
  );
}
