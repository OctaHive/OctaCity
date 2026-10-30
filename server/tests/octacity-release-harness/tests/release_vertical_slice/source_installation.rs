//! Trusted, disposable Source-only installation for packaged browser/setup qualification.
//! Only the HTTPS provider API is simulated; the adapter executable is unmodified.
use super::*;
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Scope {
  Tasks,
  Status,
  Factory,
}

pub(super) struct Installation {
  pub(super) catalog: PathBuf,
  pub(super) reporter: bool,
  pub(super) adapter_sha256: String,
  pub(super) configuration_sha256: String,
  pub(super) profiles_sha256: String,
  pub(super) resource: Value,
  pub(super) installation: Value,
  pub(super) endpoint: String,
}

pub(super) fn install(directory: &Path, forge: &source_provider::Forge, scope: Scope) -> Installation {
  let reporter = scope != Scope::Tasks;
  let adapter = required_path("OCTACITY_RELEASE_GITHUB_BINARY", true);
  let registry = directory.join("adapters");
  let provider = registry.join("github");
  fs::create_dir_all(&provider).unwrap();
  set_mode(directory, 0o700);
  set_mode(&registry, 0o700);
  set_mode(&provider, 0o700);
  fs::copy(&adapter, provider.join("plugin")).unwrap();
  set_mode(&provider.join("plugin"), 0o700);
  for name in ["credentials", "selections"] {
    fs::create_dir(provider.join(name)).unwrap();
    set_mode(&provider.join(name), 0o700);
  }
  if reporter {
    fs::create_dir(provider.join("mappings")).unwrap();
    set_mode(&provider.join("mappings"), 0o700);
  }
  let mappings = if reporter {
    "mapping_directory=\"mappings\"\n"
  } else {
    ""
  };
  let reporter_repositories = if reporter {
    "reporter_repositories=[\"owner/tickets\"]\n"
  } else {
    ""
  };
  let extra = if scope == Scope::Factory {
    ",\"research/backlog\""
  } else {
    ""
  };
  let extra_repository = if scope == Scope::Factory {
    "[repositories.\"research/backlog\"]\nrepository_id=\"5678\"\nowner=\"research\"\nname=\"backlog\"\n"
  } else {
    ""
  };
  let native = format!(
    "config_version=1\napi_base={:?}\napi_version=\"2026-03-10\"\napi_root_certificates_pem=[{:?}]\ncredential_directory=\"credentials\"\nselection_directory=\"selections\"\n{mappings}[capabilities]\nsource_repositories=[\"owner/tickets\"{extra}]\n{reporter_repositories}[repositories.\"owner/tickets\"]\nrepository_id=\"1234\"\nowner=\"owner\"\nname=\"tickets\"\n{extra_repository}",
    forge.endpoint, forge.certificate
  );
  private_file(&provider, "github-work.toml", &native);
  let digest = |bytes: &[u8]| hex::encode(Sha256::digest(bytes));
  let binary_digest = digest(&fs::read(provider.join("plugin")).unwrap());
  let configuration_digest = digest(native.as_bytes());
  private_file(
    &provider,
    "adapter.toml",
    &format!(
      "manifest_version=1\nadapter_id=\"github\"\nexecutable=\"plugin\"\nexecutable_sha256={binary_digest:?}\n[protocol.source]\nmin=1\nmax=1\n[capabilities.source]\ncheck=true\nauthorize=true\npreview=true\nlabel_presence=true\ncancel=false\ncredential_class=\"work_source_read\"\nconfiguration_digest={configuration_digest:?}\n"
    ),
  );
  if reporter {
    let manifest = provider.join("adapter.toml");
    let mut contents = fs::read_to_string(&manifest).unwrap();
    contents.push_str(&format!("\n[protocol.reporter]\nmin=1\nmax=2\n[capabilities.reporter]\ncheck=true\ncustom_effects=true\nobserve=true\nproject=true\ncancel=false\nidempotency=\"observe_before_retry\"\ncredential_class=\"work_reporter_write\"\nconfiguration_digest={configuration_digest:?}\n"));
    fs::write(manifest, contents).unwrap();
  }
  let exact = |identity: &str, digest: &str| json!({"identity":identity,"version":"v1","digest":digest});
  let installation = exact("packaged-github", &configuration_digest);
  let plugin = exact("github", &binary_digest);
  let reference = exact("github-issues", &configuration_digest);
  let credential =
    json!({"provider":"installed","reference":format!("packaged-reader-{}",&configuration_digest[..16])});
  let capability = json!({"purpose":"task_read","capability":exact("issue-reader",&configuration_digest),
    "configuration":exact("issue-reader-profile",&configuration_digest),"credential":credential});
  let mut resource =
    json!({"reference":reference,"native_id":"1234","locator":"owner/tickets","capabilities":[capability]});
  let read = json!({"installation":installation,"resource_reference":reference,"capability":capability,"plugin":plugin,
    "provider_configuration":configuration_digest,"resource":{"resource_id":"1234","resource_locator":"owner/tickets",
      "credential":{"class":"work_source_read","handle":"issue-reader"}},"network_hosts":["localhost"]});
  let policy: Value =
    serde_json::from_str(include_str!("../../../../../deployment/local-stand/source-policy.json")).unwrap();
  let selection = serde_json::to_string(&policy["selector"]["definition"]).unwrap();
  private_file(
    &provider.join("selections"),
    &format!("{}.json", digest(b"all-issues")),
    &selection,
  );
  let mut reader = policy["reader"].clone();
  if scope == Scope::Factory {
    reader["preparation"]["mapping"]["source"] = json!({"installation":installation,
      "capability":capability["capability"],"configuration":capability["configuration"],"resource":reference});
    reader["preparation"]["source"] = json!({"kind":"exact_revision","value":"a".repeat(40)});
  } else {
    reader.as_object_mut().unwrap().remove("preparation");
  }
  reader["read"] = read.clone();
  reader["selection_labels"] = policy["selector"]["definition"].clone();
  reader["selection"] = json!({"identity":"all-issues","sha256":digest(selection.as_bytes())});
  let mut reporters = Vec::new();
  let mut checks = vec![read];
  let mut targets =
    vec![json!({"reference":credential,"handle":"issue-reader","purpose":"task_read","resources":["owner/tickets"]})];
  if reporter {
    let credential =
      json!({"provider":"installed","reference":format!("packaged-writer-{}",&configuration_digest[..16])});
    let capability = json!({"purpose":"task_update","capability":exact("issue-reporter",&configuration_digest),
      "configuration":exact("issue-reporter-profile",&configuration_digest),"credential":credential});
    resource["capabilities"]
      .as_array_mut()
      .unwrap()
      .push(capability.clone());
    let write = json!({"installation":installation,"resource_reference":reference,"capability":capability,"plugin":plugin,
      "provider_configuration":configuration_digest,"resource":{"resource_id":"1234","resource_locator":"owner/tickets",
        "credential":{"class":"work_reporter_write","handle":"issue-reporter"}},"network_hosts":["localhost"]});
    let mapping = serde_json::to_string(&policy["mapping"]["definition"]).unwrap();
    private_file(
      &provider.join("mappings"),
      &format!("{}.json", digest(b"native-status")),
      &mapping,
    );
    let mut declaration = policy["reporter"].clone();
    declaration["write"] = write.clone();
    declaration["mapping"] = json!({"identity":"native-status","sha256":digest(mapping.as_bytes())});
    declaration["credential_profile"] = json!("issue-reporter");
    declaration["reporting"]["mapping"] = json!({"statuses":[
      {"status":"native-open","add":[],"remove":[],"state":"open"},
      {"status":"native-closed","add":[],"remove":[],"state":"closed"}
    ]});
    reporters.push(declaration);
    checks.push(write);
    targets.push(
      json!({"reference":credential,"handle":"issue-reporter","purpose":"task_update","resources":["owner/tickets"]}),
    );
  }
  let mut readers = vec![reader.clone()];
  let mut resources = vec![resource.clone()];
  if scope == Scope::Factory {
    let reference = exact("research-issues", &configuration_digest);
    let credential = json!({"provider":"installed",
      "reference":format!("packaged-research-reader-{}",&configuration_digest[..16])});
    let mut second = resource.clone();
    second["reference"] = reference.clone();
    second["native_id"] = json!("5678");
    second["locator"] = json!("research/backlog");
    second["capabilities"].as_array_mut().unwrap().truncate(1);
    second["capabilities"][0]["credential"] = credential.clone();
    let mut read = reader["read"].clone();
    read["capability"] = second["capabilities"][0].clone();
    read["resource_reference"] = reference.clone();
    read["resource"]["resource_id"] = json!("5678");
    read["resource"]["resource_locator"] = json!("research/backlog");
    read["resource"]["credential"]["handle"] = json!("research-reader");
    checks.push(read.clone());
    reader["identity"] = json!("research-issues");
    reader["read"] = read;
    reader["preparation"]["mapping"]["source"]["resource"] = reference;
    readers.push(reader);
    resources.push(second);
    targets.push(
      json!({"reference":credential,"handle":"research-reader","purpose":"task_read",
      "resources":["research/backlog"]}),
    );
  }
  let profiles = private_file(
    directory,
    "source-profiles.json",
    &json!({"version":1,"readers":readers,"reporters":reporters}).to_string(),
  );
  let catalog = private_file(directory,"integrations.json", &json!({"version":1,"registry":registry,
    "installations":[{"installation":installation,"provider":"github","adapter":plugin,"endpoints":[forge.endpoint],
      "authentication":["token"],"resources":resources}],"checks":checks,"source_profiles_file":profiles,
    "credential_publishers":[{"kind":"github_private_files","root":provider,"directory":"credentials","targets":targets}]
  }).to_string());
  Installation {
    catalog,
    reporter,
    adapter_sha256: binary_digest,
    configuration_sha256: configuration_digest,
    profiles_sha256: digest(&fs::read(profiles).unwrap()),
    resource,
    installation,
    endpoint: forge.endpoint.clone(),
  }
}

