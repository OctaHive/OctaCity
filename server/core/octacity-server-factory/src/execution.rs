use crate::{
  BudgetLimit, BudgetUsage, ContextManifest, ContextManifestId, DecisionSignalPurpose, DecisionSignalRequestId,
  ExactSubject, FactoryClaim, FactoryDigest, FactoryError, FactoryKey, FactoryRun, FactoryRunId, FactoryStageKind,
  FactoryStageTarget, FactoryTaskSubject, MacroCallId, MacroCallKind, StageAttemptId, StageAttemptNumber,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// Fenced worker ownership attached to immutable Factory execution records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactoryClaimOwnership {
  owner: FactoryKey,
  claim: FactoryClaim,
}

impl FactoryClaimOwnership {
  /// Binds one bounded worker identity to its exclusive claim window.
  #[must_use]
  pub const fn new(owner: FactoryKey, claim: FactoryClaim) -> Self {
    Self { owner, claim }
  }

  /// Returns the bounded worker identity.
  #[must_use]
  pub const fn owner(&self) -> &FactoryKey {
    &self.owner
  }

  /// Returns the exclusive fenced claim window.
  #[must_use]
  pub const fn claim(&self) -> FactoryClaim {
    self.claim
  }
}

/// Canonical input and deterministic policy digests for one Decision Signal request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalDigests {
  input: FactoryDigest,
  policy: FactoryDigest,
}

impl DecisionSignalDigests {
  /// Constructs the exact immutable request digests.
  #[must_use]
  pub const fn new(input: FactoryDigest, policy: FactoryDigest) -> Self {
    Self { input, policy }
  }

  /// Returns the canonical redacted input digest.
  #[must_use]
  pub const fn input(self) -> FactoryDigest {
    self.input
  }

  /// Returns the deterministic policy digest.
  #[must_use]
  pub const fn policy(self) -> FactoryDigest {
    self.policy
  }
}

/// Append-only attempt to execute one program-selected Factory stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StageAttempt {
  id: StageAttemptId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  number: StageAttemptNumber,
  target: FactoryStageTarget,
  budget: BudgetLimit,
  input_digest: FactoryDigest,
  ownership: FactoryClaimOwnership,
}

impl StageAttempt {
  /// Constructs an attempt bound to the exact Run subject.
  #[must_use]
  pub fn new(
    id: StageAttemptId,
    run: &FactoryRun,
    number: StageAttemptNumber,
    target: FactoryStageTarget,
    budget: BudgetLimit,
    input_digest: FactoryDigest,
    ownership: FactoryClaimOwnership,
  ) -> Self {
    Self {
      id,
      run_id: run.id(),
      subject: run.subject().clone(),
      number,
      target,
      budget,
      input_digest,
      ownership,
    }
  }

  /// Returns the Stage Attempt identity.
  #[must_use]
  pub const fn id(&self) -> StageAttemptId {
    self.id
  }

  /// Returns the owning Run identity.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact immutable subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the append-only attempt number.
  #[must_use]
  pub const fn number(&self) -> StageAttemptNumber {
    self.number
  }

  /// Returns the stage kind.
  #[must_use]
  pub const fn kind(&self) -> FactoryStageKind {
    self.target.kind()
  }

  /// Returns the exact program-owned stage or evaluator branch target.
  #[must_use]
  pub const fn target(&self) -> &FactoryStageTarget {
    &self.target
  }

  /// Returns the immutable hard budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the canonical stage input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the reconciler owner that created this attempt.
  #[must_use]
  pub const fn owner(&self) -> &FactoryKey {
    self.ownership.owner()
  }

  /// Returns the fenced ownership window under which this attempt was created.
  #[must_use]
  pub const fn claim(&self) -> FactoryClaim {
    self.ownership.claim()
  }
}

/// Provider-neutral terminal outcome of one Stage Attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StageAttemptOutcome {
  /// The attempt produced its required immutable result.
  Succeeded,
  /// The attempt ended without a usable result.
  Failed,
  /// Cancellation was authoritatively observed.
  Cancelled,
}

impl StageAttemptOutcome {
  const fn as_str(self) -> &'static str {
    match self {
      Self::Succeeded => "succeeded",
      Self::Failed => "failed",
      Self::Cancelled => "cancelled",
    }
  }
}

