//! Actual pinned provider, retained candidate materialization and real Git/HTTPS effects.
use super::*;
use delivery_target::{ForgeResponseMode, GithubForge};
use octacity_server::{FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft};
use octacity_server_factory::*;
use sha2::{Digest as _, Sha256};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn text(min_bytes: usize, max_bytes: usize) -> FlowValueSchema {
  FlowValueSchema::String { min_bytes, max_bytes }
}
fn object(fields: impl IntoIterator<Item = (&'static str, FlowValueSchema)>) -> FlowValueSchema {
  FlowValueSchema::Object {
    fields: fields
      .into_iter()
      .map(|(name, schema)| (key(name), FlowFieldSchema::required(schema)))
      .collect(),
  }
}
fn quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) struct InstalledDelivery {
  forge: GithubForge,
  bare: PathBuf,
  adapter_root: PathBuf,
  inputs: PathBuf,
  identity: ImmutableReference,
  plugin: ImmutableReference,
  target: ImmutableReference,
  policy: FactoryDeliveryPolicyDraft,
  permissions: FactoryPermissionSet,
  input: FlowDataSchema,
  receipt: FlowDataSchema,
}
impl InstalledDelivery {
  pub(super) async fn new(directory: &Path, resources: &resources::PilotResources, remote: &str) -> Self {
    let root = directory.join("installed-delivery");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    let git = required_path("OCTACITY_CONTRACT_GIT_EXECUTABLE", true)
      .canonicalize()
      .unwrap();
    let bare = root.join("target.git");
    let cloned = Command::new(&git)
      .args(["clone", "--quiet", "--bare"])
      .arg(directory.join("repository"))
      .arg(&bare)
      .output()
      .await
      .unwrap();
    assert!(cloned.status.success(), "initialize test-owned real Git target");
    let forge = GithubForge::start(&root, &bare, ForgeResponseMode::HoldReviewResponse).await;
    let adapter_root = root.join("adapters");
    let adapter = adapter_root.join("github");
    fs::create_dir_all(&adapter).unwrap();
    set_mode(&adapter_root, 0o700);
    set_mode(&adapter, 0o700);
    let executable = adapter.join("octacity-github");
    fs::copy(required_path("OCTACITY_FACTORY_GITHUB_PLUGIN", true), &executable).unwrap();
    set_mode(&executable, 0o700);
    let plugin_sha = hex::encode(Sha256::digest(fs::read(&executable).unwrap()));
    let plugin = ImmutableReference::new(
      key("github"),
      key("v1"),
      FactoryDigest::from_lower_hex(&plugin_sha).unwrap(),
    );
    let wrapper = adapter.join("git-mirror");
    let local = url::Url::from_directory_path(bare.canonicalize().unwrap()).unwrap();
    let rewrite = format!("url.{}.insteadOf={remote}", local.as_str().trim_end_matches('/'));
    private_file(
      &adapter,
      "git-mirror",
      &format!(
        "#!/bin/sh\nexec {} -c {} \"$@\"\n",
        quote(git.to_str().unwrap()),
        quote(&rewrite)
      ),
    );
    set_mode(&wrapper, 0o700);
    let git_sha = hex::encode(Sha256::digest(fs::read(&wrapper).unwrap()));
    let inputs = adapter.join("inputs");
    let credentials = adapter.join("credentials");
    for path in [&inputs, &credentials] {
      fs::create_dir(path).unwrap();
      set_mode(path, 0o700);
    }
    let handle = "operator-delivery-handle";
    private_file(
      &credentials,
      &format!("{}.json", hex::encode(Sha256::digest(handle.as_bytes()))),
      &json!({"class":"delivery_write","token":"private-delivery-token","repositories":[remote]}).to_string(),
    );
    let config = toml::to_string(
      &json!({"config_version":1,"api_base":forge.endpoint(),"api_version":"2022-11-28",
      "api_root_certificates_pem":[forge.root_certificate_pem()],"git_path":wrapper,"git_sha256":git_sha,
      "credential_directory":"credentials","input_directory":"inputs","repositories":{(remote):{
        "repository_id":resources.repository,"owner":"owner","name":"repository","remote":remote}}}),
    )
    .unwrap();
    private_file(&adapter, "github-plugin.toml", &config);
    let config_digest = FactoryDigest::content_sha256(config.as_bytes());
    private_file(
      &adapter,
      "adapter.toml",
      &format!(
        "manifest_version=1\nadapter_id=\"github\"\nexecutable=\"octacity-github\"\nexecutable_sha256={plugin_sha:?}\n[protocol]\nmin=1\nmax=1\n[capabilities]\nobserve=true\npublish=true\ncancel=true\nidempotency=\"observe_before_retry\"\ncredential_class=\"delivery_write\"\nconfiguration_digest={:?}\n",
        config_digest.to_string()
      ),
    );
    let policy:FactoryDeliveryPolicyDraft=serde_json::from_value(json!({
      "plugin":plugin,"configuration_digest":config_digest,"project_id":resources.project,"repository_id":resources.repository,
      "repository_locator":remote,"credential_profile":"isolated-delivery-credential","credential_handle":handle,
      "network_hosts":["localhost",url::Url::parse(remote).unwrap().host_str().unwrap()],
      "target":{"base_reference":"refs/heads/main","candidate_reference_prefix":"refs/heads/pilot/"},
      "review":{"title":"Retained candidate","summary":"Actual isolated verification completed","draft":true}
    })).unwrap();
    let target = FactoryDeliveryPolicy::new(key("installed-review-target"), key("v1"), policy.clone())
      .unwrap()
      .reference()
      .clone();
    let identity = ImmutableReference::new(
      key("installed-candidate-publication"),
      key("v1"),
      FactoryDigest::sha256(
        "pilot.delivery-installation.v1",
        &[&plugin.digest().as_bytes(), &target.digest().as_bytes()],
      ),
    );
    let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![identity.clone(), plugin.clone()],
      secret_profiles: vec![key("isolated-delivery-credential")],
      network_hosts: policy.network_hosts.clone(),
      ..Default::default()
    })
    .unwrap();
    assert!(permissions.plugins().any(|allowed| allowed == &identity));
    assert!(permissions.plugins().any(|allowed| allowed == &plugin));
    let artifact = object([
      ("artifact_id", text(36, 36)),
      ("content_digest", text(64, 64)),
      (
        "encoded_size",
        FlowValueSchema::Integer {
          minimum: 0,
          maximum: 64 * 1024 * 1024,
        },
      ),
    ]);
    let input = FlowDataSchema::new(
      key("explicit-delivery-input"),
      key("v1"),
      object([
        ("candidate_revision", text(40, 64)),
        ("candidate_bundle", artifact.clone()),
        ("candidate_manifest", artifact),
        (
          "verified",
          FlowValueSchema::Enum {
            values: vec![json!(true)],
          },
        ),
      ]),
    )
    .unwrap();
    let receipt = FlowDataSchema::new(
      key("actual-delivery-receipt"),
      key("v1"),
      object([
        ("receipt", text(64, 64)),
        ("candidate_revision", text(40, 64)),
        ("changeset_digest", text(64, 64)),
        (
          "external",
          object([
            ("branch_reference", text(1, 1024)),
            ("remote_head", text(40, 64)),
            ("review_identity", text(1, 256)),
            (
              "review_state",
              FlowValueSchema::Enum {
                values: vec![json!("open"), json!("merged_externally")],
              },
            ),
          ]),
        ),
      ]),
    )
    .unwrap();
    Self {
      forge,
      bare,
      adapter_root,
      inputs,
      identity,
      plugin,
      target,
      policy,
      permissions,
      input,
      receipt,
    }
  }
  pub(super) fn install(&self, catalog: &mut Value, ceiling: &mut FactoryPermissionSet) {
    catalog["delivery"] = json!([self.entry()]);
    catalog["projects"][0]["references"]
      .as_array_mut()
      .unwrap()
      .extend(self.references());
    workflow_author::include_permissions(ceiling, self.permissions());
  }
  pub(super) fn entry(&self) -> Value {
    json!({"identity":self.identity,"outcome":{"key":"published","kind":"success","schema":self.receipt.reference()},
      "adapter_root":self.adapter_root,"policies":[{"reference":self.target,"draft":self.policy,"input_directory":self.inputs}],
      "timeout_milliseconds":10000,"termination_grace_milliseconds":1000})
  }
  pub(super) fn accepted(&self) -> bool {
    self.forge.has_review()
  }
  pub(super) fn release_response(&self) {
    self.forge.release_response();
  }
  pub(super) fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }
  pub(super) fn references(&self) -> Vec<Value> {
    vec![
      json!({"kind":"node_capability","alias":"delivery-provider","reference":self.plugin}),
      json!({"kind":"node_capability","alias":"delivery-action","reference":self.identity}),
      json!({"kind":"secret_profile","alias":"isolated-delivery-credential","reference":self.target}),
    ]
  }
  pub(super) fn extend(
    &self,
    draft: &mut Value,
    coder: &str,
    verification: &str,
    previous: (&str, &str),
    name: &str,
    budget: BudgetLimit,
  ) {
    let mut node = workflow::node(
      name,
      "trusted_action",
      &self.receipt,
      budget,
      &self.permissions,
      "published",
    );
    node["input_schema"] = json!(self.input.reference());
    node["action"] = json!({"plugin":self.identity,"action":"publish_candidate","parameters":{
      "bundle":"candidate_bundle","manifest":"candidate_manifest","target":self.target}});
    node["input_binding"] = json!({"sources":{
      "candidate":{"source":"record","node":coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/data/payload/value",models::CANDIDATE_FACTS)},
      "bundle":{"source":"record","node":coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT)},
      "manifest":{"source":"record","node":coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT)},
      "verification":{"source":"record","node":verification,"outcome":"verified","view":"payload","path":"/verified"}
    },"mapping":{"value":{"mapping":"object","fields":{
      "candidate_revision":{"mapping":"pointer","path":"/candidate/candidate_revision"},
      "candidate_bundle":{"mapping":"pointer","path":"/bundle"},"candidate_manifest":{"mapping":"pointer","path":"/manifest"},
      "verified":{"mapping":"pointer","path":"/verification"}
    }}}});
    node["context_binding"] = json!({"candidate_revision":{"value":{"mapping":"pointer","path":"/candidate_revision"}},"entries":{
      "candidate_bundle":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/candidate_bundle"}}}}},
      "candidate_manifest":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/candidate_manifest"}}}}}
    }});
    draft["root"]["nodes"].as_array_mut().unwrap().push(node);
    let prior_schema = draft["root"]["terminals"][0]["schema"].clone();
    let transitions = draft["root"]["transitions"].as_array_mut().unwrap();
    // Other declared outcomes keep their original terminal schema; publication
    // replaces only the selected success edge and terminal payload.
    let unpublished = format!("{name}-not-published");
    let mut retained_alternative = false;
    for edge in transitions.iter_mut() {
      if edge["target"]["kind"] == "terminal"
        && edge["target"]["key"] == "finished"
        && !(edge["predecessor"] == previous.0 && edge["outcome"] == previous.1)
      {
        edge["target"]["key"] = json!(unpublished);
        retained_alternative = true;
      }
    }
    let previous = transitions
      .iter_mut()
      .find(|row| row["predecessor"] == previous.0 && row["outcome"] == previous.1)
      .unwrap();
    previous["target"] = json!({"kind":"node","key":name});
    transitions.push(workflow::edge(name, "published", "terminal", "finished"));
    if retained_alternative {
      draft["root"]["terminals"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":unpublished,"schema":prior_schema}));
    }
    draft["root"]["terminals"][0]["schema"] = json!(self.receipt.reference());
    draft["data_schemas"]
      .as_array_mut()
      .unwrap()
      .extend([json!(self.input), json!(self.receipt)]);
  }
  pub(super) fn verify(&self, rows: &[(Vec<u8>, Vec<u8>)], name: &str, coder: &str, base: &str) {
    let records = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .collect::<Vec<_>>();
    let record = records.iter().find(|row| row["input"]["node"] == name).unwrap();
    let coder = records.iter().find(|row| row["input"]["node"] == coder).unwrap();
    let facts = &coder["build"]["outputs"]
      .as_array()
      .unwrap()
      .iter()
      .find(|row| row["name"] == models::CANDIDATE_FACTS)
      .unwrap()["data"]["payload"]["value"];
    let receipt = &record["payload"]["value"];
    assert_eq!(receipt["candidate_revision"], facts["candidate_revision"]);
    assert_eq!(receipt["changeset_digest"], facts["candidate_digest"]);
    assert_eq!(receipt["external"]["remote_head"], facts["candidate_revision"]);
    assert_eq!(receipt["external"]["review_state"], "open");
    assert_eq!(receipt["external"]["review_identity"], "1");
    assert_eq!(
      self.forge.review_posts(),
      1,
      "accepted unknown POST is observed, never repeated"
    );
    let git = required_path("OCTACITY_CONTRACT_GIT_EXECUTABLE", true);
    let read = |revision: &str| {
      let output = std::process::Command::new(&git)
        .arg("--git-dir")
        .arg(&self.bare)
        .args(["rev-parse", "--verify", revision])
        .output()
        .unwrap();
      assert!(output.status.success());
      String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(read("refs/heads/main"), base);
    let branch = string(&receipt["external"], "branch_reference");
    assert_eq!(read(&branch), string(facts, "candidate_revision"));
    let content = std::process::Command::new(&git)
      .arg("--git-dir")
      .arg(&self.bare)
      .args(["show", &format!("{branch}:public/facts.txt")])
      .output()
      .unwrap();
    assert!(content.status.success());
    assert_eq!(content.stdout, b"facts");
    let public = serde_json::to_string(record).unwrap();
    for canary in [
      "private-delivery-token",
      "operator-delivery-handle",
      "finite-coder-private-canary",
      "contract.rs",
    ] {
      assert!(!public.contains(canary));
    }
  }
  pub(super) async fn stop(self) {
    self.forge.stop().await;
  }
}
