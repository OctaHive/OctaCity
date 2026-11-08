//! Isolated OpenSpec authoring whose captured revision is the implementation's parent.
use super::*;
use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT, FACTORY_SOURCE_ROOT};
use octacity_server_application::{FactoryCodexCapturePolicy, FactoryCodexUsagePolicy};
use octacity_server_factory::*;

pub(super) const AUTHOR: &str = "specification_author";
const PATH: &str = "openspec/changes/qualify-public-facts";
pub(super) const DOCUMENTS: [(&str, &str); 4] = [
  ("proposal.md", "# Proposal\nExpose the corrected public facts output.\n"),
  (
    "design.md",
    "# Design\nKeep the existing public facts file and update its content.\n",
  ),
  (
    "tasks.md",
    "# Tasks\n- [ ] Verify and update the public facts output.\n",
  ),
  (
    "specs/public-facts/spec.md",
    "## MODIFIED Requirements\n### Requirement: Public facts\nThe public facts file SHALL contain exactly facts.\n\n#### Scenario: Read the public facts\n- **WHEN** the public facts file is read\n- **THEN** its content is facts\n",
  ),
];

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn string_schema(min_bytes: usize, max_bytes: usize) -> FlowValueSchema {
  FlowValueSchema::String { min_bytes, max_bytes }
}

