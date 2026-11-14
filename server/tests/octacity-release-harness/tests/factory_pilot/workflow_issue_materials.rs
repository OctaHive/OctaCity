//! Packaged Console/native qualification of owned, frozen and scoped Issue materials.
use super::*;
use octacity_server_factory::*;

#[cfg(test)]
#[path = "workflow_issue_materials_tests.rs"]
mod tests;

const SELECTED: &str = "material-observation";
const SCOPED: &str = "component-observation";
const TEXTS: [&str; 3] = [
  "ISSUE_TEXT_SELECTED_v1",
  "PROJECT_TEXT_SELECTED_v1",
  "COMMENT_TEXT_SELECTED_v1",
];

pub(super) fn configure_model(model: &mut models::InstalledModel) {
  // This manual material case uses the existing observer only for its two probes.
  let profile = &mut model.profile;
  let mut program: Value = serde_json::from_str(profile.instructions.as_str()).unwrap();
  program["data_checks"] = json!(TEXTS.map(|text| json!({"text":text,"present":{"$input":"/payload/value/selected"}})));
  program["data_checks"]
    .as_array_mut()
    .unwrap()
    .push(json!({"text":"ISSUE_TEXT_UNSELECTED_v2","present":false}));
  profile.instructions = FactoryText::new(program.to_string()).unwrap();
  profile.toolchain.model = ImmutableReference::new(
    profile.toolchain.model.identity().clone(),
    profile.toolchain.model.version().clone(),
    FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
  );
  match &mut profile.usage_policy {
    octacity_server_application::FactoryCodexUsagePolicy::TokenRates { model, .. } => {
      *model = profile.toolchain.model.clone()
    }
    _ => panic!("the read-only fixture has a pinned token-rate model"),
  }
}

pub(super) fn schema(base: &FlowDataSchema) -> FlowDataSchema {
  let FlowValueSchema::Object { mut fields } = base.shape().clone() else {
    panic!("canonical field object")
  };
  for field in fields.values_mut() {
    *field = FlowFieldSchema::required(field.value().clone());
  }
  fields.insert(
    "environment".parse().unwrap(),
    FlowFieldSchema::required(FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: 128,
    }),
  );
  fields.insert(
    "context".parse().unwrap(),
    FlowFieldSchema::required(FlowValueSchema::Array {
      item: Box::new(FlowValueSchema::String {
        min_bytes: 0,
        max_bytes: 4096,
      }),
      min_items: 1,
      max_items: 3,
    }),
  );
  FlowDataSchema::new(
    "pilot.issue-materials".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::Object { fields },
  )
  .unwrap()
}