impl Installation {
  /// Explicit fixture installation review; literal labels must already exist at
  /// the native provider. This never creates labels or changes Project grants.
  pub(super) fn review_reporter_labels(&mut self, labels: &[&str]) {
    assert!(self.reporter);
    let catalog: Value = serde_json::from_slice(&fs::read(&self.catalog).unwrap()).unwrap();
    let profiles_path = Path::new(catalog["source_profiles_file"].as_str().unwrap());
    let mut profiles: Value = serde_json::from_slice(&fs::read(profiles_path).unwrap()).unwrap();
    let reporter = &mut profiles["reporters"][0];
    let identity = reporter["mapping"]["identity"].as_str().unwrap();
    let mapping_path = Path::new(catalog["registry"].as_str().unwrap())
      .join("github/mappings")
      .join(format!("{}.json", hex::encode(Sha256::digest(identity.as_bytes()))));
    let mut mapping: Value = serde_json::from_slice(&fs::read(&mapping_path).unwrap()).unwrap();
    mapping["managed_label_groups"] = json!({"qualification":labels});
    let bytes = serde_json::to_vec(&mapping).unwrap();
    fs::write(mapping_path, &bytes).unwrap();
    reporter["mapping"]["sha256"] = json!(hex::encode(Sha256::digest(&bytes)));
    reporter["reporting"]["editable"]["labels"] = json!(labels);
    let bytes = serde_json::to_vec(&profiles).unwrap();
    fs::write(profiles_path, &bytes).unwrap();
    self.profiles_sha256 = hex::encode(Sha256::digest(&bytes));
  }
}
