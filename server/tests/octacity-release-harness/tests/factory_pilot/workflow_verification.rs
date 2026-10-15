//! Separate ordinary placement and readonly candidate authority for a nested verifier.
use super::*;
use octacity_server_application::FactoryCodexUsagePolicy;
use octacity_server_domain::BuildConfigurationVersion;
use octacity_server_factory::*;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

pub(super) struct VerifierResources {
  pub(super) pool: String,
  pub(super) configuration: BuildConfigurationRef,
}
impl VerifierResources {
  pub(super) async fn create(
    client: &Client,
    origin: &str,
    run: &str,
    resources: &resources::PilotResources,
    backend: &ReleaseBackend,
  ) -> Self {
    let pool = resource_id(&post_management(client, origin, "/api/v1/agent-pools", &format!("{run}-verifier-pool"), json!({
      "name":format!("verifier-{run}"),"definition":{"enabled":true,"drain_state":"accepting","admission_policy":backend.pool_admission_policy(),"concurrency_limit":1,"fairness_policy":"priority_fifo","static_capacity_limit":1}
    })).await);
    let ordinary = get_json(
      client,
      format!(
        "{origin}/api/v1/build-configurations/{}/versions/1",
        resources.configuration.id()
      ),
    )
    .await;
    let mut policy = support::project_policy_body(
      &resources.pool,
      &resources.repository,
      backend.runtime_class(),
      backend.execution_target(),
    );
    policy["policy"]["pools"]["value"] = json!([resources.pool, pool]);
    policy["policy"]["artifacts"]["value"] = ordinary["definition"]["artifacts"].clone();
    policy["policy"]["concurrency"]["value"] = json!({"active_builds":4,"active_jobs":4});
    let policy = client
      .post(format!(
        "{origin}/api/v1/projects/{}/policy-versions",
        resources.project
      ))
      .header("idempotency-key", format!("{run}-verifier-policy"))
      .header("if-match", "\"1\"")
      .json(&policy)
      .send()
      .await
      .unwrap();
    assert!(
      policy.status().is_success(),
      "fixture policy update failed with {}",
      policy.status()
    );
    let policy: Value = policy.json().await.unwrap();
    assert_eq!(policy["resource"]["version"], 2);
    let mut definition = ordinary["definition"].clone();
    definition["allowed_pools"] = json!([pool]);
    let configuration = post_management(
      client,
      origin,
      "/api/v1/build-configurations",
      &format!("{run}-verifier-configuration"),
      json!({"project_id":resources.project,"name":format!("verifier-{run}"),"definition":definition}),
    )
    .await;
    let id = resource_id(&configuration);
    let published = get_json(client, format!("{origin}/api/v1/build-configurations/{id}/versions/1")).await;
    assert_eq!(published, configuration["resource"]);
    assert_eq!(published["definition"]["allowed_pools"], json!([pool]));
    let digest = FactoryDigest::content_sha256(&serde_json::to_vec(&published["definition"]).unwrap());
    Self {
      pool,
      configuration: BuildConfigurationRef::new(
        id.parse().unwrap(),
        BuildConfigurationVersion::INITIAL,
        resources.project.parse().unwrap(),
        digest,
      ),
    }
  }
}