/// Immutable terminal observation and consumed budget for one Stage Attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StageAttemptCompletion {
  id: FactoryDigest,
  stage_attempt_id: StageAttemptId,
  run_id: FactoryRunId,
  ownership: FactoryClaimOwnership,
  outcome: StageAttemptOutcome,
  usage: BudgetUsage,
  observed_at: Timestamp,
}

impl StageAttemptCompletion {
  /// Constructs a fenced terminal observation within the attempt's hard budget.
  pub fn new(
    stage: &StageAttempt,
    ownership: FactoryClaimOwnership,
    outcome: StageAttemptOutcome,
    usage: BudgetUsage,
    observed_at: Timestamp,
  ) -> Result<Self, FactoryError> {
    ownership.claim().verify_fence(ownership.claim().fence(), observed_at)?;
    usage.validate(stage.budget)?;
    let id = FactoryDigest::sha256(
      "octacity.factory.stage-attempt-completion.v1",
      &[
        stage.id.as_uuid().as_bytes(),
        ownership.owner().as_str().as_bytes(),
        &ownership.claim().fence().digest().as_bytes(),
        outcome.as_str().as_bytes(),
        &usage.attempts.to_be_bytes(),
        &usage.elapsed_millis.to_be_bytes(),
        &usage.tokens.to_be_bytes(),
        &usage.cost_micro_units.to_be_bytes(),
        &usage.output_bytes.to_be_bytes(),
        &observed_at.unix_millis().to_be_bytes(),
      ],
    );
    Ok(Self {
      id,
      stage_attempt_id: stage.id,
      run_id: stage.run_id,
      ownership,
      outcome,
      usage,
      observed_at,
    })
  }

  /// Returns the content-derived observation identity.
  #[must_use]
  pub const fn id(&self) -> FactoryDigest {
    self.id
  }

  /// Returns the completed Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the owning Factory Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the terminal observer identity.
  #[must_use]
  pub const fn owner(&self) -> &FactoryKey {
    self.ownership.owner()
  }

  /// Returns the fenced terminal observation window.
  #[must_use]
  pub const fn claim(&self) -> FactoryClaim {
    self.ownership.claim()
  }

  /// Returns the provider-neutral terminal outcome.
  #[must_use]
  pub const fn outcome(&self) -> StageAttemptOutcome {
    self.outcome
  }

  /// Returns consumed budget measured for this attempt.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Returns the authoritative terminal observation time.
  #[must_use]
  pub const fn observed_at(&self) -> Timestamp {
    self.observed_at
  }
}

/// One bounded node in a Stage Attempt's durable macro-call DAG.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MacroCall {
  id: MacroCallId,
  stage_attempt_id: StageAttemptId,
  run_id: FactoryRunId,
  subject: FactoryTaskSubject,
  kind: MacroCallKind,
  context_manifest_id: ContextManifestId,
  context_digest: FactoryDigest,
  budget: BudgetLimit,
  parent_id: Option<MacroCallId>,
  stage_dependencies: Vec<StageAttemptId>,
  call_dependencies: Vec<MacroCallId>,
  depth: u16,
}

/// Maximum number of declared stage or call dependencies for one macro call.
pub const MAX_MACRO_CALL_DEPENDENCIES: usize = 64;
/// Maximum depth of the durable macro-call DAG.
pub const MAX_MACRO_CALL_DEPTH: u16 = 32;

/// Deterministic program-owned declaration used to create one macro-call node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroCallDeclaration {
  id: MacroCallId,
  subject: FactoryTaskSubject,
  kind: MacroCallKind,
  budget: BudgetLimit,
  stage_dependencies: Vec<StageAttemptId>,
  call_dependencies: Vec<MacroCallId>,
  depth: u16,
}

impl MacroCallDeclaration {
  /// Canonicalizes the declared DAG edges before durable stage binding.
  pub fn new(
    id: MacroCallId,
    subject: FactoryTaskSubject,
    kind: MacroCallKind,
    budget: BudgetLimit,
    mut stage_dependencies: Vec<StageAttemptId>,
    mut call_dependencies: Vec<MacroCallId>,
    depth: u16,
  ) -> Result<Self, FactoryError> {
    stage_dependencies.sort_unstable();
    stage_dependencies.dedup();
    call_dependencies.sort_unstable();
    call_dependencies.dedup();
    validate_macro_call_shape(id, None, &stage_dependencies, &call_dependencies, depth)?;
    Ok(Self {
      id,
      subject,
      kind,
      budget,
      stage_dependencies,
      call_dependencies,
      depth,
    })
  }
}

