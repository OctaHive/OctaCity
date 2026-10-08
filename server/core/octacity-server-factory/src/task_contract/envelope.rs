use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  BudgetLimit, CommandArgumentPattern, ContextManifestId, FactoryDigest, FactoryError, FactoryKey,
  FactoryPermissionSet, FactorySafeText, ImmutableReference, StageAttemptId, TaskEnvelopeId,
};

use super::{common::*, context::ContextManifest};

/// One output path and kind required from a Factory task.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FactoryTaskDeliverable {
  kind: FactoryKey,
  path: FactoryRepositoryPath,
  required: bool,
}

impl FactoryTaskDeliverable {
  /// Constructs one typed task deliverable.
  #[must_use]
  pub const fn new(kind: FactoryKey, path: FactoryRepositoryPath, required: bool) -> Self {
    Self { kind, path, required }
  }

  /// Returns the stable output kind.
  #[must_use]
  pub const fn kind(&self) -> &FactoryKey {
    &self.kind
  }

  /// Returns the protected output-relative path.
  #[must_use]
  pub const fn path(&self) -> &FactoryRepositoryPath {
    &self.path
  }

  /// Reports whether absence makes the task result invalid.
  #[must_use]
  pub const fn required(&self) -> bool {
    self.required
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryTaskDeliverableWire {
  kind: FactoryKey,
  path: FactoryRepositoryPath,
  required: bool,
}

impl<'de> Deserialize<'de> for FactoryTaskDeliverable {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryTaskDeliverableWire::deserialize(deserializer)?;
    Ok(Self::new(wire.kind, wire.path, wire.required))
  }
}

/// Exact immutable digests controlling one Factory task invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryTaskDigests {
  /// Effective policy digest.
  pub policy: FactoryDigest,
  /// Rendered protected prompt digest.
  pub prompt: FactoryDigest,
  /// BLAKE3 prompt identity emitted by the pinned harness provenance record.
  pub prompt_provenance: FactoryDigest,
  /// Canonical complete input digest.
  pub input: FactoryDigest,
  /// Canonical declared-deliverable digest.
  pub deliverable: FactoryDigest,
  /// Canonical Task Envelope permission-set digest.
  pub permission: FactoryDigest,
  /// Canonical hard-budget digest.
  pub budget: FactoryDigest,
  /// Provenance record digest.
  pub provenance: FactoryDigest,
}

/// Stable identities and subject that declare one immutable task invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryTaskDeclaration {
  /// Stable Task Envelope identity.
  pub id: TaskEnvelopeId,
  /// Owning Stage Attempt.
  pub stage_attempt_id: StageAttemptId,
  /// Exact immutable task subject.
  pub subject: FactoryTaskSubject,
  /// Program-owned task mode.
  pub mode: FactoryTaskMode,
}

/// Bounded task inputs and expected outputs controlled by Factory policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryTaskDefinition {
  /// Exact task-body Artifact.
  pub task: FactoryArtifactReference,
  /// Exact specification Artifacts.
  pub specifications: Vec<FactoryArtifactReference>,
  /// Typed findings explicitly selected for this task.
  pub prior_findings: Vec<FactoryDigest>,
  /// Maximum authority carried by the task.
  pub permissions: FactoryPermissionSet,
  /// Hard invocation budget.
  pub budget: BudgetLimit,
  /// Strict expected result schema.
  pub result_schema: FactoryTaskResultSchema,
  /// Required generic outputs.
  pub deliverables: Vec<FactoryTaskDeliverable>,
}

/// Exact released toolchain selected for one task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryTaskToolchain {
  /// Exact Octa plugin identity.
  pub plugin: ImmutableReference,
  /// Exact executable identity.
  pub executable: ImmutableReference,
  /// Exact model identity.
  pub model: ImmutableReference,
}

