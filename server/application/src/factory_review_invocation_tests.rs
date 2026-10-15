//! Review authority comes only from full explicit owner-frozen typed selections.
use crate::*;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;

fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
#[test]
fn review_invocation_qualifies_only_the_full_selected_plan_evidence_and_installed_evaluator() {
  crate::factory_admission_tests::run_ready(async {
    let (store, input, evaluator) = crate::factory_review_planning_tests::prepared_review_invocation(true).await;
    let source = FactoryStoredReviewInvocationSource::new(store);
    let proof = source.invocation(&input, &evaluator, at(31)).await.unwrap();
    assert_eq!(proof.input_digest(), input.digest().unwrap());
    assert_eq!(proof.evaluator(), &evaluator);
    assert_eq!(
      proof.plan().subject().candidate_revision().as_str(),
      "reviewed-revision"
    );
    assert_eq!(proof.plan().evidence_id(), proof.evidence().id());
    assert_eq!(proof.plan().required_quorum(), 1);
    assert_eq!(proof.evidence().items().len(), 1);
    assert_eq!(proof.evidence().items()[0].outcome(), DeterministicGateOutcome::Passed);
    let replay = source.invocation(&input, &evaluator, at(32)).await.unwrap();
    assert_eq!(replay.plan(), proof.plan());
    assert_eq!(replay.evidence(), proof.evidence());
    let other = ImmutableReference::new(
      FactoryKey::new("uninstalled-reviewer").unwrap(),
      FactoryKey::new("v1").unwrap(),
      FactoryDigest::from_bytes([5; 32]),
    );
    assert!(source.invocation(&input, &other, at(31)).await.is_err());
    assert!(source.invocation(&input, &evaluator, at(2000)).await.is_err());
    let (store, partial, evaluator) = crate::factory_review_planning_tests::prepared_review_invocation(false).await;
    assert_eq!(
      partial.sources().len(),
      2,
      "a selected source does not authorize copying its omitted fields"
    );
    assert!(
      FactoryStoredReviewInvocationSource::new(store)
        .invocation(&partial, &evaluator, at(31))
        .await
        .is_err()
    );
  });
}
#[test]
fn qualified_review_compilation_freezes_adapter_evaluation_schema_under_the_typed_assessment_contract() {
  crate::factory_admission_tests::run_ready(async {
    let (runs, intent, admitted, profile, context) =
      crate::factory_review_planning_tests::prepared_codex_review_invocation().await;
    let policy = profile.reviewer.as_ref().unwrap();
    let invocation = FactoryStoredReviewInvocationSource::new(runs)
      .invocation(intent.input(), &policy.evaluator, at(31))
      .await
      .unwrap();
    let compile = |reviewer| {
      compile_codex_flow_task_with_options(
        &intent,
        &admitted,
        &profile.instructions,
        &profile.credentials,
        profile.consumer,
        &profile.usage_policy,
        FactoryCodexFlowCompilationOptions {
          context: Some(&context),
          reviewer,
          ..Default::default()
        },
      )
    };
    assert!(
      compile(None).is_err(),
      "the ordinary schema path cannot consume a repinned reviewer role"
    );
    let compiled = compile(Some(FactoryCodexReviewCompilation {
      policy,
      invocation: &invocation,
    }))
    .unwrap();
    let schema: serde_json::Value =
      serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::ResultSchema).bytes()).unwrap();
    assert_eq!(
      schema["properties"]["outcome"]["enum"],
      serde_json::json!(["satisfied", "violated", "indeterminate"])
    );
    assert!(schema["properties"].get("assessment_id").is_none());
    assert!(schema["properties"].get("plan_id").is_none());
    let prompt = String::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes().to_vec()).unwrap();
    assert!(prompt.contains("Review the actual candidate against the selected deterministic evidence."));
    let envelope: serde_json::Value =
      serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::TaskEnvelope).bytes()).unwrap();
    let expected_schema = admitted
      .closure()
      .definition(intent.input().definition())
      .unwrap()
      .node(intent.input().node())
      .unwrap()
      .outcome(intent.result_outcome())
      .unwrap()
      .schema();
    assert_eq!(
      envelope["result_schema"],
      serde_json::to_value(expected_schema).unwrap()
    );
    let mut replacement = policy.clone();
    replacement.evaluator = ImmutableReference::new(
      FactoryKey::new("uninstalled").unwrap(),
      FactoryKey::new("v1").unwrap(),
      FactoryDigest::from_bytes([9; 32]),
    );
    assert!(
      compile(Some(FactoryCodexReviewCompilation {
        policy: &replacement,
        invocation: &invocation
      }))
      .is_err()
    );
  });
}