fn object(fields: impl IntoIterator<Item = (&'static str, FlowValueSchema)>) -> FlowValueSchema {
  FlowValueSchema::Object {
    fields: fields
      .into_iter()
      .map(|(name, shape)| (key(name), FlowFieldSchema::required(shape)))
      .collect(),
  }
}

pub(super) struct Specification {
  model: models::InstalledModel,
}

impl Specification {
  pub(super) fn new(coder: &models::InstalledModel) -> Self {
    let mut profile = coder.profile.clone();
    profile.candidate_capture_tool = None;
    profile.instructions = FactoryText::new(json!({
      "fixture_version":1,
      "files":DOCUMENTS.map(|(file, text)| json!({"path":format!("{FACTORY_SOURCE_ROOT}/{PATH}/{file}"),"text":text})),
      "result":{"value":"Public facts requirements authored"}
    }).to_string()).unwrap();
    profile.toolchain.model = ImmutableReference::new(
      key("finite-specification-author"),
      key("v1"),
      FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
    );
    profile.usage_policy = FactoryCodexUsagePolicy::TokenRates {
      model: profile.toolchain.model.clone(),
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    let capture = profile.capture.as_ref().unwrap();
    let mut instruction = capture.instruction().clone();
    instruction.policy.allowed_path_prefixes = vec![PATH.into()];
    instruction.policy.max_changed_paths = DOCUMENTS.len() as u32;
    instruction.policy.max_total_bytes = instruction.policy.max_file_bytes * DOCUMENTS.len() as u64;
    profile.capture = Some(FactoryCodexCapturePolicy::new(capture.tool().clone(), instruction).unwrap());
    Self {
      model: models::InstalledModel {
        profile,
        executable: coder.executable.clone(),
        platform: coder.platform.clone(),
        capture_executable: coder.capture_executable.clone(),
      },
    }
  }

  pub(super) fn install(&self, catalog: &mut Value) {
    catalog["codex"].as_array_mut().unwrap().push(self.model.coder_entry());
    catalog["projects"][0]["references"].as_array_mut().unwrap().push(
      json!({"kind":"node_capability","alias":"specification-model","reference":self.model.profile.toolchain.model}),
    );
  }

  pub(super) fn extend(&self, draft: &mut Value, coder: &str, handoff: &FlowDataSchema) {
    let profile = &self.model.profile;
    let capture = profile.capture.as_ref().unwrap();
    let facts = models::candidate_schema();
    let summary = FlowDataSchema::new(key("pilot.specification-summary"), key("v1"), string_schema(1, 64)).unwrap();
    let artifacts = [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT].map(|name| {
      EvidenceRequirement::new(
        key(name),
        EvidenceOutputKind::Artifact,
        facts.reference().clone(),
        capture.tool().clone(),
        profile.toolchain.plugin.clone(),
      )
    });
    let mut author = workflow::node(
      AUTHOR,
      "reasoning",
      &summary,
      BudgetLimit::new(1, 120_000, 100, 100, 8 * 1024 * 1024).unwrap(),
      &profile.project_permissions,
      "authored",
    );
    let triage = &draft["root"]["input_schema"];
    author["input_schema"] = triage.clone();
    author["input_binding"] = json!({"sources":{"triage":{"source":"incoming","schema":triage,"path":""}},
      "mapping":{"value":{"mapping":"pointer","path":"/triage"}}});
    author["context_binding"] = json!({"work_task":"task","entries":{}});
    author["build"] = json!(FlowBuildBinding {
      execution_permissions: None,
      tool: profile.toolchain.executable.clone(),
      plugin: profile.toolchain.plugin.clone(),
      result_tool: None,
      capture_write_paths: Some(vec![FactoryRepositoryPath::new(PATH).unwrap()]),
      capture_deliverables: None,
      model_or_tool: profile.toolchain.model.clone(),
      task_digest: profile.task_digest(),
      build_configuration: profile.build_configuration.clone(),
      result_output: key("codex-run-result"),
      result_output_kind: EvidenceOutputKind::Report,
      result_outcome: key("authored"),
      evidence: artifacts
        .into_iter()
        .chain([EvidenceRequirement::new(
          key(models::CANDIDATE_FACTS),
          EvidenceOutputKind::Report,
          facts.reference().clone(),
          capture.tool().clone(),
          profile.toolchain.plugin.clone()
        )])
        .collect(),
    });
    let artifact = object([
      ("artifact_id", string_schema(36, 36)),
      ("content_digest", string_schema(64, 64)),
      (
        "encoded_size",
        FlowValueSchema::Integer {
          minimum: 1,
          maximum: 64 * 1024 * 1024,
        },
      ),
    ]);
    let input = FlowDataSchema::new(
      key("pilot.specification-implementation"),
      key("v1"),
      object([
        ("handoff", handoff.shape().clone()),
        ("revision", string_schema(40, 64)),
        ("bundle", artifact.clone()),
        ("manifest", artifact),
      ]),
    )
    .unwrap();
    let nodes = draft["root"]["nodes"].as_array_mut().unwrap();
    let implementation = nodes.iter_mut().find(|node| node["key"] == coder).unwrap();
    let source =
      |path: String| json!({"source":"record","node":AUTHOR,"outcome":"authored","view":"metadata","path":path});
    let pointer = |path: &str| json!({"mapping":"pointer","path":path});
    let binding = &mut implementation["input_binding"];
    binding["sources"]["revision"] = source(format!(
      "/build/outputs/{}/data/payload/value/candidate_revision",
      models::CANDIDATE_FACTS
    ));
    binding["sources"]["bundle"] = source(format!("/build/outputs/{CHANGE_SET_BUNDLE_OUTPUT}/artifact"));
    binding["sources"]["manifest"] = source(format!("/build/outputs/{CHANGE_SET_MANIFEST_OUTPUT}/artifact"));
    binding["mapping"]["value"] = json!({"mapping":"object","fields":{
      "handoff":pointer("/handoff"),"revision":pointer("/revision"),"bundle":pointer("/bundle"),"manifest":pointer("/manifest")
    }});
    implementation["input_schema"] = json!(input.reference());
    let context = &mut implementation["context_binding"];
    context["candidate_revision"] = json!({"value":pointer("/revision")});
    for (name, path) in [("bundle", "/bundle"), ("manifest", "/manifest")] {
      context["entries"][name] = json!({"source_kind":"evidence","source":{"value":{"mapping":"object","fields":{
        "kind":{"mapping":"constant","value":"artifact"},"value":pointer(path)
      }}}});
    }
    nodes.push(author);
    draft["root"]["entry"] = json!(AUTHOR);
    draft["root"]["transitions"]
      .as_array_mut()
      .unwrap()
      .push(workflow::edge(
        AUTHOR,
        "authored",
        "node",
        workflow_ticket::REQUIREMENTS_POLICY,
      ));
    draft["data_schemas"]
      .as_array_mut()
      .unwrap()
      .extend([json!(summary), json!(input)]);
  }

  pub(super) async fn verify(
    &self,
    client: &Client,
    origin: &str,
    rows: &[(Vec<u8>, Vec<u8>)],
    original_revision: &str,
  ) -> String {
    let record = rows
      .iter()
      .map(|(_, bytes)| serde_json::from_slice::<Value>(bytes).unwrap())
      .find(|row| row["input"]["node"] == AUTHOR)
      .unwrap();
    let manifest = record["build"]["outputs"]
      .as_array()
      .unwrap()
      .iter()
      .find(|output| output["name"] == CHANGE_SET_MANIFEST_OUTPUT)
      .unwrap();
    let bytes = workflow_author::download_artifact(client, origin, &string(&manifest["artifact"], "artifact_id")).await;
    let manifest: octacity_protocol::CapturedChangeSetManifestV1 = serde_json::from_slice(&bytes).unwrap();
    manifest.validate().unwrap();
    assert_eq!(manifest.base_revision, original_revision);
    assert_eq!(manifest.changed_paths.len(), DOCUMENTS.len());
    for (name, text) in DOCUMENTS {
      let change = manifest
        .changed_paths
        .iter()
        .find(|change| change.path == document_path(name))
        .unwrap();
      assert_eq!(
        change.content_sha256.as_deref(),
        Some(FactoryDigest::content_sha256(text.as_bytes()).to_string().as_str())
      );
    }
    manifest.candidate_revision
  }
}

pub(super) fn document_path(file: &str) -> String {
  format!("{PATH}/{file}")
}

pub(super) fn inherit_specification(model: &mut models::InstalledModel) {
  let profile = &mut model.profile;
  let capture = profile.capture.as_ref().unwrap();
  let mut instruction = capture.instruction().clone();
  // Inherited OpenSpec files count toward the cumulative capture ceiling.
  // The implementation's writable paths still come from its own profile.
  instruction.policy.max_changed_paths += DOCUMENTS.len() as u32;
  instruction.policy.max_total_bytes += DOCUMENTS.iter().map(|(_, text)| text.len() as u64).sum::<u64>();
  profile.candidate_capture_tool = Some(capture.tool().clone());
  profile.capture = Some(FactoryCodexCapturePolicy::new(capture.tool().clone(), instruction).unwrap());
}
