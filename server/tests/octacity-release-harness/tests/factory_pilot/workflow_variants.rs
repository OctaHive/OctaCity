//! Two lightweight variants reuse the caller's live Server and Agent.
//! The full Run A must use [Research,Requirements] and finish before this helper.
use super::*;
use octacity_server_factory::*;
use workflow_prelude::{CaptureVariantClosure, Prelude, PreludePhase, PreludeVariant, PublishedVariant};

pub(super) struct VariantProbeContext<'a> {
  pub prelude: &'a Prelude,
  pub client: &'a Client,
  pub origin: &'a str,
  pub resources: &'a resources::PilotResources,
  /// Actual bootstrap Artifact collection, including task and acceptance.
  pub artifacts: &'a Value,
  pub revision: &'a str,
  pub ceiling: &'a FactoryPermissionSet,
  /// Private existing pilot database; never rendered into diagnostics.
  pub postgres: &'a str,
  pub run_a: &'a str,
  pub root_reference_a: &'a Value,
  pub published_a: &'a PublishedVariant,
}
pub(super) struct VariantProbeEvidence {
  run: String,
  root_reference: Value,
  publication: PublishedVariant,
  frozen_closure: CaptureVariantClosure,
  builds: Value,
  completions: Value,
}
fn key(name: &str) -> FactoryKey {
  FactoryKey::new(name).unwrap()
}
fn original_artifacts(context: &VariantProbeContext<'_>) -> Value {
  let artifact = |name: &str| {
    let item = context.artifacts["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|item| item["name"] == name)
      .unwrap();
    json!({"artifact_id":item["id"],"content_digest":item["sha256"],"encoded_size":item["size_bytes"]})
  };
  json!({"task":artifact("task"),"acceptance":artifact("acceptance"),"specifications":[]})
}
async fn capture(
  context: &VariantProbeContext<'_>,
  run: &str,
  root_reference: Value,
  publication: PublishedVariant,
) -> VariantProbeEvidence {
  let frozen_closure =
    CaptureVariantClosure::capture(context.client, context.origin, run, &root_reference, &publication).await;
  VariantProbeEvidence {
    run: run.to_owned(),
    root_reference,
    publication,
    frozen_closure,
    builds: workflow::diagnostic(context.client, context.origin, run, "builds").await,
    completions: workflow::diagnostic(context.client, context.origin, run, "node-completions").await,
  }
}
/// Run after A completes, before the existing caller-owned Server/Agent restart.
/// No processes, provider installations, credentials or bootstrap Jobs are created.
pub(super) async fn run(context: &VariantProbeContext<'_>) -> Vec<VariantProbeEvidence> {
  assert_eq!(
    context.published_a.variant.phases,
    [PreludePhase::Research, PreludePhase::Requirements]
  );
  assert_eq!(
    get_json(
      context.client,
      format!("{}/api/v1/factory-runs/{}", context.origin, context.run_a)
    )
    .await["state"],
    "completed"
  );
  let mut evidence = vec![
    capture(
      context,
      context.run_a,
      context.root_reference_a.clone(),
      context.published_a.clone(),
    )
    .await,
  ];
  let artifacts = original_artifacts(context);
  for phases in [vec![PreludePhase::Requirements, PreludePhase::Research], vec![]] {
    let variant = PreludeVariant::new(phases);
    let identity = Uuid::new_v4().simple().to_string();
    let acceptance = format!("variant-acceptance-{identity}");
    let accepted = context.prelude.accepted_schema();
    let gate_budget = BudgetLimit::new(1, workflow_prelude::LOCAL_GATE_WINDOW_MILLIS, 0, 0, 16 * 1024).unwrap();
    let child_budget = context.prelude.budget_for(&variant);
    // Sum the declared parent call reservation and local accepting-gate ceiling.
    let budget = BudgetLimit::new(
      child_budget.max_attempts() + 2,
      child_budget.max_elapsed_millis() + gate_budget.max_elapsed_millis(),
      child_budget.max_tokens(),
      child_budget.max_cost_micro_units(),
      child_budget.max_output_bytes() + gate_budget.max_output_bytes(),
    )
    .unwrap();
    let mut gate = workflow::node(
      &acceptance,
      "deterministic_gate",
      accepted,
      gate_budget,
      &FactoryPermissionSet::deny_all(),
      "accepted",
    );
    gate["accepted_work_outcomes"] = json!(["accepted"]);
    gate["gate"] = json!(
      FlowGateProgram::new(vec![], key("accepted"))
        .unwrap()
        .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
        .unwrap()
        .binding()
        .unwrap()
    );
    let mut draft = workflow::draft(
      &acceptance,
      vec![gate],
      vec![workflow::edge(&acceptance, "accepted", "terminal", "accepted")],
      "accepted",
      accepted,
      budget,
      context.ceiling,
    );
    let publication = context
      .prelude
      .extend_variant(context.client, context.origin, &mut draft, &acceptance, None, variant)
      .await;
    let previous = &evidence.last().unwrap().root_reference;
    let id = previous["id"].as_str().unwrap();
    let version = previous["version"].as_u64().unwrap();
    // Same immutable parent identity, next guarded version. This is not a new Flow ID.
    let response = context
      .client
      .post(format!("{}/api/v1/flow-definitions/{id}/versions", context.origin))
      .header("idempotency-key", format!("variant-parent-{identity}"))
      .header("if-match", format!("\"{version}\""))
      .json(&json!({"definition":draft}))
      .send()
      .await
      .unwrap();
    assert!(
      response.status().is_success(),
      "variant publication rejected with {}",
      response.status()
    );
    let parent: Value = response.json().await.unwrap();
    let root_reference = parent["resource"]["flow"]["closure"]["root"].clone();
    assert_eq!(root_reference["id"], previous["id"]);
    assert_eq!(root_reference["version"], version + 1);
    assert_ne!(root_reference["digest"], previous["digest"]);
    let child_details = get_json(
      context.client,
      format!(
        "{}/api/v1/flow-definitions/{}/versions/{}",
        context.origin,
        publication.child_reference["id"].as_str().unwrap(),
        publication.child_reference["version"].as_u64().unwrap()
      ),
    )
    .await;
    context
      .prelude
      .assert_variant_definition(&child_details, &publication.variant);
    let configuration = post_management(
      context.client,
      context.origin,
      "/api/v1/factory-configurations",
      &format!("variant-config-{identity}"),
      json!({"id":Uuid::new_v4(),"project_id":context.resources.project,"definition":{
        "admission_policy":"pilot-admission","permission_ceiling":"pilot-ceiling","hard_budget":budget,
        "wip_limits":{"max_active_runs":4,"max_active_nodes":4},"enabled":true,"flow":parent["resource"]["flow"]}}),
    )
    .await;
    let admitted=post_management(context.client,context.origin,&format!("/api/v1/projects/{}/factory-work",context.resources.project),
      &format!("variant-work-{identity}"),json!({
        "configuration_id":configuration["resource"]["configuration"]["reference"]["id"],"configuration_version":1,
        "repository_id":context.resources.repository,"repository_version":1,"external_identity":format!("variant-probe/{identity}"),
        "source":{"kind":"exact_revision","value":context.revision},"artifacts":artifacts,
        "classification":{"priority":40,"risk":"low","metadata":{"work_kind":"feature_request","size":"small"}}})).await;
    let run = admitted["factory_run_id"].as_str().unwrap();
    timeout(Duration::from_millis(budget.max_elapsed_millis()), async {
      loop {
        let state = get_json(context.client, format!("{}/api/v1/factory-runs/{run}", context.origin)).await;
        if state["state"] == "completed" {
          break;
        }
        assert!(
          ["admitted", "running", "waiting"].contains(&state["state"].as_str().unwrap()),
          "probe must remain in a live state"
        );
        sleep(Duration::from_millis(100)).await;
      }
    })
    .await
    .expect("variant probe completes inside its frozen declared bound");
    // Read the actual atomically committed accepting record, never reconstruct one.
    let pool = sqlx::PgPool::connect(context.postgres).await.unwrap();
    let bytes:Vec<u8>=sqlx::query_scalar("SELECT record.document FROM factory_flow_records record JOIN factory_node_attempts attempt ON attempt.id=record.node_attempt_id AND attempt.run_id=record.run_id WHERE record.run_id=$1 AND attempt.node_key=$2")
      .bind(run.parse::<Uuid>().unwrap()).bind(&acceptance).fetch_one(&pool).await.unwrap();
    pool.close().await;
    let record: Value = serde_json::from_slice(&bytes).unwrap();
    context
      .prelude
      .verify_variant(
        context.client,
        context.origin,
        run,
        &record["payload"]["value"],
        &artifacts,
        &publication.variant,
      )
      .await;
    let probe = capture(context, run, root_reference, publication).await;
    assert_eq!(
      probe.builds["items"].as_array().unwrap().len(),
      2 + probe.publication.variant.phases.len(),
      "only declared readonly roles create ordinary Builds"
    );
    assert_eq!(
      probe.completions["items"].as_array().unwrap().len(),
      7 + probe.publication.variant.phases.len(),
      "fixed five child nodes plus optional Jobs, parent call and acceptance"
    );
    evidence.push(probe);
    verify_after_restart(context, &evidence).await; // also proves A stayed frozen after each publication
  }
  evidence
}
/// The caller invokes this again after its existing real Server/Agent restart.
pub(super) async fn verify_after_restart(context: &VariantProbeContext<'_>, evidence: &[VariantProbeEvidence]) {
  for probe in evidence {
    let closure = CaptureVariantClosure::capture(
      context.client,
      context.origin,
      &probe.run,
      &probe.root_reference,
      &probe.publication,
    )
    .await;
    assert_eq!(
      closure, probe.frozen_closure,
      "new versions/restart cannot replace admitted closure bytes or refs"
    );
    assert_eq!(
      workflow::diagnostic(context.client, context.origin, &probe.run, "builds").await,
      probe.builds,
      "no repeated Job after restart"
    );
    assert_eq!(
      workflow::diagnostic(context.client, context.origin, &probe.run, "node-completions").await,
      probe.completions
    );
  }
}
