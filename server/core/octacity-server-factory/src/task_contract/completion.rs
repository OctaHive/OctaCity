use octacity_server_domain::Timestamp;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{BudgetUsage, FactoryDigest, FactoryError, ImmutableReference, MacroCall, MacroCallId, TaskEnvelopeId};

use super::{
  BoundedSummary, FACTORY_TASK_CONTRACT_VERSION, FactoryArtifactReference, FactoryTaskEnvelope,
  FactoryTaskResultSchema, FactoryTaskSubject, canonical_json, invalid, record_digest, require_version,
};

/// Stable terminal classification for one durable model invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroCallTerminal {
  /// Every required output was verified and projected successfully.
  Succeeded,
  /// The signed execution deadline elapsed.
  TimedOut,
  /// Cancellation became terminal.
  Cancelled,
  /// The selected provider or provider transport was unavailable.
  ProviderUnavailable,
  /// Output collection exceeded an immutable byte bound.
  OutputOverflow,
  /// A required declared output was absent.
  MissingDeliverable,
  /// Retained output bytes were not valid JSON.
  InvalidJson,
  /// Retained output violated the pinned schema.
  InvalidSchema,
  /// Retained bytes disagreed with their immutable identity.
  IntegrityFailure,
  /// Another immutable task budget was exceeded.
  BudgetExceeded,
  /// The ordinary Build reported an execution failure without a narrower trusted classification.
  ExecutionFailed,
  /// The ordinary Build reported an infrastructure failure without a narrower trusted classification.
  InfrastructureFailed,
}

impl MacroCallTerminal {
  /// Returns the stable storage and diagnostics label.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Succeeded => "succeeded",
      Self::TimedOut => "timed_out",
      Self::Cancelled => "cancelled",
      Self::ProviderUnavailable => "provider_unavailable",
      Self::OutputOverflow => "output_overflow",
      Self::MissingDeliverable => "missing_deliverable",
      Self::InvalidJson => "invalid_json",
      Self::InvalidSchema => "invalid_schema",
      Self::IntegrityFailure => "integrity_failure",
      Self::BudgetExceeded => "budget_exceeded",
      Self::ExecutionFailed => "execution_failed",
      Self::InfrastructureFailed => "infrastructure_failed",
    }
  }
}

/// Exact retained outputs accepted for one terminal macro call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MacroCallCompletionOutputs {
  /// Exact typed result Artifact.
  pub result: Option<FactoryArtifactReference>,
  /// Bounded successor-facing summary.
  pub summary: Option<BoundedSummary>,
  /// Exact Artifact containing the canonical bounded summary bytes.
  pub summary_artifact: Option<FactoryArtifactReference>,
  /// Exact sanitized trace Artifact.
  pub trace: Option<FactoryArtifactReference>,
  /// Exact producer provenance Artifact.
  pub provenance: Option<FactoryArtifactReference>,
}

/// Immutable terminal result bound to one durable macro-call node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MacroCallCompletion {
  schema_version: u16,
  call_id: MacroCallId,
  subject: FactoryTaskSubject,
  task_envelope_id: TaskEnvelopeId,
  task_envelope_digest: FactoryDigest,
  plugin: ImmutableReference,
  executable: ImmutableReference,
  model: ImmutableReference,
  prompt_digest: FactoryDigest,
  result_schema: FactoryTaskResultSchema,
  terminal: MacroCallTerminal,
  usage: BudgetUsage,
  result: Option<FactoryArtifactReference>,
  summary: Option<BoundedSummary>,
  summary_artifact: Option<FactoryArtifactReference>,
  trace: Option<FactoryArtifactReference>,
  provenance: Option<FactoryArtifactReference>,
  completed_at: Timestamp,
}

