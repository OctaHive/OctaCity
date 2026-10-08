use std::{
  future::Future,
  sync::atomic::{AtomicUsize, Ordering},
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_protocol::{
  FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryImmutableReferenceV3, FactoryMountModeV3,
  FactoryMountPermissionV3, FactoryOutputPermissionsV3, FactoryPermissionSetV3, FactoryResourceLimitsV3,
  FactoryToolActionDecisionSourceV3, FactoryToolActionDispositionV3, FactoryToolActionProposalV3,
  FactoryToolPathAccessV3, FactoryToolPathV3,
};
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, DecisionSignalChoiceCriterion, DecisionSignalChoices, DecisionSignalDigests, DecisionSignalDisposition,
  DecisionSignalFallback, DecisionSignalInputMedia, DecisionSignalMode, DecisionSignalProbability,
  DecisionSignalProfile, DecisionSignalProfileDefinition, DecisionSignalProviderCapability,
  DecisionSignalProviderInput, DecisionSignalProviderLimits, DecisionSignalProviderRequest, DecisionSignalPurpose,
  DecisionSignalQuestion, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind,
  DecisionSignalReceiptId, DecisionSignalRequest, DecisionSignalRequestId, DecisionSignalThreshold, ExactSubject,
  ExternalWorkIdentity, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership, FactoryConfigurationId,
  FactoryConfigurationRef, FactoryConfigurationVersion, FactoryDigest, FactoryKey, FactoryMetadata, FactoryRun,
  FactoryRunId, FactoryText, ImmutableReference, RiskClass, StageAttempt, StageAttemptId, StageAttemptNumber,
  ToolRiskChoiceMapping, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};

use super::{
  DecisionSignalApplicationError, DeterministicToolActionRule, ProtectedToolActionGate, ToolRiskAssessment,
  ToolRiskSignalEvaluation, ToolRiskSignalEvaluator,
};

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn out_of_envelope_actions_are_denied_before_a_signal_provider_is_reached() {
  run_ready(async {
    let evaluator = RecordingEvaluator::successful(FactoryToolActionDispositionV3::Allow);
    let gate = ProtectedToolActionGate::new(Some(&evaluator));
    let mut proposal = proposal();
    proposal.network_hosts.push("outside.example".to_owned());
    let prepared = ProtectedToolActionGate::prepare(proposal).unwrap();
    let request = signal_request(prepared.state_document().clone(), DecisionSignalFallback::Escalate);
    let mapping = mapping(&request);

    let decision = gate
      .authorize(
        &prepared,
        &permissions(),
        &permissions(),
        DeterministicToolActionRule::Assess,
        Some(assessment(&request, &mapping)),
      )
      .await
      .unwrap();

    assert_eq!(decision.disposition(), FactoryToolActionDispositionV3::Deny);
    assert_eq!(decision.source(), FactoryToolActionDecisionSourceV3::HardPolicy);
    assert_eq!(evaluator.calls(), 0);
  });
}

#[test]
fn signals_only_preserve_or_narrow_the_same_redacted_action() {
  for expected in [
    FactoryToolActionDispositionV3::Allow,
    FactoryToolActionDispositionV3::Deny,
    FactoryToolActionDispositionV3::Escalate,
  ] {
    run_ready(async {
      let evaluator = RecordingEvaluator::successful(expected);
      let gate = ProtectedToolActionGate::new(Some(&evaluator));
      let prepared = ProtectedToolActionGate::prepare(proposal()).unwrap();
      let request = signal_request(prepared.state_document().clone(), DecisionSignalFallback::Deny);
      let mapping = mapping(&request);

      let decision = gate
        .authorize(
          &prepared,
          &permissions(),
          &permissions(),
          DeterministicToolActionRule::Assess,
          Some(assessment(&request, &mapping)),
        )
        .await
        .unwrap();

      assert_eq!(decision.disposition(), expected);
      assert_eq!(decision.source(), FactoryToolActionDecisionSourceV3::DecisionSignal);
      assert_eq!(decision.proposal_sha256(), prepared.proposal_digest().to_string());
      assert_eq!(evaluator.calls(), 1);
    });
  }
}

