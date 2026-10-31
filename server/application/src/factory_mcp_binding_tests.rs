//! Exact connection/credential scope comes from the frozen invocation, never tool text.
use super::*;
use crate::factory_codex_observation_tests::{Fixture, native_capture_fixture};

struct EvidenceFixture {
  role: FactoryRoleInvocation,
  compiled: CompiledCodexTask,
  configurations: Vec<FactoryCodexMcpConfiguration>,
  native: Fixture,
}
impl EvidenceFixture {
  fn capture(&self) -> FactoryCodexNativeCapture {
    let task = &self.role.package().inputs().task;
    let identity = &self.native.documents[0].identity;
    verify_codex_native_capture(
      FactoryCodexNativeCaptureContract {
        build_id: identity.build_id,
        attempt_id: identity.attempt_id,
        job_id: identity.job_id,
        provenance: FactoryCodexProvenanceContract {
          plugin: task.plugin(),
          executable: task.executable(),
          model: task.model(),
          model_identifier: None,
          prompt_digest: codex_prompt_provenance_digest(
            std::str::from_utf8(self.compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).unwrap(),
          ),
          source_revision: Some(task.subject().exact().unwrap().base_revision().as_str()),
        },
        maximum_bytes: 16 * 1024 * 1024,
        execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
      },
      &self.native.documents,
    )
    .unwrap()
  }
}
fn fixture(result_text: &str) -> EvidenceFixture {
  fixture_with_tools(result_text, vec!["lookup".into()])
}

fn fixture_with_tools(result_text: &str, enabled_tools: Vec<String>) -> EvidenceFixture {
  let secret = logical_secret("project/issue-reader");
  let configuration = FactoryMcpConfiguration::new(
    key("issues"),
    key("v1"),
    FactoryMcpServer::Http {
      url: "https://octacity.example/mcp/issues".into(),
      bearer_token_env_var: Some("ISSUE_TOKEN".into()),
      enabled_tools: Some(enabled_tools),
    },
    None,
  )
  .unwrap()
  .with_authorization(FactoryMcpAuthorization {
    credential_profile: key("mcp-read"),
    secret: secret.clone(),
    environment: "ISSUE_TOKEN".into(),
    purpose: FactoryMcpCredentialPurpose::McpRead,
    resource: reference("project-issues", 91),
  })
  .unwrap();
  let (intent, admitted, installed, package, context) = fixture_for_work_mode_with_context(
    fixtures::work(),
    |profile| {
      profile.execution.environment.insert(
        "ISSUE_TOKEN".into(),
        FactoryRoleEnvironmentValue::Secret {
          reference: secret.clone(),
        },
      );
      profile.execution.mcp_servers.insert(
        key("issues"),
        FactoryRoleMcpBinding {
          configuration: configuration.reference().clone(),
          integration: None,
        },
      );
    },
    Default::default(),
    factory_role_task_digest,
    false,
    RoleFixtureOptions {
      additional_credentials: &["mcp-read"],
      ..Default::default()
    },
  );
  let role = FactoryRoleInvocation::new(package, None, [("OCTACITY_MCP_ISSUES".into(), secret)].into()).unwrap();
  let configurations = vec![FactoryCodexMcpConfiguration::try_from(configuration).unwrap()];
  let compiled = compile_codex_flow_task_with_options(
    &intent,
    &admitted,
    &installed.instructions,
    &installed.credentials,
    installed.consumer,
    &installed.usage_policy,
    FactoryCodexFlowCompilationOptions {
      role: Some(&role),
      context: Some(&context),
      mcp_configurations: Some(&configurations),
      ..Default::default()
    },
  )
  .unwrap();
  let mut native = native_capture_fixture(result_text);
  let mut provenance: Value = serde_json::from_slice(&native.documents[2].bytes).unwrap();
  provenance["settings"]["model"] = json!(role.package().inputs().task.model().identity());
  provenance["source_revision"] = json!(role.package().inputs().task.subject().exact().unwrap().base_revision());
  provenance["prompt_digest"]["value"] = json!(codex_prompt_provenance_digest(
    std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).unwrap()
  ));
  native.replace_document("codex-run-provenance", provenance);
  EvidenceFixture {
    role,
    compiled,
    configurations,
    native,
  }
}

fn model_conclusion() -> Value {
  json!({"assessment":{},"claims":[{
    "connection":"issues","tool":"lookup","arguments_json":"{\"query\":\"candidate\"}",
    "pointer":"/content/0/text","value_json":"\"candidate evidence\""
  }]})
}

#[test]
fn native_mcp_evidence_binds_the_actual_call_to_its_frozen_invocation_connection_and_resource() {
  let f = fixture("candidate evidence");
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let observations = bound.observations();
  assert_eq!(observations.len(), 1);
  assert_eq!(observations[0].connection.as_str(), "issues");
  assert_eq!(observations[0].configuration, *f.configurations[0].reference());
  assert_eq!(observations[0].resource, Some(reference("project-issues", 91)));
  assert_eq!(observations[0].tool, "lookup");
  assert_eq!(bound.invocation(), f.role.package().digest().unwrap());
  assert_eq!(observations[0].reference, capture.mcp_calls()[0].reference());
  assert_eq!(
    observations[0].coverage, "unknown",
    "native capture is not remote search completeness"
  );
}