pub(super) struct Verifier {
  pub(super) model: models::InstalledModel,
  pub(super) input: FlowDataSchema,
  pub(super) report: FlowDataSchema,
}
impl Verifier {
  pub(super) fn new(
    directory: &Path,
    release: &octacity_release_harness::InstalledRelease,
    resources: &resources::PilotResources,
    isolated: &VerifierResources,
    backend: &ReleaseBackend,
    coder: &models::InstalledModel,
  ) -> Self {
    let installation = directory.join("isolated-verifier-model");
    fs::create_dir(&installation).unwrap();
    set_mode(&installation, 0o700);
    let mut model = models::InstalledModel::observer(&installation, release, resources, backend);
    fs::copy(required_path("OCTACITY_FACTORY_VERIFIER_CLI", true), &model.executable).unwrap();
    set_mode(&model.executable, 0o700);
    let profile = &mut model.profile;
    profile.build_configuration = isolated.configuration.clone();
    profile.toolchain.executable = ImmutableReference::new(
      profile.toolchain.executable.identity().clone(),
      profile.toolchain.executable.version().clone(),
      FactoryDigest::from_lower_hex(&hex::encode(Sha256::digest(fs::read(&model.executable).unwrap()))).unwrap(),
    );
    profile.instructions = FactoryText::new(json!({"fixture_version":1,
      "checks":[{"path":models::CANDIDATE_FILE,"readable":true,"expected_text":"facts"},
        {"path":"/octacity/protected/test-inventory.json","readable":false},
        {"path":"/octacity/protected/tests/contract.rs","readable":false},
        {"path":"/workspace/output/private-contract.rs","readable":false}],
      "result":{"verified":true,"candidate_revision":{"$input":"/payload/value/candidate_revision"},"candidate_digest":{"$input":"/payload/value/candidate_digest"},"candidate_input_digest":{"$input":"/payload/value/candidate_input_digest"}}
    }).to_string()).unwrap();
    profile.credentials = FactoryCredentialProfiles::new(
      key("unselected-author"),
      key("isolated-verifier-credential"),
      key("unselected-source"),
      key("unselected-delivery"),
    )
    .unwrap();
    profile.toolchain.model = ImmutableReference::new(
      profile.toolchain.model.identity().clone(),
      profile.toolchain.model.version().clone(),
      FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
    );
    profile.usage_policy = FactoryCodexUsagePolicy::TokenRates {
      model: profile.toolchain.model.clone(),
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    profile.candidate_capture_tool = Some(coder.profile.capture.as_ref().unwrap().tool().clone());
    let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
    grants["executables"] = json!([profile.toolchain.executable]);
    for command in grants["commands"].as_array_mut().unwrap() {
      command["executable"] = json!(profile.toolchain.executable);
    }
    grants["secret_profiles"] = json!([profile.credentials.profile(FactoryCredentialPurpose::Evaluator)]);
    grants["mounts"]
      .as_array_mut()
      .unwrap()
      .push(json!({"root":octacity_protocol::FACTORY_SOURCE_ROOT,"mode":"read_only"}));
    profile.project_permissions = serde_json::from_value(grants).unwrap();
    profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
    let text = || FlowValueSchema::String {
      min_bytes: 64,
      max_bytes: 64,
    };
    let report = FlowDataSchema::new(
      key("isolated-candidate-verification"),
      key("v1"),
      FlowValueSchema::Object {
        fields: BTreeMap::from([
          (key("verified"), FlowFieldSchema::required(FlowValueSchema::Boolean)),
          (
            key("candidate_revision"),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 40,
              max_bytes: 64,
            }),
          ),
          (key("candidate_digest"), FlowFieldSchema::required(text())),
          (key("candidate_input_digest"), FlowFieldSchema::required(text())),
        ]),
      },
    )
    .unwrap();
    let artifact = FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("artifact_id"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 36,
            max_bytes: 36,
          }),
        ),
        (key("content_digest"), FlowFieldSchema::required(text())),
        (
          key("encoded_size"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 64 * 1024 * 1024,
          }),
        ),
      ]),
    };
    let mut fields = match report.shape() {
      FlowValueSchema::Object { fields } => fields.clone(),
      _ => unreachable!(),
    };
    fields.remove(&key("verified"));
    fields.insert(key("candidate_bundle"), FlowFieldSchema::required(artifact.clone()));
    fields.insert(key("candidate_manifest"), FlowFieldSchema::required(artifact));
    let input = FlowDataSchema::new(
      key("isolated-candidate-input"),
      key("v1"),
      FlowValueSchema::Object { fields },
    )
    .unwrap();
    Self { model, input, report }
  }
}