/// Digests supplied by trusted policy and compilation boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryTaskControlDigests {
  /// Effective policy digest.
  pub policy: FactoryDigest,
  /// Rendered protected prompt digest.
  pub prompt: FactoryDigest,
  /// BLAKE3 prompt identity expected in producer provenance.
  pub prompt_provenance: FactoryDigest,
  /// Effective provenance-policy digest.
  pub provenance: FactoryDigest,
}

/// Immutable versioned task definition compiled at the trusted boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryTaskEnvelope {
  schema_version: u16,
  id: TaskEnvelopeId,
  stage_attempt_id: StageAttemptId,
  subject: FactoryTaskSubject,
  mode: FactoryTaskMode,
  task: FactoryArtifactReference,
  specifications: Vec<FactoryArtifactReference>,
  context_manifest_id: ContextManifestId,
  context_manifest_digest: FactoryDigest,
  prior_findings: Vec<FactoryDigest>,
  permissions: FactoryPermissionSet,
  budget: BudgetLimit,
  result_schema: FactoryTaskResultSchema,
  deliverables: Vec<FactoryTaskDeliverable>,
  plugin: ImmutableReference,
  executable: ImmutableReference,
  model: ImmutableReference,
  digests: FactoryTaskDigests,
}

impl FactoryTaskEnvelope {
  /// Constructs a strict task definition from one frozen Context Manifest.
  pub fn new(
    declaration: FactoryTaskDeclaration,
    mut definition: FactoryTaskDefinition,
    context_manifest: &ContextManifest,
    toolchain: FactoryTaskToolchain,
    control_digests: FactoryTaskControlDigests,
  ) -> Result<Self, FactoryError> {
    validate_task_mode(declaration.mode, definition.result_schema, &declaration.subject)?;
    validate_permission_content(&definition.permissions)?;
    if context_manifest.subject() != &declaration.subject {
      return Err(FactoryError::InconsistentSubject);
    }
    validate_count(&definition.specifications, 0, MAX_TASK_ARTIFACTS, "task specifications")?;
    definition.specifications.sort();
    reject_duplicates(&definition.specifications, "task specifications")?;
    if definition
      .specifications
      .iter()
      .any(|reference| reference.artifact_id() == definition.task.artifact_id())
    {
      return Err(invalid("task specification artifact"));
    }
    validate_count(
      &definition.prior_findings,
      0,
      MAX_TASK_PRIOR_FINDINGS,
      "task prior findings",
    )?;
    definition.prior_findings.sort_unstable();
    reject_duplicates(&definition.prior_findings, "task prior findings")?;
    validate_count(&definition.deliverables, 1, MAX_TASK_DELIVERABLES, "task deliverables")?;
    definition.deliverables.sort();
    reject_duplicates(&definition.deliverables, "task deliverables")?;

    let context_manifest_digest = context_manifest.digest()?;
    let permission_digest = definition.permissions.digest();
    let budget_digest = record_digest("octacity.factory.task-budget.v1", &definition.budget)?;
    let deliverable_digest = record_digest("octacity.factory.task-deliverables.v1", &definition.deliverables)?;
    let input_digest = task_input_digest(
      &declaration.subject,
      &definition.task,
      &definition.specifications,
      context_manifest_digest,
      &definition.prior_findings,
    )?;
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      id: declaration.id,
      stage_attempt_id: declaration.stage_attempt_id,
      subject: declaration.subject,
      mode: declaration.mode,
      task: definition.task,
      specifications: definition.specifications,
      context_manifest_id: context_manifest.id(),
      context_manifest_digest,
      prior_findings: definition.prior_findings,
      permissions: definition.permissions,
      budget: definition.budget,
      result_schema: definition.result_schema,
      deliverables: definition.deliverables,
      plugin: toolchain.plugin,
      executable: toolchain.executable,
      model: toolchain.model,
      digests: FactoryTaskDigests {
        policy: control_digests.policy,
        prompt: control_digests.prompt,
        prompt_provenance: control_digests.prompt_provenance,
        input: input_digest,
        deliverable: deliverable_digest,
        permission: permission_digest,
        budget: budget_digest,
        provenance: control_digests.provenance,
      },
    })
  }

  /// Returns the immutable envelope identity.
  #[must_use]
  pub const fn id(&self) -> TaskEnvelopeId {
    self.id
  }

  /// Returns the owning Stage Attempt.
  #[must_use]
  pub const fn stage_attempt_id(&self) -> StageAttemptId {
    self.stage_attempt_id
  }

  /// Returns the exact task subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the program-owned task mode.
  #[must_use]
  pub const fn mode(&self) -> FactoryTaskMode {
    self.mode
  }

  /// Returns the exact task-body Artifact reference.
  #[must_use]
  pub const fn task(&self) -> &FactoryArtifactReference {
    &self.task
  }

  /// Returns exact specification Artifact references in canonical order.
  #[must_use]
  pub fn specifications(&self) -> &[FactoryArtifactReference] {
    &self.specifications
  }

  /// Returns the immutable Context Manifest identity.
  #[must_use]
  pub const fn context_manifest_id(&self) -> ContextManifestId {
    self.context_manifest_id
  }

  /// Returns the exact Context Manifest digest.
  #[must_use]
  pub const fn context_manifest_digest(&self) -> FactoryDigest {
    self.context_manifest_digest
  }

  /// Returns prior typed finding digests in canonical order.
  #[must_use]
  pub fn prior_findings(&self) -> &[FactoryDigest] {
    &self.prior_findings
  }

  /// Returns the maximum authority carried by this task.
  #[must_use]
  pub const fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }

  /// Returns the immutable task budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }

  /// Returns the strict expected result schema.
  #[must_use]
  pub const fn result_schema(&self) -> FactoryTaskResultSchema {
    self.result_schema
  }

  /// Returns declared outputs in canonical order.
  #[must_use]
  pub fn deliverables(&self) -> &[FactoryTaskDeliverable] {
    &self.deliverables
  }

  /// Returns the exact selected Octa plugin identity.
  #[must_use]
  pub const fn plugin(&self) -> &ImmutableReference {
    &self.plugin
  }

  /// Returns the exact selected executable identity.
  #[must_use]
  pub const fn executable(&self) -> &ImmutableReference {
    &self.executable
  }

  /// Returns the exact selected model identity.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns all exact controlling digests.
  #[must_use]
  pub const fn digests(&self) -> FactoryTaskDigests {
    self.digests
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "task envelope serialization")
  }

  /// Returns the content-addressed canonical envelope digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.task-envelope.v1", self)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryTaskEnvelopeWire {
  schema_version: u16,
  id: TaskEnvelopeId,
  stage_attempt_id: StageAttemptId,
  subject: FactoryTaskSubject,
  mode: FactoryTaskMode,
  task: FactoryArtifactReference,
  specifications: Vec<FactoryArtifactReference>,
  context_manifest_id: ContextManifestId,
  context_manifest_digest: FactoryDigest,
  prior_findings: Vec<FactoryDigest>,
  permissions: FactoryPermissionSet,
  budget: BudgetLimit,
  result_schema: FactoryTaskResultSchema,
  deliverables: Vec<FactoryTaskDeliverable>,
  plugin: ImmutableReference,
  executable: ImmutableReference,
  model: ImmutableReference,
  digests: FactoryTaskDigests,
}