#[test]
fn unavailable_or_mismatched_assessment_fails_closed_without_raw_data() {
  run_ready(async {
    let evaluator = RecordingEvaluator::unavailable();
    let gate = ProtectedToolActionGate::new(Some(&evaluator));
    let prepared = ProtectedToolActionGate::prepare(proposal()).unwrap();
    let request = signal_request(prepared.state_document().clone(), DecisionSignalFallback::Escalate);
    let assessment_mapping = mapping(&request);
    let decision = gate
      .authorize(
        &prepared,
        &permissions(),
        &permissions(),
        DeterministicToolActionRule::Assess,
        Some(assessment(&request, &assessment_mapping)),
      )
      .await
      .unwrap();
    assert_eq!(decision.disposition(), FactoryToolActionDispositionV3::Escalate);
    assert_eq!(decision.source(), FactoryToolActionDecisionSourceV3::FailClosed);

    let serialized = format!(
      "{} {} {prepared:?}",
      serde_json::to_string(&request).unwrap(),
      serde_json::to_string(&decision).unwrap()
    );
    for sensitive in [
      "sensitive-argument",
      "/workspace/source/private",
      "api.openai.com",
      "model-coding",
    ] {
      assert!(!serialized.contains(sensitive), "retained raw value {sensitive}");
    }

    let mismatch_evaluator = RecordingEvaluator::successful(FactoryToolActionDispositionV3::Allow);
    let mismatch_gate = ProtectedToolActionGate::new(Some(&mismatch_evaluator));
    let mismatched_request = signal_request(
      FactoryText::new("{\"different\":true}").unwrap(),
      DecisionSignalFallback::Deny,
    );
    let mismatched_mapping = mapping(&mismatched_request);
    let mismatched = mismatch_gate
      .authorize(
        &prepared,
        &permissions(),
        &permissions(),
        DeterministicToolActionRule::Assess,
        Some(assessment(&mismatched_request, &mismatched_mapping)),
      )
      .await
      .unwrap();
    assert_eq!(mismatched.disposition(), FactoryToolActionDispositionV3::Deny);
    assert_eq!(mismatched.source(), FactoryToolActionDecisionSourceV3::FailClosed);
    assert_eq!(mismatch_evaluator.calls(), 0);
  });
}

#[test]
fn deterministic_hard_allow_and_deny_skip_the_provider() {
  run_ready(async {
    let evaluator = RecordingEvaluator::successful(FactoryToolActionDispositionV3::Deny);
    let gate = ProtectedToolActionGate::new(Some(&evaluator));
    let prepared = ProtectedToolActionGate::prepare(proposal()).unwrap();
    for (rule, expected) in [
      (
        DeterministicToolActionRule::Allow,
        FactoryToolActionDispositionV3::Allow,
      ),
      (DeterministicToolActionRule::Deny, FactoryToolActionDispositionV3::Deny),
    ] {
      let decision = gate
        .authorize(&prepared, &permissions(), &permissions(), rule, None)
        .await
        .unwrap();
      assert_eq!(decision.disposition(), expected);
      assert_eq!(decision.source(), FactoryToolActionDecisionSourceV3::HardPolicy);
    }
    assert_eq!(evaluator.calls(), 0);
  });
}

struct RecordingEvaluator {
  calls: AtomicUsize,
  outcome: Option<FactoryToolActionDispositionV3>,
}

impl RecordingEvaluator {
  const fn successful(outcome: FactoryToolActionDispositionV3) -> Self {
    Self {
      calls: AtomicUsize::new(0),
      outcome: Some(outcome),
    }
  }

  const fn unavailable() -> Self {
    Self {
      calls: AtomicUsize::new(0),
      outcome: None,
    }
  }

  fn calls(&self) -> usize {
    self.calls.load(Ordering::Acquire)
  }
}

#[async_trait]
impl ToolRiskSignalEvaluator for RecordingEvaluator {
  async fn assess(
    &self,
    _receipt_id: DecisionSignalReceiptId,
    _request: &DecisionSignalProviderRequest,
    _mapping: &ToolRiskChoiceMapping,
  ) -> Result<ToolRiskSignalEvaluation, DecisionSignalApplicationError> {
    self.calls.fetch_add(1, Ordering::AcqRel);
    self
      .outcome
      .map(|outcome| ToolRiskSignalEvaluation::new(outcome, digest(41)))
      .ok_or(DecisionSignalApplicationError::ClockUnavailable)
  }
}

