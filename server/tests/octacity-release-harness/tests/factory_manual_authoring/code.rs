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
  resources: &installation::Resources,
  base: &FactoryCodexBuildProfile,
  backend: &ReleaseBackend,
  released_plugins: &Path,
) -> CodeInstallation {
  let mirror = directory.join("code-mirror");
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
  let content = "code-guide-canary: console owns the operator interface\n";
  let stream = format!(
    "blob\nmark :1\ndata {}\n{content}\ncommit refs/heads/main\ncommitter Fixture <fixture@example.invalid> 1 +0000\ndata 7\nfixture\nM 100644 :1 README.md\n\ndone\n",
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
  let remote = "https://manual-source.example/code-guide";
  let source_plugins = local_mirror::install_local_mirror(
    &directory.join("mirror-transport"),
    released_plugins,
    remote,
    &mirror,
    git,
  );
  let repository = resource_id(&post_management(client,origin,"/api/v1/repositories","code-repository",json!({
    "project_id":resources.project,"name":"Manual code guide","definition":{"vcs_integration_id":Uuid::new_v4(),"repository_locator":remote,"selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}
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
    "code-worker",
    json!({"project_id":resources.project,"name":"Read-only managed worker","definition":definition}),
  )
  .await;
  let mut profile = base.clone();
  profile.build_configuration = BuildConfigurationRef::new(
    resource_id(&published).parse().unwrap(),
    BuildConfigurationVersion::INITIAL,
    resources.project.parse().unwrap(),
    FactoryDigest::content_sha256(&serde_json::to_vec(&published["resource"]["definition"]).unwrap()),
  );
  let mut permissions = serde_json::to_value(&profile.project_permissions).unwrap();
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
  policy["policy"]["artifacts"]["value"] = original["definition"]["artifacts"].clone();
  policy["policy"]["concurrency"]["value"] = json!({"active_builds":2,"active_jobs":2});
  let response = client
    .post(format!(
      "{origin}/api/v1/projects/{}/policy-versions",
      resources.project
    ))
    .header("idempotency-key", "code-policy")
    .header("if-match", "\"1\"")
    .json(&policy)
    .send()
    .await
    .unwrap();
  assert!(
    response.status().is_success(),
    "technical source policy: {}",
    response.status()
  );

  CodeInstallation {
    profile,
    repository,
    revision,
    source_plugins,
  }
}