impl<'de> Deserialize<'de> for FactoryTaskEnvelope {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = FactoryTaskEnvelopeWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    validate_task_mode(wire.mode, wire.result_schema, &wire.subject).map_err(D::Error::custom)?;
    validate_envelope_wire(&wire).map_err(D::Error::custom)?;
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      id: wire.id,
      stage_attempt_id: wire.stage_attempt_id,
      subject: wire.subject,
      mode: wire.mode,
      task: wire.task,
      specifications: wire.specifications,
      context_manifest_id: wire.context_manifest_id,
      context_manifest_digest: wire.context_manifest_digest,
      prior_findings: wire.prior_findings,
      permissions: wire.permissions,
      budget: wire.budget,
      result_schema: wire.result_schema,
      deliverables: wire.deliverables,
      plugin: wire.plugin,
      executable: wire.executable,
      model: wire.model,
      digests: wire.digests,
    })
  }
}

fn validate_task_mode(
  mode: FactoryTaskMode,
  result_schema: FactoryTaskResultSchema,
  subject: &FactoryTaskSubject,
) -> Result<(), FactoryError> {
  let valid = matches!(
    (mode, result_schema, subject),
    (
      FactoryTaskMode::Implement,
      FactoryTaskResultSchema::ImplementationV1,
      FactoryTaskSubject::Exact(_)
    ) | (
      FactoryTaskMode::Rework,
      FactoryTaskResultSchema::ImplementationV1,
      FactoryTaskSubject::Candidate(_)
    ) | (
      FactoryTaskMode::Evaluate,
      FactoryTaskResultSchema::EvaluationV1,
      FactoryTaskSubject::Candidate(_)
    )
  );
  if valid {
    Ok(())
  } else {
    Err(invalid("task mode, result schema, and subject"))
  }
}