impl MacroCall {
  /// Constructs a call and validates its optional parent belongs to the same stage and subject.
  pub fn new(
    declaration: MacroCallDeclaration,
    stage: &StageAttempt,
    context: &ContextManifest,
    parent: Option<&Self>,
  ) -> Result<Self, FactoryError> {
    validate_macro_call_shape(
      declaration.id,
      parent.map(|value| value.id),
      &declaration.stage_dependencies,
      &declaration.call_dependencies,
      declaration.depth,
    )?;
    if declaration.subject.exact() != &stage.subject || context.subject() != &declaration.subject {
      return Err(FactoryError::InvalidReference {
        relationship: "macro call context",
      });
    }
    if parent.is_some_and(|parent| {
      parent.id == declaration.id
        || parent.stage_attempt_id != stage.id
        || parent.run_id != stage.run_id
        || parent.subject != declaration.subject
        || !declaration.call_dependencies.contains(&parent.id)
        || parent.depth >= declaration.depth
    }) {
      return Err(FactoryError::InvalidReference {
        relationship: "macro call parent",
      });
    }
    Ok(Self {
      id: declaration.id,
      stage_attempt_id: stage.id,
      run_id: stage.run_id,
      subject: declaration.subject,
      kind: declaration.kind,
      context_manifest_id: context.id(),
      context_digest: context.digest()?,
      budget: declaration.budget,
      parent_id: parent.map(|value| value.id),
      stage_dependencies: declaration.stage_dependencies,
      call_dependencies: declaration.call_dependencies,
      depth: declaration.depth,
    })
  }

  /// Returns the call identity.
  #[must_use]
  pub const fn id(&self) -> MacroCallId {
    self.id
  }

  /// Returns the owning Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the owning Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact immutable subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the call purpose.
  #[must_use]
  pub const fn kind(&self) -> MacroCallKind {
    self.kind
  }

  /// Returns the immutable Context Manifest identity.
  #[must_use]
  pub const fn context_manifest_id(&self) -> ContextManifestId {
    self.context_manifest_id
  }

  /// Returns the exact Context Manifest digest.
  #[must_use]
  pub const fn context_digest(&self) -> FactoryDigest {
    self.context_digest
  }

  /// Returns the immutable call budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the optional parent call.
  #[must_use]
  pub const fn parent_id(&self) -> Option<MacroCallId> {
    self.parent_id
  }

  /// Returns declared predecessor Stage Attempts in canonical order.
  #[must_use]
  pub fn stage_dependencies(&self) -> &[StageAttemptId] {
    &self.stage_dependencies
  }

  /// Returns declared predecessor calls in canonical order.
  #[must_use]
  pub fn call_dependencies(&self) -> &[MacroCallId] {
    &self.call_dependencies
  }