fn assessment<'a>(
  request: &'a DecisionSignalProviderRequest,
  mapping: &'a ToolRiskChoiceMapping,
) -> ToolRiskAssessment<'a> {
  ToolRiskAssessment {
    receipt_id: DecisionSignalReceiptId::generate(),
    request,
    mapping,
  }
}

fn mapping(request: &DecisionSignalProviderRequest) -> ToolRiskChoiceMapping {
  ToolRiskChoiceMapping::try_new(
    request.choices(),
    vec![
      (key("allow"), DecisionSignalDisposition::Allow),
      (key("deny"), DecisionSignalDisposition::Deny),
      (key("escalate"), DecisionSignalDisposition::Escalate),
    ],
  )
  .unwrap()
}

fn signal_request(state: FactoryText, fallback: DecisionSignalFallback) -> DecisionSignalProviderRequest {
  let choices = DecisionSignalChoices::try_new(vec![key("allow"), key("deny"), key("escalate")]).unwrap();
  let profile = DecisionSignalProfile::new(DecisionSignalProfileDefinition {
    purpose: DecisionSignalPurpose::ToolRisk,
    provider: exact("provider", "v1", 3),
    adapter: exact("adapter", "v1", 4),
    model: exact("model", "v7", 5),
    question_set: exact("questions", "v2", 6),
    policy: exact("policy", "v4", 7),
    mode: DecisionSignalMode::BoundedControl,
    fallback,
    budget: budget(),
    routes: None,
  })
  .unwrap();
  let input = DecisionSignalProviderInput::new(
    DecisionSignalInputMedia::CanonicalJson,
    state,
    vec![
      DecisionSignalQuestion::new(
        exact("tool-risk", "v1", 8),
        FactoryText::new("Classify only the redacted in-envelope action.").unwrap(),
        DecisionSignalQuestionDomain::FiniteChoice(choices),
        DecisionSignalQuestionCriteria::FiniteChoice(vec![
          DecisionSignalChoiceCriterion::new(key("allow"), FactoryText::new("Preserve deterministic allow.").unwrap()),
          DecisionSignalChoiceCriterion::new(key("deny"), FactoryText::new("Narrow to deny.").unwrap()),
          DecisionSignalChoiceCriterion::new(key("escalate"), FactoryText::new("Narrow to escalation.").unwrap()),
        ]),
      )
      .unwrap(),
    ],
    key("tool-risk"),
    None,
  )
  .unwrap();
  let capability = DecisionSignalProviderCapability::new(
    profile.provider().clone(),
    profile.adapter().clone(),
    profile.model().clone(),
    exact("semantics", "v1", 9),
    exact("private-data", "v1", 11),
    vec![DecisionSignalPurpose::ToolRisk],
    vec![DecisionSignalInputMedia::CanonicalJson],
    vec![DecisionSignalQuestionKind::FiniteChoice],
    DecisionSignalProviderLimits::new(1_024, 4, 4).unwrap(),
    budget(),
  )
  .unwrap();
  let request = DecisionSignalRequest::new(
    DecisionSignalRequestId::generate(),
    &stage(),
    DecisionSignalPurpose::ToolRisk,
    DecisionSignalDigests::new(input.digest(), profile.policy().digest()),
    budget(),
  );
  DecisionSignalProviderRequest::new(
    request,
    &profile,
    &capability,
    input,
    DecisionSignalThreshold::new(
      DecisionSignalPurpose::ToolRisk,
      capability.probability_semantics().clone(),
      exact("calibration", "v1", 10),
      DecisionSignalProbability::new(600_000).unwrap(),
      DecisionSignalProbability::new(100_000).unwrap(),
    )
    .unwrap(),
    Timestamp::from_unix_millis(10_000).unwrap(),
  )
  .unwrap()
}