impl MacroCallCompletion {
  /// Binds a trusted terminal observation to the exact call and Task Envelope.
  pub fn new(
    call: &MacroCall,
    envelope: &FactoryTaskEnvelope,
    terminal: MacroCallTerminal,
    usage: BudgetUsage,
    outputs: MacroCallCompletionOutputs,
    completed_at: Timestamp,
  ) -> Result<Self, FactoryError> {
    if call.stage_attempt_id() != envelope.stage_attempt_id() || call.subject() != envelope.subject() {
      return Err(FactoryError::InvalidReference {
        relationship: "macro call task envelope",
      });
    }
    if !terminal.accepts_usage(usage, call.budget()) {
      return Err(FactoryError::InvalidTaskContract {
        field: "macro call completion usage",
      });
    }
    validate_outputs(terminal, call.subject(), envelope.result_schema(), &outputs)?;
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      call_id: call.id(),
      subject: call.subject().clone(),
      task_envelope_id: envelope.id(),
      task_envelope_digest: envelope.digest()?,
      plugin: envelope.plugin().clone(),
      executable: envelope.executable().clone(),
      model: envelope.model().clone(),
      prompt_digest: envelope.digests().prompt,
      result_schema: envelope.result_schema(),
      terminal,
      usage,
      result: outputs.result,
      summary: outputs.summary,
      summary_artifact: outputs.summary_artifact,
      trace: outputs.trace,
      provenance: outputs.provenance,
      completed_at,
    })
  }

  /// Returns the completed macro call.
  #[must_use]
  pub const fn call_id(&self) -> MacroCallId {
    self.call_id
  }

  /// Returns the exact call subject.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }

  /// Returns the consumed Task Envelope identity.
  #[must_use]
  pub const fn task_envelope_id(&self) -> TaskEnvelopeId {
    self.task_envelope_id
  }

  /// Returns the consumed canonical Task Envelope digest.
  #[must_use]
  pub const fn task_envelope_digest(&self) -> FactoryDigest {
    self.task_envelope_digest
  }

  /// Returns the exact selected plugin.
  #[must_use]
  pub const fn plugin(&self) -> &ImmutableReference {
    &self.plugin
  }

  /// Returns the exact selected executable.
  #[must_use]
  pub const fn executable(&self) -> &ImmutableReference {
    &self.executable
  }

  /// Returns the exact selected model.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns the frozen protected prompt digest.
  #[must_use]
  pub const fn prompt_digest(&self) -> FactoryDigest {
    self.prompt_digest
  }

  /// Returns the pinned typed result schema.
  #[must_use]
  pub const fn result_schema(&self) -> FactoryTaskResultSchema {
    self.result_schema
  }

  /// Returns the trusted terminal classification.
  #[must_use]
  pub const fn terminal(&self) -> MacroCallTerminal {
    self.terminal
  }

  /// Returns immutable measured usage.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Validates measured usage against the call budget and terminal cause.
  #[must_use]
  pub fn usage_is_valid_for(&self, budget: crate::BudgetLimit) -> bool {
    self.terminal.accepts_usage(self.usage, budget)
  }

  /// Returns the exact typed result Artifact, when accepted.
  #[must_use]
  pub const fn result(&self) -> Option<&FactoryArtifactReference> {
    self.result.as_ref()
  }

  /// Returns the bounded summary, when accepted.
  #[must_use]
  pub const fn summary(&self) -> Option<&BoundedSummary> {
    self.summary.as_ref()
  }

  /// Returns the exact canonical summary Artifact, when accepted.
  #[must_use]
  pub const fn summary_artifact(&self) -> Option<&FactoryArtifactReference> {
    self.summary_artifact.as_ref()
  }

  /// Returns the sanitized trace Artifact, when retained.
  #[must_use]
  pub const fn trace(&self) -> Option<&FactoryArtifactReference> {
    self.trace.as_ref()
  }

  /// Returns the exact provenance Artifact, when retained.
  #[must_use]
  pub const fn provenance(&self) -> Option<&FactoryArtifactReference> {
    self.provenance.as_ref()
  }

  /// Returns the authoritative observation time.
  #[must_use]
  pub const fn completed_at(&self) -> Timestamp {
    self.completed_at
  }

  /// Returns deterministic canonical JSON bytes.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "macro call completion serialization")
  }

  /// Returns the content-addressed canonical completion digest.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.macro-call-completion.v1", self)
  }
}

