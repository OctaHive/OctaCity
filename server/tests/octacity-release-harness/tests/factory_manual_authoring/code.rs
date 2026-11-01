//! Technical read-only worker and local Git mirror; no authored Factory definition.
use super::*;
use octacity_server_application::FactoryCodexBuildProfile;
use octacity_server_domain::BuildConfigurationVersion;
use std::io::Write as _;

pub(super) struct CodeInstallation {
  pub profile: FactoryCodexBuildProfile,
  pub repository: String,
  pub revision: String,
  pub source_plugins: PathBuf,
}

pub(super) async fn install(
  client: &Client,
  origin: &str,
  directory: &Path,
  resources: &mut installation::Resources,
  base: &FactoryCodexBuildProfile,
  backend: &ReleaseBackend,
  released_plugins: &Path,
) -> CodeInstallation {
  install_named(
    client,
    origin,
    directory,
    resources,
    base,
    backend,
    released_plugins,
    "code-guide",
    "code-guide-canary: console owns the operator interface\n",
    &[],
  )
  .await
}

/// Install independent technical sources without seeding a Flow or Work.
#[allow(clippy::too_many_arguments)]
pub(super) async fn install_named(
  client: &Client,
  origin: &str,
  directory: &Path,
  resources: &mut installation::Resources,
  base: &FactoryCodexBuildProfile,
  backend: &ReleaseBackend,
  released_plugins: &Path,
  name: &str,
  content: &str,
  previous: &[&CodeInstallation],
) -> CodeInstallation {
  let mirror = directory.join(format!("{name}-mirror"));
  let git = Path::new("/usr/bin/git");
  assert!(
    std::process::Command::new(git)
      .args(["-c", "init.templateDir=", "init", "--bare"])
      .arg(&mirror)
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .unwrap()
      .success()
  );
  let stream = format!(
    "blob\nmark :1\ndata {}\n{content}\nblob\nmark :2\ndata 25\nsource-unselected-canary\n\ncommit refs/heads/main\ncommitter Fixture <fixture@example.invalid> 1 +0000\ndata 7\nfixture\nM 100644 :1 README.md\nM 100644 :2 hidden.txt\n\ndone\n",
    content.len()
  );
  let mut import = std::process::Command::new(git)
    .arg("-C")
    .arg(&mirror)
    .arg("fast-import")
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
  import.stdin.take().unwrap().write_all(stream.as_bytes()).unwrap();
  assert!(import.wait().unwrap().success());
  let output = std::process::Command::new(git)
    .arg("-C")
    .arg(&mirror)
    .args(["rev-parse", "refs/heads/main"])
    .output()
    .unwrap();
  assert!(output.status.success());
  let revision = String::from_utf8(output.stdout).unwrap().trim().to_owned();
  let remote = format!("https://manual-source.example/{name}");
  let registry = octacity_source::SourcePluginRegistry::discover(released_plugins).unwrap();
  let transport_git = PathBuf::from(
    registry.get("git").unwrap().manifest.settings["git_path"]
      .as_str()
      .unwrap(),
  );
  let source_plugins = local_mirror::install_local_mirror(
    &directory.join(format!("{name}-transport")),
    released_plugins,
    &remote,
    &mirror,
    &transport_git,
  );
  let repository = resource_id(&post_management(client,origin,"/api/v1/repositories",&format!("{name}-repository"),json!({
    "project_id":resources.project,"name":format!("Manual {}", name.replace('-', " ")),"definition":{"vcs_integration_id":Uuid::new_v4(),"repository_locator":remote,"selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}
  })).await);
  let original = get_json(
    client,
    format!(
      "{origin}/api/v1/build-configurations/{}/versions/1",
      resources.configuration.id()
    ),
  )
  .await;
  let mut definition = original["definition"].clone();
  definition.as_object_mut().unwrap().remove("source");
  definition["repository_id"] = json!(repository);
  definition["repository_version"] = json!(1);
  let published = post_management(
    client,
    origin,
    "/api/v1/build-configurations",
    &format!("{name}-worker"),
    json!({"project_id":resources.project,"name":format!("Read-only {name} worker"),"definition":definition}),
  )
  .await;
  let mut profile = base.clone();
  profile.build_configuration = BuildConfigurationRef::new(
    resource_id(&published).parse().unwrap(),
    BuildConfigurationVersion::INITIAL,
    resources.project.parse().unwrap(),
    FactoryDigest::content_sha256(&serde_json::to_vec(&published["resource"]["definition"]).unwrap()),
  );
  let mut permissions = serde_json::to_value(&base.project_permissions).unwrap();
  permissions["mounts"]
    .as_array_mut()
    .unwrap()
    .push(json!({"root":"/workspace/source","mode":"read_only"}));
  profile.project_permissions = serde_json::from_value(permissions).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
  let mut policy = support::project_policy_body(
    &resources.pool,
    &repository,
    backend.runtime_class(),
    backend.execution_target(),
  );
  policy["policy"]["repositories"]["value"] = json!(
    previous
      .iter()
      .map(|code| code.repository.clone())
      .chain(std::iter::once(repository.clone()))
      .collect::<Vec<_>>()
  );
  policy["policy"]["artifacts"]["value"] = original["definition"]["artifacts"].clone();
  policy["policy"]["concurrency"]["value"] = json!({"active_builds":2,"active_jobs":2});
  let response = client
    .post(format!(
      "{origin}/api/v1/projects/{}/policy-versions",
      resources.project
    ))
    .header("idempotency-key", format!("{name}-policy"))
    .header("if-match", format!("\"{}\"", resources.policy_version))
    .json(&policy)
    .send()
    .await
    .unwrap();
  assert!(
    response.status().is_success(),
    "technical source policy: {}",
    response.status()
  );
  let published: Value = response.json().await.unwrap();
  resources.policy_version = published["resource"]["version"].as_u64().unwrap();

  CodeInstallation {
    profile,
    repository,
    revision,
    source_plugins,
  }
}

/// Observe the real SourceGit transport; no-code Jobs must never invoke it.
pub(super) fn observe_git(directory: &Path, released: &Path) -> PathBuf {
  let registry = octacity_source::SourcePluginRegistry::discover(released).unwrap();
  let original = registry.get("git").unwrap();
  let mut manifest = original.manifest.clone();
  let wrapper = directory.join("git-observer");
  let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
  fs::write(
    &wrapper,
    format!(
      "#!/bin/sh\nprintf '%s\\n' \"$PWD|$*\" >> {}\nexec {} \"$@\"\n",
      quote(directory.join("git-invocations").to_str().unwrap()),
      quote(manifest.settings["git_path"].as_str().unwrap())
    ),
  )
  .unwrap();
  set_mode(&wrapper, 0o700);
  manifest.settings.insert("git_path".into(), json!(wrapper));
  let root = directory.join("observed-source-plugins");
  let plugin = root.join(&manifest.name);
  fs::create_dir_all(&plugin).unwrap();
  let executable = plugin.join(&manifest.executable);
  fs::create_dir_all(executable.parent().unwrap()).unwrap();
  fs::copy(&original.executable, &executable).unwrap();
  set_mode(&executable, 0o700);
  fs::write(plugin.join("plugin.toml"), toml::to_string(&manifest).unwrap()).unwrap();
  octacity_source::SourcePluginRegistry::discover(&root).unwrap();
  root
}