  /// Returns this node's bounded one-based DAG depth.
  #[must_use]
  pub const fn depth(&self) -> u16 {
    self.depth
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MacroCallWire {
  id: MacroCallId,
  stage_attempt_id: StageAttemptId,
  run_id: FactoryRunId,
  subject: FactoryTaskSubject,
  kind: MacroCallKind,
  context_manifest_id: ContextManifestId,
  context_digest: FactoryDigest,
  budget: BudgetLimit,
  parent_id: Option<MacroCallId>,
  stage_dependencies: Vec<StageAttemptId>,
  call_dependencies: Vec<MacroCallId>,
  depth: u16,
}

impl<'de> Deserialize<'de> for MacroCall {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = MacroCallWire::deserialize(deserializer)?;
    validate_macro_call_shape(
      wire.id,
      wire.parent_id,
      &wire.stage_dependencies,
      &wire.call_dependencies,
      wire.depth,
    )
    .map_err(D::Error::custom)?;
    Ok(Self {
      id: wire.id,
      stage_attempt_id: wire.stage_attempt_id,
      run_id: wire.run_id,
      subject: wire.subject,
      kind: wire.kind,
      context_manifest_id: wire.context_manifest_id,
      context_digest: wire.context_digest,
      budget: wire.budget,
      parent_id: wire.parent_id,
      stage_dependencies: wire.stage_dependencies,
      call_dependencies: wire.call_dependencies,
      depth: wire.depth,
    })
  }
}

fn validate_macro_call_shape(
  id: MacroCallId,
  parent_id: Option<MacroCallId>,
  stage_dependencies: &[StageAttemptId],
  call_dependencies: &[MacroCallId],
  depth: u16,
) -> Result<(), FactoryError> {
  let stage_dependencies_are_canonical = stage_dependencies.windows(2).all(|values| values[0] < values[1]);
  let call_dependencies_are_canonical = call_dependencies.windows(2).all(|values| values[0] < values[1]);
  if stage_dependencies.len() > MAX_MACRO_CALL_DEPENDENCIES
    || call_dependencies.len() > MAX_MACRO_CALL_DEPENDENCIES
    || !stage_dependencies_are_canonical
    || !call_dependencies_are_canonical
    || call_dependencies.contains(&id)
    || parent_id.is_some_and(|parent| !call_dependencies.contains(&parent))
    || depth == 0
    || depth > MAX_MACRO_CALL_DEPTH
  {
    return Err(FactoryError::InvalidReference {
      relationship: "macro call dependencies",
    });
  }
  Ok(())
}

/// Provider-neutral request for one bounded, non-authoritative Decision Signal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalRequest {
  id: DecisionSignalRequestId,
  stage_attempt_id: StageAttemptId,
  run_id: FactoryRunId,
  subject: ExactSubject,
  purpose: DecisionSignalPurpose,
  input_digest: FactoryDigest,
  policy_digest: FactoryDigest,
  budget: BudgetLimit,
}

impl DecisionSignalRequest {
  /// Constructs a request bound to one exact Stage Attempt.
  #[must_use]
  pub fn new(
    id: DecisionSignalRequestId,
    stage: &StageAttempt,
    purpose: DecisionSignalPurpose,
    digests: DecisionSignalDigests,
    budget: BudgetLimit,
  ) -> Self {
    Self {
      id,
      stage_attempt_id: stage.id,
      run_id: stage.run_id,
      subject: stage.subject.clone(),
      purpose,
      input_digest: digests.input,
      policy_digest: digests.policy,
      budget,
    }
  }

  /// Returns the stable logical request identity.
  #[must_use]
  pub const fn id(&self) -> DecisionSignalRequestId {
    self.id
  }

  /// Returns the owning Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the owning Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the only seam in which this signal may be consumed.
  #[must_use]
  pub const fn purpose(&self) -> DecisionSignalPurpose {
    self.purpose
  }

  /// Returns the canonical redacted input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the immutable deterministic policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }

  /// Returns the immutable call budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
}