#[test]
fn mcp_stage_conclusion_retains_verified_references_without_turning_model_claims_into_duplicate_or_absence_proof() {
  let f = fixture("candidate evidence");
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let reference = capture.mcp_calls()[0].reference();
  let mut value = model_conclusion();
  value["assessment"] = json!({"duplicate":true,"no_other_duplicates":true});
  value["verification"] = json!({"global_absence_proven":true,"duplicate_proven":true,"coverage":"all_projects"});
  let verified = bound.verify_conclusion(value).unwrap();
  assert_eq!(
    verified["assessment"]["duplicate"], true,
    "model assessment remains visibly separate data"
  );
  assert_eq!(verified["verification"]["status"], "observed");
  assert_eq!(verified["verification"]["global_absence_proven"], false);
  assert_eq!(verified["verification"]["duplicate_proven"], false);
  assert_eq!(verified["verification"]["coverage"], "unknown");
  assert_eq!(
    verified["verification"]["invocation"],
    json!(f.role.package().digest().unwrap())
  );
  assert_eq!(
    verified["verification"]["observations"][0]["reference"],
    json!(reference)
  );
}

#[test]
fn mcp_stage_conclusion_rejects_forged_candidate_values_record_paths_and_evidence_references() {
  let f = fixture("candidate evidence");
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let reference = capture.mcp_calls()[0].reference();
  for mutation in [
    "identity",
    "pointer",
    "record",
    "claim_reference",
    "duplicate",
    "connection",
    "tool",
    "arguments",
    "secret",
  ] {
    let mut value = model_conclusion();
    value["evidence"] = json!([reference]);
    value["claims"][0]["reference"] = json!(reference);
    match mutation {
      "identity" => value["claims"][0]["value_json"] = json!("\"invented hidden Issue\""),
      "pointer" => value["claims"][0]["pointer"] = json!("/arguments/query"),
      "record" => value["evidence"][0]["sequence"] = json!(99),
      "claim_reference" => value["claims"][0]["reference"]["sequence"] = json!(99),
      "connection" => value["claims"][0]["connection"] = json!("ambient"),
      "tool" => value["claims"][0]["tool"] = json!("write"),
      "arguments" => value["claims"][0]["arguments_json"] = json!("{\"query\":\"hidden\"}"),
      "duplicate" => value["evidence"] = json!([reference, reference]),
      _ => value["assessment"] = json!({"text":"Authorization: Bearer private-credential-canary"}),
    }
    assert!(bound.verify_conclusion(value).is_err(), "{mutation}");
  }
}

#[test]
fn mcp_model_can_cite_observed_values_before_server_artifact_ids_exist() {
  let f = fixture("candidate evidence");
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let observed = bound
    .verify_conclusion(model_conclusion())
    .expect("the model knows the call arguments and consumed value, but cannot know a future server Artifact ID");
  assert_eq!(observed["evidence"][0], json!(capture.mcp_calls()[0].reference()));
  assert_eq!(
    observed["claims"][0]["reference"],
    json!(capture.mcp_calls()[0].reference())
  );
  assert_eq!(observed["verification"]["status"], "observed");
}

#[test]
fn mcp_conclusion_rejects_sensitive_json_after_decoding() {
  for value_json in [
    r#""\u0067hp_fixture_review_canary""#,
    r#""\u0062earer fixture-review-canary""#,
    r#"{"\u0070assword":"fixture-review-canary"}"#,
  ] {
    let f = fixture(&format!("{{\"value\":{value_json}}}"));
    let capture = f.capture();
    let bound = capture
      .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
      .unwrap();
    let claim = json!({"assessment":{},"claims":[{
      "connection":"issues","tool":"lookup","arguments_json":"{\"query\":\"candidate\"}",
      "pointer":"/value","json_text":0,"value_json":value_json
    }]});
    assert!(
      !octacity_server_factory::contains_sensitive_material(&claim.to_string()),
      "this case reaches the decoded-value check, not the existing wire check"
    );
    assert!(
      bound.verify_conclusion(claim).is_err(),
      "decoded sensitive material: {value_json}"
    );
  }
}

#[test]
fn mcp_conclusion_rejects_the_undocumented_reference_only_claim_grammar() {
  let f = fixture("candidate evidence");
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let reference = capture.mcp_calls()[0].reference();
  assert!(
    bound
      .verify_conclusion(json!({"assessment":{},"evidence":[reference],"claims":[{
        "reference":reference,"pointer":"/content/0/text","value":"candidate evidence"
      }]}))
      .is_err(),
    "the unreleased contract has one model-facing claim grammar"
  );
}

#[test]
fn mcp_conclusion_schema_and_verifier_share_the_flow_pointer_boundary() {
  let field = "x".repeat(255);
  let pointer = format!("/{field}");
  let f = fixture(&json!({field:"candidate evidence"}).to_string());
  let capture = f.capture();
  let bound = capture
    .bind_mcp_connections(&f.role, &f.compiled, &f.configurations)
    .unwrap();
  let mut claim = model_conclusion();
  claim["claims"][0]["pointer"] = json!(pointer);
  claim["claims"][0]["json_text"] = json!(0);
  let retained = bound
    .verify_conclusion(claim.clone())
    .expect("a 256-byte Flow pointer is supported");
  let schema = FlowDataSchema::new(
    key("mcp-bounds"),
    key("v1"),
    factory_mcp_conclusion_shape(FlowValueSchema::Object {
      fields: BTreeMap::new(),
    }),
  )
  .unwrap();
  assert!(octacity_server_factory::FlowPayload::new(&schema, retained.clone()).is_ok());
  claim["claims"][0]["pointer"] = json!(format!("{pointer}x"));
  assert!(
    bound.verify_conclusion(claim).is_err(),
    "the Flow mapping rejects a 257-byte pointer"
  );
  let mut overlong = retained;
  overlong["claims"][0]["pointer"] = json!(format!("{pointer}x"));
  assert!(
    octacity_server_factory::FlowPayload::new(&schema, overlong).is_err(),
    "the advertised schema must not promise pointers the verifier rejects"
  );
}

#[path = "factory_triage_eligibility_tests.rs"]
mod triage_eligibility;