pub(super) struct VerificationNodes<'a> {
  pub(super) coder: &'a str,
  pub(super) previous: &'a str,
  pub(super) call: &'a str,
  pub(super) leaf: &'a str,
}
fn context() -> Value {
  json!({"candidate_revision":{"value":{"mapping":"pointer","path":"/candidate_revision"}},"entries":{
    "candidate_bundle":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/candidate_bundle"}}}}},
    "candidate_manifest":{"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{"kind":{"mapping":"constant","value":"artifact"},"value":{"mapping":"pointer","path":"/candidate_manifest"}}}}}
  }})
}
impl Verifier {
  pub(super) fn entry(&self) -> Value {
    let p = &self.model.profile;
    json!({"toolchain":{"plugin":p.toolchain.plugin,"executable":p.toolchain.executable,"model":p.toolchain.model},"build_configuration":p.build_configuration,"instructions":p.instructions,"credentials":p.credentials,"credential_purpose":"evaluator","usage_policy":p.usage_policy,"candidate_capture_tool":p.candidate_capture_tool,"pipeline_node":p.pipeline_node,"parameters":p.parameters,"project_permissions":p.project_permissions,"local_permissions":{"grants":p.local_permissions.grants(),"enforceable":PermissionCategory::ALL}})
  }
  pub(super) async fn extend(
    &self,
    client: &Client,
    origin: &str,
    project: &str,
    draft: &mut Value,
    nodes: VerificationNodes<'_>,
    budget: BudgetLimit,
  ) {
    let p = &self.model.profile;
    let mut leaf = workflow::node(
      nodes.leaf,
      "reasoning",
      &self.report,
      budget,
      &p.project_permissions,
      "checked",
    );
    leaf["input_schema"] = json!(self.input.reference());
    leaf["input_binding"] = json!({"sources":{"caller":{"source":"caller_input","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/caller"}}});
    leaf["context_binding"] = context();
    leaf["build"] = json!(FlowBuildBinding {
      tool: p.toolchain.executable.clone(),
      plugin: p.toolchain.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: p.toolchain.model.clone(),
      task_digest: p.task_digest(),
      build_configuration: p.build_configuration.clone(),
      result_output: key("codex-run-result"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("checked"),
      evidence: vec![]
    });
    let mut child = workflow::draft(
      nodes.leaf,
      vec![leaf],
      vec![workflow::edge(nodes.leaf, "checked", "terminal", "verified")],
      "verified",
      &self.input,
      budget,
      &p.project_permissions,
    );
    child["root"]["terminals"][0]["schema"] = json!(self.report.reference());
    child["data_schemas"].as_array_mut().unwrap().push(json!(self.report));
    let _: octacity_server_application::FactoryFlowDefinitionDraft =
      serde_json::from_value(child.clone()).expect("typed verifier child draft");
    let child = post_management(
      client,
      origin,
      "/api/v1/flow-definitions",
      &format!("verifier-child-{}", Uuid::new_v4()),
      json!({"id":Uuid::new_v4(),"project_id":project,"definition":child}),
    )
    .await;
    let mut call = workflow::node(
      nodes.call,
      "subflow_call",
      &self.report,
      budget,
      &p.project_permissions,
      "verified",
    );
    call["input_schema"] = json!(self.input.reference());
    call["subflow"] = child["resource"]["flow"]["closure"]["root"].clone();
    call["input_binding"] = json!({"sources":{
      "candidate":{"source":"record","node":nodes.coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/data/payload/value",models::CANDIDATE_FACTS)},
      "bundle":{"source":"record","node":nodes.coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT)},
      "manifest":{"source":"record","node":nodes.coder,"outcome":"implemented","view":"metadata","path":format!("/build/outputs/{}/artifact",octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT)}
    },"mapping":{"value":{"mapping":"object","fields":{
      "candidate_revision":{"mapping":"pointer","path":"/candidate/candidate_revision"},
      "candidate_digest":{"mapping":"pointer","path":"/candidate/candidate_digest"},
      "candidate_input_digest":{"mapping":"pointer","path":"/candidate/input_digest"},
      "candidate_bundle":{"mapping":"pointer","path":"/bundle"},
      "candidate_manifest":{"mapping":"pointer","path":"/manifest"}
    }}}});
    draft["root"]["nodes"].as_array_mut().unwrap().push(call);
    let edges = draft["root"]["transitions"].as_array_mut().unwrap();
    let previous = edges
      .iter_mut()
      .find(|row| row["predecessor"] == nodes.previous)
      .unwrap();
    *previous = workflow::edge(nodes.previous, "validated", "node", nodes.call);
    edges.push(workflow::edge(nodes.call, "verified", "terminal", "finished"));
    draft["root"]["terminals"][0]["schema"] = json!(self.report.reference());
    draft["data_schemas"]
      .as_array_mut()
      .unwrap()
      .extend([json!(self.input), json!(self.report)]);
  }
}

pub(super) struct VerifierProcess {
  pub(super) config: PathBuf,
  pub(super) worker: Child,
}
impl Verifier {
  pub(super) async fn start(
    &self,
    s: &workflow_author::AuthorScenario<'_>,
    isolated: &VerifierResources,
    client: &Client,
    origin: &str,
  ) -> VerifierProcess {
    let p = &self.model.profile;
    let installation = s.directory.join("isolated-verifier-agent");
    fs::create_dir(&installation).unwrap();
    set_mode(&installation, 0o700);
    let credential = private_file(
      &installation,
      "evaluator.json",
      &json!({"OCTACITY_CODEX_AUTH":"isolated-verifier-credential-canary"}).to_string(),
    );
    let credentials = BTreeMap::from([(
      p.credentials.profile(FactoryCredentialPurpose::Evaluator).to_string(),
      credential,
    )]);
    let grants = octacity_server_store::factory_job_permissions(&p.project_permissions).unwrap();
    let coder = s.coder.unwrap();
    let tool = coder.profile.capture.as_ref().unwrap().tool();
    let capture_platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let capture = agent::AgentToolExecutable {
      product: "git",
      path: coder.capture_executable.as_ref().unwrap(),
      version: tool.version().as_str(),
      platform: &capture_platform,
      sha256: &tool.digest().to_string(),
    };
    let executable = agent::AgentToolExecutable {
      product: "codex-cli",
      path: &self.model.executable,
      version: "0.161.0",
      platform: &self.model.platform,
      sha256: &p.toolchain.executable.digest().to_string(),
    };
    let executables = [executable];
    let object_origin = required_string("OCTACITY_MINIO_ENDPOINT");
    let origins = [object_origin.as_str()];
    let mut overrides = agent::AgentConfigOverrides::restricted_without_outputs(&origins);
    overrides.factory = Some(agent::AgentFactoryOverrides {
      permissions: &grants,
      credential_profiles: &credentials,
      capture: Some(&capture),
      protected_reader: None,
    });
    overrides.tool_executables = &executables;
    overrides.output_limits = Some(octacity_protocol::OutputLimits {
      artifact_count: 16,
      artifact_bytes: 4 * 1024 * 1024,
      report_count: 16,
      report_bytes: 4 * 1024 * 1024,
      single_output_bytes: 4 * 1024 * 1024,
    });
    let identity = Uuid::new_v4().simple().to_string();
    let enrollment = resources::issue_enrollment(client, origin, &identity, &isolated.pool, s.backend).await;
    let config = agent::write_agent_config(
      &installation,
      &format!("http://{}", s.agent_addr),
      &format!("verifier-{identity}"),
      &enrollment,
      agent::AgentReleasePaths {
        octa_root: &s.release.octa_root,
        source_plugins: &s.directory.join("source-mirror/source-plugins"),
      },
      s.backend,
      &overrides,
    );
    let validation = Command::new(&s.release.agent_binary)
      .arg("validate")
      .arg(&config)
      .output()
      .await
      .unwrap();
    fs::write(installation.join("validate.stdout.log"), validation.stdout).unwrap();
    fs::write(installation.join("validate.stderr.log"), validation.stderr).unwrap();
    assert!(
      validation.status.success(),
      "isolated verifier installation invalid; private evidence: {}",
      s.directory.display()
    );
    let worker = agent::spawn_agent(
      &s.release.agent_binary,
      &config,
      &installation.join("stdout.log"),
      &installation.join("stderr.log"),
    );
    VerifierProcess { config, worker }
  }
  pub(super) async fn verify(
    &self,
    client: &Client,
    origin: &str,
    isolated: &VerifierResources,
    original_pool: &str,
    rows: &[(Vec<u8>, Vec<u8>)],
    nodes: VerificationNodes<'_>,
  ) {
    let records = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .collect::<Vec<_>>();
    let selected = |name: &str| records.iter().find(|row| row["input"]["node"] == name).unwrap();
    let coder = selected(nodes.coder);
    let leaf = selected(nodes.leaf);
    let call = selected(nodes.call);
    assert_eq!(
      leaf["payload"], call["payload"],
      "nested return is the exact verified child payload"
    );
    assert!(
      call["subflow"].is_object(),
      "the return retains its exact child record reference"
    );
    assert_ne!(leaf["input"]["flow_run_id"], coder["input"]["flow_run_id"]);
    assert!(leaf["input"]["caller_input_digest"].is_string());
    assert_eq!(leaf["input"]["context"]["entries"].as_array().unwrap().len(), 2);
    let capture = coder["build"]["outputs"]
      .as_array()
      .unwrap()
      .iter()
      .find(|v| v["name"] == models::CANDIDATE_FACTS)
      .unwrap();
    let facts = &capture["data"]["payload"]["value"];
    assert_eq!(
      leaf["payload"]["value"],
      json!({"verified":true,"candidate_revision":facts["candidate_revision"],"candidate_digest":facts["candidate_digest"],"candidate_input_digest":facts["input_digest"]})
    );
    let mut assignments = Vec::new();
    let attempts = workflow::diagnostic(
      client,
      origin,
      leaf["input"]["factory_run_id"].as_str().unwrap(),
      "node-attempts",
    )
    .await;
    for (record, pool) in [(coder, original_pool), (leaf, isolated.pool.as_str())] {
      let producer = &record["build"]["outputs"][0]["producer"];
      let build = get_json(
        client,
        format!("{origin}/api/v1/builds/{}", string(producer, "build_id")),
      )
      .await;
      let node = attempts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["record"]["id"] == record["node_attempt_id"])
        .unwrap();
      assert_eq!(build["factory_origin"]["input_digest"], node["record"]["input_digest"]);
      let attempt = get_json(
        client,
        format!("{origin}/api/v1/attempts/{}", string(producer, "attempt_id")),
      )
      .await;
      let job = &attempt["jobs"][0];
      assert_eq!(job["state"], "succeeded");
      assert_eq!(job["allowed_pool_ids"], json!([pool]));
      assert_eq!(job["assignment"]["selected_pool_id"], pool);
      assignments.push(job["assignment"]["assigned_agent_id"].clone());
    }
    assert_ne!(
      assignments[0], assignments[1],
      "independent verification must use a different enrolled Agent"
    );
    let retained = leaf.to_string();
    for canary in [
      "isolated-verifier-credential-canary",
      "finite-coder-private-canary",
      "PRIVATE_EXPECTATION",
      "contract.rs",
    ] {
      assert!(!retained.contains(canary));
    }
  }
}