#[cfg(test)]
mod tests {
  use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId};

  use crate::{
    ContextManifestEntry, ContextSourceKind, ExternalWorkIdentity, FactoryArtifactReference, FactoryConfigurationId,
    FactoryConfigurationRef, FactoryConfigurationVersion, FactoryContextReference, FactoryMetadata, FactoryRunId,
    FactorySafeText, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
  };

  use super::*;

  fn digest(byte: u8) -> FactoryDigest {
    FactoryDigest::from_bytes([byte; 32])
  }

  fn artifact(byte: u8) -> FactoryArtifactReference {
    FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).expect("fixture artifact")
  }

  fn stage() -> StageAttempt {
    let subject = ExactSubject::new(
      ProjectId::generate(),
      RepositoryId::generate(),
      ImmutableRevision::new("base").expect("fixture revision"),
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
      ExternalWorkIdentity::new("source/1").expect("fixture identity"),
      subject,
      WorkArtifacts::new(artifact(90), artifact(91), vec![]).expect("fixture artifacts"),
      WorkClassification::new(
        WorkPriority::new(0).expect("fixture priority"),
        RiskClass::Low,
        FactoryMetadata::default(),
      ),
    )
    .expect("fixture work");
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    StageAttempt::new(
      StageAttemptId::generate(),
      &run,
      StageAttemptNumber::INITIAL,
      FactoryStageTarget::Implementation,
      BudgetLimit::new(1, 1, 1, 1, 1).expect("fixture budget"),
      digest(2),
      FactoryClaimOwnership::new(
        FactoryKey::new("worker").expect("fixture owner"),
        FactoryClaim::new(
          crate::FactoryClaimFence::new(digest(9)),
          octacity_server_domain::Timestamp::from_unix_millis(1).expect("fixture claim start"),
          octacity_server_domain::Timestamp::from_unix_millis(2).expect("fixture claim deadline"),
        )
        .expect("fixture claim"),
      ),
    )
  }

  fn context(stage: &StageAttempt, byte: u8) -> ContextManifest {
    let subject = FactoryTaskSubject::Exact(stage.subject().clone());
    ContextManifest::new(
      ContextManifestId::generate(),
      subject.clone(),
      digest(byte),
      vec![
        ContextManifestEntry::new(
          ContextSourceKind::Task,
          FactoryKey::new("task").unwrap(),
          subject,
          FactoryContextReference::Artifact(
            FactoryArtifactReference::new(ArtifactId::generate(), digest(byte), 1).unwrap(),
          ),
          digest(byte),
          1,
          FactorySafeText::new("required task").unwrap(),
          digest(byte),
        )
        .unwrap(),
      ],
    )
    .unwrap()
  }

  #[test]
  fn macro_call_rejects_a_parent_from_another_stage() {
    let first_stage = stage();
    let first_context = context(&first_stage, 3);
    let parent = MacroCall::new(
      MacroCallDeclaration::new(
        MacroCallId::generate(),
        first_context.subject().clone(),
        MacroCallKind::Implement,
        first_stage.budget(),
        vec![],
        vec![],
        1,
      )
      .unwrap(),
      &first_stage,
      &first_context,
      None,
    )
    .expect("fixture parent");
    let second_stage = stage();
    let second_context = context(&second_stage, 4);
    assert!(matches!(
      MacroCall::new(
        MacroCallDeclaration::new(
          MacroCallId::generate(),
          second_context.subject().clone(),
          MacroCallKind::Summarize,
          first_stage.budget(),
          vec![],
          vec![parent.id()],
          2,
        )
        .unwrap(),
        &second_stage,
        &second_context,
        Some(&parent),
      ),
      Err(FactoryError::InvalidReference { .. })
    ));
  }

  #[test]
  fn persisted_macro_call_rejects_unknown_fields_and_invalid_graph_shape() {
    let stage = stage();
    let context = context(&stage, 5);
    let call = MacroCall::new(
      MacroCallDeclaration::new(
        MacroCallId::generate(),
        context.subject().clone(),
        MacroCallKind::Implement,
        stage.budget(),
        vec![],
        vec![],
        1,
      )
      .unwrap(),
      &stage,
      &context,
      None,
    )
    .unwrap();
    let mut unknown = serde_json::to_value(&call).unwrap();
    unknown
      .as_object_mut()
      .unwrap()
      .insert("transcript".into(), "hidden".into());
    assert!(serde_json::from_value::<MacroCall>(unknown).is_err());

    let mut invalid_depth = serde_json::to_value(&call).unwrap();
    invalid_depth.as_object_mut().unwrap().insert("depth".into(), 0.into());
    assert!(serde_json::from_value::<MacroCall>(invalid_depth).is_err());
  }

  #[test]
  fn stage_completion_requires_a_live_fence_and_stays_within_the_stage_budget() {
    let stage = stage();
    let ownership = FactoryClaimOwnership::new(
      FactoryKey::new("completion-worker").expect("fixture owner"),
      FactoryClaim::new(
        crate::FactoryClaimFence::new(digest(10)),
        Timestamp::from_unix_millis(10).expect("fixture claim start"),
        Timestamp::from_unix_millis(20).expect("fixture claim deadline"),
      )
      .expect("fixture claim"),
    );

    assert!(
      StageAttemptCompletion::new(
        &stage,
        ownership.clone(),
        StageAttemptOutcome::Succeeded,
        BudgetUsage::default(),
        Timestamp::from_unix_millis(15).expect("fixture observation"),
      )
      .is_ok()
    );
    assert!(
      StageAttemptCompletion::new(
        &stage,
        ownership.clone(),
        StageAttemptOutcome::Succeeded,
        BudgetUsage {
          tokens: 2,
          ..BudgetUsage::default()
        },
        Timestamp::from_unix_millis(15).expect("fixture observation"),
      )
      .is_err()
    );
    assert!(
      StageAttemptCompletion::new(
        &stage,
        ownership,
        StageAttemptOutcome::Succeeded,
        BudgetUsage::default(),
        Timestamp::from_unix_millis(20).expect("fixture expired observation"),
      )
      .is_err()
    );
  }
}
