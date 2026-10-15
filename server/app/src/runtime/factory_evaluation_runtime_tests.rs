//! Deterministic Decision profiles use the same native adapter through file installation.
use super::*;
use octacity_server_application::FactoryEvaluationDecisionProfile;
use octacity_server_factory::*;

fn profile() -> FactoryEvaluationDecisionProfile {
  let key = |value: &str| FactoryKey::new(value).unwrap();
  let schema = ImmutableReference::new(key("decision-contract"), key("v1"), FactoryDigest::from_bytes([3; 32]));
  FactoryEvaluationDecisionProfile {
    action: key("apply-policy"),
    policy: DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![key("selected-tests")],
        required_evaluators: vec![key("independent-reviewer")],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: DecisionOutcome::Rework,
      },
    )
    .unwrap(),
    outcomes: [
      DecisionOutcome::Accept,
      DecisionOutcome::Rework,
      DecisionOutcome::Reject,
      DecisionOutcome::Escalate,
      DecisionOutcome::Cancel,
    ]
    .into_iter()
    .map(|disposition| {
      (
        disposition,
        FlowOutcomeDefinition::new(key(disposition.as_str()), FlowOutcomeKind::Success, schema.clone()),
      )
    })
    .collect(),
  }
}
fn identity(profile: &FactoryEvaluationDecisionProfile) -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new("installed-decision").unwrap(),
    FactoryKey::new("v1").unwrap(),
    profile.digest().unwrap(),
  )
}

#[tokio::test]
async fn native_decision_executor_requires_its_exact_installed_policy_pin() {
  let profile = profile();
  let identity = identity(&profile);
  let resources = super::factory_tests::resources();
  let (kind, execution, _) = resources
    .evaluation_decision_executor(identity.clone(), profile.clone())
    .unwrap();
  assert_eq!(kind, FlowNodeKind::TrustedAction);
  assert_eq!(execution, NodeExecutionIdentity::External(identity.clone()));
  let mut changed = profile;
  changed.outcomes[0].1 = FlowOutcomeDefinition::new(
    FactoryKey::new("another-route").unwrap(),
    FlowOutcomeKind::Success,
    changed.outcomes[0].1.schema().clone(),
  );
  assert!(resources.evaluation_decision_executor(identity, changed).is_err());
}

#[tokio::test]
async fn file_decision_profile_installs_native_policy_and_rejects_unrepinned_rules() {
  let profile = profile();
  let identity = identity(&profile);
  let project = octacity_server_domain::ProjectId::generate();
  let mut document = serde_json::json!({
    "version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[]}],"codex":[],
    "evaluation_decisions":[{"identity":identity,"profile":{
      "action":profile.action,
      "outcomes":profile.outcomes.iter().map(|(disposition,outcome)| serde_json::json!({"disposition":disposition,"outcome":outcome})).collect::<Vec<_>>(),
      "policy":{"version":1,"required_evidence":["selected-tests"],"required_evaluators":["independent-reviewer"],"quorum":1,"severity_threshold":"high","indeterminate_policy":"required_only","failure_outcome":"rework"}
    }}]
  });
  let bytes = serde_json::to_vec(&document).unwrap();
  let installer = super::factory_profile_file::FileProfileInstaller::parse_json(&bytes, &[])
    .expect("installed deterministic policy is deployable through FILE");
  let installed = installer.install(super::factory_tests::resources()).await.unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.executors[0].1, NodeExecutionIdentity::External(identity));
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&bytes).is_err(),
    "metadata-only parsing cannot silently omit this executor"
  );
  document["evaluation_decisions"][0]["profile"]["policy"]["required_evaluators"] =
    serde_json::json!(["substituted-reviewer"]);
  assert!(
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .is_err()
  );
}