pub(super) async fn extend(
  client: &Client,
  origin: &str,
  project: &str,
  draft: &mut Value,
  schema: &FlowDataSchema,
  model: &models::InstalledModel,
) {
  let catalog = get_json(
    client,
    format!("{origin}/api/v1/projects/{project}/issue-property-definitions"),
  )
  .await;
  let environment = catalog["definition"]["properties"]
    .as_array()
    .unwrap()
    .iter()
    .find(|field| field["name"] == "Material environment")
    .unwrap();
  draft["issue_snapshot"]["fields"]["environment"] =
    json!({"source":{"kind":"property","id":environment["id"]},"required":true});
  draft["issue_snapshot"]["fields"]["severity"] =
    json!({"source":{"kind":"classification","slot":"severity"},"required":true});
  draft["issue_snapshot"]["materials"] =
    json!({"context":{"source":{"kind":"selected","max_entries":3},"required":true,"max_bytes":4096}});
  let FlowValueSchema::Object { fields } = schema.shape() else {
    unreachable!()
  };
  let mut full = fields.clone();
  full.insert(
    "selected".parse().unwrap(),
    FlowFieldSchema::required(FlowValueSchema::Boolean),
  );
  let full = FlowDataSchema::new(
    "pilot.material-observation".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::Object { fields: full },
  )
  .unwrap();
  let sparse = FlowDataSchema::new(
    "pilot.component-observation".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::Object {
      fields: [
        (
          "component".parse().unwrap(),
          fields[&"component".parse::<FactoryKey>().unwrap()].clone(),
        ),
        (
          "selected".parse().unwrap(),
          FlowFieldSchema::required(FlowValueSchema::Boolean),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  let p = &model.profile;
  // Reuse the observer's allowance: compiler-owned outputs also reserve space before Job publication.
  let budget = BudgetLimit::new(1, 120_000, 100, 100, 8 * 1024 * 1024).unwrap();
  let previous = draft["root"]["entry"].clone();
  for (name, output, selected) in [(SELECTED, &full, true), (SCOPED, &sparse, false)] {
    let mut node = workflow::node(name, "reasoning", output, budget, &p.project_permissions, "observed");
    let FlowValueSchema::Object { fields: projected } = output.shape() else {
      unreachable!()
    };
    let mapping: serde_json::Map<_, _> = projected
      .keys()
      .map(|key| {
        (
          key.to_string(),
          if key.as_str() == "selected" {
            json!({"mapping":"constant","value":selected})
          } else {
            json!({"mapping":"pointer","path":format!("/issue/{key}")})
          },
        )
      })
      .collect();
    node["input_binding"] = json!({"sources":{"issue":{"source":"incoming","schema":schema.reference(),"path":""}},"mapping":{"value":{"mapping":"object","fields":mapping}}});
    node["build"] = json!(FlowBuildBinding {
      execution_permissions: None,
      tool: p.toolchain.executable.clone(),
      plugin: p.toolchain.plugin.clone(),
      result_tool: None,
      capture_write_paths: None,
      capture_deliverables: None,
      model_or_tool: p.toolchain.model.clone(),
      task_digest: p.task_digest(),
      build_configuration: p.build_configuration.clone(),
      result_output: "codex-run-result".parse().unwrap(),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: "observed".parse().unwrap(),
      evidence: vec![],
    });
    node["context_binding"] = json!({"work_task":"task","entries":{}});
    draft["root"]["nodes"].as_array_mut().unwrap().push(node);
  }
  draft["root"]["entry"] = json!(SELECTED);
  draft["root"]["input_schema"] = json!(full.reference());
  draft["root"]["transitions"].as_array_mut().unwrap().extend([
    workflow::edge(SELECTED, "observed", "node", SCOPED),
    workflow::edge(SCOPED, "observed", "node", previous.as_str().unwrap()),
  ]);
  draft["limits"]["max_expanded_nodes"] = json!(64);
  draft["data_schemas"]
    .as_array_mut()
    .unwrap()
    .extend([json!(full), json!(sparse)]);
}

pub(super) async fn verify_probes(client: &Client, origin: &str, run: &str) -> Value {
  let attempts = workflow::diagnostic(client, origin, run, "node-attempts").await;
  let builds = workflow::diagnostic(client, origin, run, "builds").await;
  let mut proof = vec![];
  for name in [SELECTED, SCOPED] {
    let attempt = attempts["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|row| &row["record"])
      .find(|row| row["node_key"] == name)
      .expect("a declared material probe executed");
    let build = builds["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|row| &row["record"])
      .find(|row| row["node_attempt_id"] == attempt["id"])
      .unwrap();
    let artifacts = get_json(
      client,
      format!(
        "{origin}/api/v1/builds/{}/artifacts?limit=100",
        build["build_id"].as_str().unwrap()
      ),
    )
    .await;
    let report = artifacts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|artifact| artifact["name"] == "codex-run-result")
      .unwrap();
    let bytes = workflow_author::download_artifact(client, origin, report["id"].as_str().unwrap()).await;
    let artifact = report.clone();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    let expected = if name == SELECTED {
      json!({"priority":"high","component":"api","kind":"feature_request","size":"small",
      "environment":"staging-arm64","severity":"significant","context":TEXTS,"selected":true})
    } else {
      json!({"component":"api","selected":false})
    };
    assert_eq!(
      report["structured_result"], expected,
      "ordinary Job input projection for {name}"
    );
    proof.push(json!({"node":name,"build":build,"artifact":artifact,"result":report["structured_result"]}));
  }
  json!(proof)
}

/// HTTP assertions only; callers choose whether to retain the observed documents.
pub(super) async fn verify_retained(
  client: &Client,
  origin: &str,
  path: &str,
  retained: &workflow_issue_snapshot::Retained,
  run: &Value,
) -> (Value, Value) {
  let request = get_json(client, format!("{origin}{path}/{}", retained.execution)).await;
  assert_eq!(request["association"]["run_id"], run["id"]);
  assert_eq!(request["snapshot_digest"], retained.reviewed["snapshot_digest"]);
  let detail = get_json(client, format!("{origin}{path}/{}/snapshot", retained.execution)).await;
  assert_eq!(detail["availability"], "available");
  assert_eq!(
    detail["snapshot"], retained.reviewed["snapshot"],
    "later publications, comments and restart preserve admitted bytes"
  );
  let snapshot = &detail["snapshot"];
  assert_eq!(snapshot["version"], 6);
  assert_eq!(
    snapshot["values"],
    json!({"priority":"high","component":"api","kind":"feature_request","size":"small",
    "environment":"staging-arm64","severity":"significant","context":TEXTS})
  );
  assert!(snapshot["target"].get("predecessor_run").is_none_or(Value::is_null));
  for alias in ["priority", "component", "kind", "size", "severity"] {
    assert_eq!(snapshot["attribution"][alias]["origin"]["kind"], "manual");
  }
  let issue = get_json(client, format!("{origin}{}", path.strip_suffix("/executions").unwrap())).await;
  let lineage = snapshot["materials"]["context"].as_array().unwrap();
  assert_eq!(lineage.len(), 3);
  let selected: Vec<_> = lineage.iter().map(|entry| entry["selection"].clone()).collect();
  assert_eq!(
    &selected,
    issue["materials"].as_array().unwrap(),
    "the complete current material selection survives publication and restart"
  );
  (request, detail)
}

/// Explicit native-journey output, after its ordinary Jobs have been independently checked.
pub(super) fn save_native_evidence(run: &Value, request: &Value, snapshot: &Value, probes: &Value) {
  if let Some(directory) = evidence() {
    private_file(
      &directory,
      &format!("issue-materials-{}.json", run["id"].as_str().unwrap()),
      &json!({"execution":request,"snapshot":snapshot,"run":run,"ordinary_probes":probes}).to_string(),
    );
  }
}

pub(super) async fn publish_later(client: &Client, origin: &str, project: &str, issue: &str) {
  let base = format!("/api/v1/projects/{project}");
  let path = format!("{base}/issues/{issue}");
  let before = get_json(client, format!("{origin}{path}")).await;
  for (suffix, key, body) in [
    (
      format!("{path}/input-documents"),
      "later-owned-document",
      json!({"name":"Task investigation","content":"ISSUE_TEXT_LATER_v3"}),
    ),
    (
      format!("{base}/input-documents"),
      "later-shared-document",
      json!({"name":"Shared delivery guide","content":"PROJECT_TEXT_LATER_v2"}),
    ),
    (
      format!("{path}/comments"),
      "later-operator-comment",
      json!({"body":"COMMENT_TEXT_LATER_v2"}),
    ),
  ] {
    post_management(client, origin, &suffix, key, body).await;
  }
  let after = get_json(client, format!("{origin}{path}")).await;
  assert_eq!(
    after, before,
    "publishing text never selects it or revises Issue requirements"
  );
  let documents = get_json(client, format!("{origin}{path}/input-documents")).await;
  assert_eq!(documents["items"].as_array().unwrap().len(), 3);
  let timeline = get_json(client, format!("{origin}{path}/timeline?limit=200")).await;
  assert!(timeline["next_cursor"].is_null());
  assert_eq!(
    timeline["items"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|entry| entry["kind"] == "discussion")
      .count(),
    2
  );
  let executions = get_json(client, format!("{origin}{path}/executions")).await;
  assert_eq!(executions["items"].as_array().unwrap().len(), 1);
}

pub(super) async fn browser(
  client: &Client,
  origin: &str,
  project: &str,
  directory: &Path,
  phase: &str,
  issue: &str,
  revision: &str,
) {
  let release =
    octacity_release_harness::validate_installed_browser_release(&octacity_release_harness::BrowserReleaseBundles {
      server: required_path("OCTACITY_RELEASE_SERVER_ROOT", true),
      console: required_path("OCTACITY_RELEASE_CONSOLE_ROOT", true),
    })
    .unwrap();
  let directory = directory.join(format!("issue-materials-{phase}"));
  fs::create_dir(&directory).unwrap();
  let address = unused_loopback_addresses().0;
  let stderr = directory.join("nginx.stderr.log");
  let config = browser_proxy::write_nginx_config_with_upstream(
    &directory,
    &release,
    origin.strip_prefix("http://").unwrap(),
    address,
    &directory.join("nginx.error.log"),
  );
  let mut nginx = browser_proxy::spawn_nginx(
    &required_path("OCTACITY_RELEASE_NGINX", true),
    &config,
    &directory.join("nginx.stdout.log"),
    &stderr,
  );
  let console = format!("http://{address}");
  browser_proxy::wait_for_proxy(client, &console, &mut nginx, &stderr).await;
  let result = browser_proxy::run_browser_scenario(
    &required_path("OCTACITY_RELEASE_UI_ROOT", true),
    &console,
    "issue-classification-snapshots.spec.ts",
    &[
      ("OCTACITY_RELEASE_PROJECT_ID", project),
      ("OCTACITY_RELEASE_MATERIAL_PHASE", phase),
      ("OCTACITY_RELEASE_ISSUE_ID", issue),
      ("OCTACITY_RELEASE_CODE_REVISION", revision),
    ],
    &directory,
  )
  .await;
  browser_proxy::shutdown_process(&mut nginx, &stderr, "nginx").await;
  if let Some(evidence) = evidence() {
    for stream in ["stdout", "stderr"] {
      private_file(
        &evidence,
        &format!("issue-materials-{project}-{phase}.{stream}.log"),
        &fs::read_to_string(directory.join(format!("browser.{stream}.log"))).unwrap(),
      );
    }
  }
  result.unwrap();
}

fn evidence() -> Option<PathBuf> {
  env::var_os("OCTACITY_RELEASE_EVIDENCE_DIR").map(|directory| {
    let directory = PathBuf::from(directory);
    assert!(directory.is_absolute() && directory.is_dir() && !directory.is_symlink());
    directory
  })
}