fn validate_envelope_wire(wire: &FactoryTaskEnvelopeWire) -> Result<(), FactoryError> {
  validate_permission_content(&wire.permissions)?;
  validate_count(&wire.specifications, 0, MAX_TASK_ARTIFACTS, "task specifications")?;
  validate_count(&wire.prior_findings, 0, MAX_TASK_PRIOR_FINDINGS, "task prior findings")?;
  validate_count(&wire.deliverables, 1, MAX_TASK_DELIVERABLES, "task deliverables")?;
  if !strictly_sorted(&wire.specifications)
    || !strictly_sorted(&wire.prior_findings)
    || !strictly_sorted(&wire.deliverables)
  {
    return Err(invalid("task canonical ordering"));
  }
  if wire
    .specifications
    .iter()
    .any(|reference| reference.artifact_id() == wire.task.artifact_id())
  {
    return Err(invalid("task specification artifact"));
  }
  let expected_permission = wire.permissions.digest();
  let expected_budget = record_digest("octacity.factory.task-budget.v1", &wire.budget)?;
  let expected_deliverables = record_digest("octacity.factory.task-deliverables.v1", &wire.deliverables)?;
  let expected_input = task_input_digest(
    &wire.subject,
    &wire.task,
    &wire.specifications,
    wire.context_manifest_digest,
    &wire.prior_findings,
  )?;
  if wire.digests.permission != expected_permission
    || wire.digests.budget != expected_budget
    || wire.digests.deliverable != expected_deliverables
    || wire.digests.input != expected_input
  {
    return Err(invalid("task derived digest"));
  }
  Ok(())
}

fn validate_permission_content(permissions: &FactoryPermissionSet) -> Result<(), FactoryError> {
  for argument in permissions.commands().flat_map(|command| command.arguments()) {
    if let CommandArgumentPattern::Exact(value) = argument {
      FactorySafeText::new(value.as_str()).map_err(|_| invalid("permission command argument"))?;
    }
  }
  Ok(())
}

fn task_input_digest(
  subject: &FactoryTaskSubject,
  task: &FactoryArtifactReference,
  specifications: &[FactoryArtifactReference],
  context_manifest_digest: FactoryDigest,
  prior_findings: &[FactoryDigest],
) -> Result<FactoryDigest, FactoryError> {
  #[derive(Serialize)]
  struct Input<'a> {
    subject: &'a FactoryTaskSubject,
    task: &'a FactoryArtifactReference,
    specifications: &'a [FactoryArtifactReference],
    context_manifest_digest: FactoryDigest,
    prior_findings: &'a [FactoryDigest],
  }
  record_digest(
    "octacity.factory.task-input.v1",
    &Input {
      subject,
      task,
      specifications,
      context_manifest_digest,
      prior_findings,
    },
  )
}