fn proposal() -> FactoryToolActionProposalV3 {
  FactoryToolActionProposalV3 {
    tool: wire_reference("shell"),
    executable: wire_reference("codex-cli"),
    arguments: vec!["sensitive-argument".to_owned()],
    paths: vec![FactoryToolPathV3 {
      path: "/workspace/source/private".to_owned(),
      access: FactoryToolPathAccessV3::Read,
    }],
    network_hosts: vec!["api.openai.com".to_owned()],
    secret_profiles: vec!["model-coding".to_owned()],
    workload_identity_profiles: vec![],
    descendants: 1,
    resources: FactoryResourceLimitsV3 {
      cpu_millis: 500,
      memory_bytes: 512,
      disk_bytes: 512,
      process_count: 2,
      elapsed_millis: 1_000,
    },
    outputs: FactoryOutputPermissionsV3 {
      kinds: vec!["codex-result".to_owned()],
      max_artifact_count: 1,
      max_artifact_bytes: 512,
      max_report_count: 0,
      max_report_bytes: 0,
    },
  }
}

fn permissions() -> FactoryPermissionSetV3 {
  let executable = wire_reference("codex-cli");
  FactoryPermissionSetV3 {
    plugins: vec![wire_reference("codex")],
    executables: vec![executable.clone()],
    tools: vec![wire_reference("shell")],
    commands: vec![FactoryCommandPermissionV3 {
      executable,
      arguments: vec![FactoryCommandArgumentV3::Any { max_bytes: 256 }],
    }],
    max_descendants: 2,
    mounts: vec![
      FactoryMountPermissionV3 {
        root: "/octacity/protected".to_owned(),
        mode: FactoryMountModeV3::ReadOnly,
      },
      FactoryMountPermissionV3 {
        root: "/workspace/output".to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
      FactoryMountPermissionV3 {
        root: "/workspace/scratch".to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
      FactoryMountPermissionV3 {
        root: "/workspace/source".to_owned(),
        mode: FactoryMountModeV3::ReadWrite,
      },
    ],
    network_hosts: vec!["api.openai.com".to_owned()],
    secret_profiles: vec!["model-coding".to_owned()],
    workload_identity_profiles: vec![],
    resources: FactoryResourceLimitsV3 {
      cpu_millis: 1_000,
      memory_bytes: 1_024,
      disk_bytes: 1_024,
      process_count: 4,
      elapsed_millis: 60_000,
    },
    outputs: FactoryOutputPermissionsV3 {
      kinds: vec!["codex-result".to_owned()],
      max_artifact_count: 2,
      max_artifact_bytes: 1_024,
      max_report_count: 0,
      max_report_bytes: 0,
    },
  }
}

fn stage() -> StageAttempt {
  let subject = ExactSubject::new(
    ProjectId::generate(),
    RepositoryId::generate(),
    ImmutableRevision::new("base").unwrap(),
  );
  let configuration = FactoryConfigurationRef::new(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    subject.project_id(),
    digest(1),
  );
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration,
    ExternalWorkIdentity::new("source/1").unwrap(),
    subject,
    WorkArtifacts::new(ArtifactId::generate(), ArtifactId::generate(), vec![]).unwrap(),
    WorkClassification::new(
      WorkPriority::new(0).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  StageAttempt::new(
    StageAttemptId::generate(),
    &run,
    StageAttemptNumber::INITIAL,
    octacity_server_factory::FactoryStageTarget::Implementation,
    budget(),
    digest(2),
    FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(digest(12)),
        Timestamp::from_unix_millis(1).unwrap(),
        Timestamp::from_unix_millis(2).unwrap(),
      )
      .unwrap(),
    ),
  )
}

fn budget() -> BudgetLimit {
  BudgetLimit::new(2, 10_000, 10_000, 2_000_000, 10_000).unwrap()
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn exact(identity: &str, version: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(value))
}

fn wire_reference(identity: &str) -> FactoryImmutableReferenceV3 {
  FactoryImmutableReferenceV3 {
    identity: identity.to_owned(),
    version: "1.0.0".to_owned(),
    sha256: DIGEST.to_owned(),
  }
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory tool-action future must be immediately ready"),
  }
}