impl MacroCallTerminal {
  fn accepts_usage(self, usage: BudgetUsage, budget: crate::BudgetLimit) -> bool {
    usage.validate(budget).is_ok() || matches!(self, Self::BudgetExceeded | Self::OutputOverflow)
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MacroCallCompletionWire {
  schema_version: u16,
  call_id: MacroCallId,
  subject: FactoryTaskSubject,
  task_envelope_id: TaskEnvelopeId,
  task_envelope_digest: FactoryDigest,
  plugin: ImmutableReference,
  executable: ImmutableReference,
  model: ImmutableReference,
  prompt_digest: FactoryDigest,
  result_schema: FactoryTaskResultSchema,
  terminal: MacroCallTerminal,
  usage: BudgetUsage,
  result: Option<FactoryArtifactReference>,
  summary: Option<BoundedSummary>,
  summary_artifact: Option<FactoryArtifactReference>,
  trace: Option<FactoryArtifactReference>,
  provenance: Option<FactoryArtifactReference>,
  completed_at: Timestamp,
}

impl<'de> Deserialize<'de> for MacroCallCompletion {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = MacroCallCompletionWire::deserialize(deserializer)?;
    require_version(wire.schema_version).map_err(D::Error::custom)?;
    let outputs = MacroCallCompletionOutputs {
      result: wire.result,
      summary: wire.summary,
      summary_artifact: wire.summary_artifact,
      trace: wire.trace,
      provenance: wire.provenance,
    };
    validate_outputs(wire.terminal, &wire.subject, wire.result_schema, &outputs).map_err(D::Error::custom)?;
    Ok(Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      call_id: wire.call_id,
      subject: wire.subject,
      task_envelope_id: wire.task_envelope_id,
      task_envelope_digest: wire.task_envelope_digest,
      plugin: wire.plugin,
      executable: wire.executable,
      model: wire.model,
      prompt_digest: wire.prompt_digest,
      result_schema: wire.result_schema,
      terminal: wire.terminal,
      usage: wire.usage,
      result: outputs.result,
      summary: outputs.summary,
      summary_artifact: outputs.summary_artifact,
      trace: outputs.trace,
      provenance: outputs.provenance,
      completed_at: wire.completed_at,
    })
  }
}

fn validate_outputs(
  terminal: MacroCallTerminal,
  subject: &FactoryTaskSubject,
  result_schema: FactoryTaskResultSchema,
  outputs: &MacroCallCompletionOutputs,
) -> Result<(), FactoryError> {
  if terminal != MacroCallTerminal::Succeeded {
    if outputs.result.is_some() || outputs.summary.is_some() || outputs.summary_artifact.is_some() {
      return Err(invalid("unsuccessful macro call typed output"));
    }
    return Ok(());
  }
  let (Some(_result), Some(summary), Some(_trace), Some(_provenance)) = (
    outputs.result.as_ref(),
    outputs.summary.as_ref(),
    outputs.trace.as_ref(),
    outputs.provenance.as_ref(),
  ) else {
    return Err(invalid("successful macro call output"));
  };
  if summary.subject() != subject {
    return Err(FactoryError::InconsistentSubject);
  }
  if result_schema == FactoryTaskResultSchema::ImplementationV1 && outputs.summary_artifact.is_none() {
    return Err(invalid("successful macro call output"));
  }
  if let Some(summary_artifact) = &outputs.summary_artifact {
    let bytes = summary.canonical_bytes()?;
    if summary_artifact.content_digest() != FactoryDigest::content_sha256(&bytes)
      || summary_artifact.encoded_size()
        != u64::try_from(bytes.len()).map_err(|_| invalid("macro call summary bytes"))?
    {
      return Err(invalid("macro call summary artifact"));
    }
  }
  Ok(())
}
