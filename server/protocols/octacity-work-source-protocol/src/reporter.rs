//! Independently versioned projection, without Factory or Delivery authority.
use crate::*;

/// Supported Reporter version; Source and Delivery negotiate independently.
pub const PROTOCOL_VERSION: u16 = 1;
/// Single-write planning contract; the host checks durable authority between steps.
pub const ORDERED_PROTOCOL_VERSION: u16 = 2;
/// Exact request to observe/project one original accepted status intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
  /// Selected Reporter version.
  pub protocol_version: u16,
  /// Process response correlation.
  pub request_id: String,
  /// Closed configured reporting operation.
  pub command: Command,
}
/// Reporter never accepts arbitrary provider mutations, model prose or Flow transitions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// Read-only installed connection/resource readiness, without execution inputs.
  Check(check::CheckIntent),
  /// Read current effects and opaque, conditional single-write steps.
  Inspect(Box<ProjectionIntent>),
  /// Execute at most one previously inspected conditional native write.
  Apply(Box<StepRequest>),
  /// Inspect current effects before retrying an uncertain projection.
  Observe(Box<ProjectionIntent>),
  /// Converge only the pinned configured effects for the original item.
  Project(Box<ProjectionIntent>),
  /// Stops the original projection without creating missing effects.
  Cancel {
    /// Original trusted projection identity.
    target_projection_id: String,
  },
}
impl Command {
  /// Original immutable projection intent; checks and cancellations have no task projection.
  pub fn intent(&self) -> Option<&ProjectionIntent> {
    match self {
      Self::Observe(intent) | Self::Project(intent) | Self::Inspect(intent) => Some(intent),
      Self::Apply(request) => Some(&request.intent),
      Self::Cancel { .. } | Self::Check(_) => None,
    }
  }
}
/// One provider-owned conditional write, with no native DTOs in the host.
/// Every step applies only a subset of the pinned desired effects. Steps of
/// the same original projection must commute, including late uncertain writes;
/// temporary or nonconvergent route changes cannot use this contract.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionStep {
  /// Canonical opaque identities of all overlapping managed fields affected by this write.
  pub fields: Vec<String>,
  /// Private bounded provider token, pinned to the original intent and observed managed values.
  pub token: String,
}
impl std::fmt::Debug for ProjectionStep {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ProjectionStep")
      .field("fields", &self.fields)
      .finish_non_exhaustive()
  }
}
impl ProjectionStep {
  /// Validates bounds without interpreting the provider's private token.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    fields(&self.fields, false)?;
    common::require_bounded_text(&self.token, 8192).map_err(|_| ProtocolError::Invalid)
  }
}
/// A single write authorized by the host after checking the current outbox fence and order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StepRequest {
  /// Frozen accepted operation and provider scope.
  pub intent: ProjectionIntent,
  /// Exact conditional step returned by inspection.
  pub step: ProjectionStep,
}
/// Bounded read-only plan. A plan is provider data, never permission to write.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionPlan {
  /// Exact original scope and observed effect match.
  pub receipt: ProjectionReceipt,
  /// Opaque identities, stable across mappings/installations for the same native item.
  /// Namespace the native provider instance/container/item to isolate coincident ids.
  pub fields: Vec<String>,
  /// Each entry performs at most one write after a separate host authorization.
  pub steps: Vec<ProjectionStep>,
}
impl ProjectionPlan {
  /// Rejects substitutions, unbounded plans and steps outside the declared field scope.
  pub fn validate_for(&self, intent: &ProjectionIntent) -> Result<(), ProtocolError> {
    self.receipt.validate_for(intent)?;
    fields(&self.fields, true)?;
    if self.steps.len() > 256
      || self.receipt.matches != self.steps.is_empty()
      || self.steps.iter().map(|step| step.token.len()).sum::<usize>() > 262_144
    {
      return Err(ProtocolError::Invalid);
    }
    let mut tokens = std::collections::BTreeSet::new();
    for step in &self.steps {
      step.validate()?;
      if !tokens.insert(&step.token) || step.fields.iter().any(|field| !self.fields.contains(field)) {
        return Err(ProtocolError::Invalid);
      }
    }
    Ok(())
  }
}
fn fields(values: &[String], empty: bool) -> Result<(), ProtocolError> {
  if (!empty && values.is_empty()) || values.len() > if empty { 512 } else { 256 } {
    return Err(ProtocolError::Invalid);
  }
  let mut seen = std::collections::BTreeSet::new();
  for value in values {
    digest(value)?;
    if !seen.insert(value) {
      return Err(ProtocolError::Invalid);
    }
  }
  Ok(())
}
/// Frozen logical projection; native labels/transitions stay in the installed mapping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionIntent {
  /// Stable original outbox operation identity, independent of process retries.
  pub projection_id: String,
  /// Exact operator profile, excluding rotated secret bytes.
  pub configuration_digest: String,
  /// Original authorized ticket container and separate write grant.
  pub resource: ResourceAccess,
  /// Original native stable item identity.
  pub item_id: String,
  /// Original human-facing number/key, verified by the provider against native identity.
  pub display_id: String,
  /// Exact installed desired-effects mapping.
  pub mapping: ConfigurationReference,
  /// Arbitrary configured status value; no built-in business vocabulary.
  pub status: String,
  /// Exact trusted network ceiling, checked by the provider before credentials.
  pub network_hosts: Vec<String>,
}
impl ProjectionIntent {
  /// Checks immutable scope without resolving credentials or interpreting provider state.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    digest(&self.projection_id)?;
    digest(&self.configuration_digest)?;
    self.resource.validate(CredentialClass::WorkReporterWrite)?;
    self.mapping.validate()?;
    field(&self.item_id)?;
    common::require_bounded_text(&self.display_id, 256).map_err(|_| ProtocolError::Invalid)?;
    common::require_bounded_text(&self.status, 256).map_err(|_| ProtocolError::Invalid)?;
    common::require_network_hosts(&self.network_hosts).map_err(|_| ProtocolError::Invalid)
  }
}
impl Request {
  /// Rejects wrong role/version or invalid authority before provider execution.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if !matches!(self.protocol_version, PROTOCOL_VERSION | ORDERED_PROTOCOL_VERSION) {
      return Err(ProtocolError::IncompatibleVersion);
    }
    field(&self.request_id)?;
    match &self.command {
      Command::Check(intent) => intent.validate(CredentialClass::WorkReporterWrite),
      Command::Observe(intent) | Command::Project(intent) if self.protocol_version == PROTOCOL_VERSION => {
        intent.validate()
      }
      Command::Inspect(intent) if self.protocol_version == ORDERED_PROTOCOL_VERSION => intent.validate(),
      Command::Apply(request) if self.protocol_version == ORDERED_PROTOCOL_VERSION => {
        request.intent.validate()?;
        request.step.validate()
      }
      Command::Cancel { target_projection_id } => digest(target_projection_id),
      _ => Err(ProtocolError::Invalid),
    }
  }
}
/// Correlated reporter facts, never accepted Factory evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
  /// Exact negotiated Reporter version.
  pub protocol_version: u16,
  /// Original process correlation.
  pub request_id: String,
  /// Bounded projection facts.
  pub outcome: Outcome,
}
/// Closed projection outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// Safe read-only configuration/authentication/resource/capability observations.
  Checked(check::CheckReport),
  /// Read-only native effects and conditional steps; this grants no write authority.
  Planned(ProjectionPlan),
  /// One step completed; the host must recheck order and inspect before another write.
  Progressed(ProjectionPlan),
  /// Safe classified failure without raw provider diagnostics.
  Failure(Failure),
  /// Independently observed current effect state.
  Observed(ProjectionReceipt),
  /// Exact desired configured effects were observed applied.
  Applied(ProjectionReceipt),
  /// Original operation stopped without creating new effects.
  Acknowledged {
    /// Original trusted projection identity.
    projection_id: String,
  },
}
/// Safe projection checkpoint with no raw labels, URLs, token or provider diagnostic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionReceipt {
  /// Original logical operation identity.
  pub projection_id: String,
  /// Original native ticket container.
  pub resource_id: String,
  /// Original stable native item.
  pub item_id: String,
  /// Original installed profile.
  pub configuration_digest: String,
  /// Original exact desired-effects mapping.
  pub mapping: ConfigurationReference,
  /// Original configured value.
  pub status: String,
  /// Whether actual observed state satisfies the pinned effects.
  pub matches: bool,
}
impl ProjectionReceipt {
  /// Rejects any substitution of item, mapping or original intent.
  pub fn validate_for(&self, intent: &ProjectionIntent) -> Result<(), ProtocolError> {
    intent.validate()?;
    if self.projection_id != intent.projection_id
      || self.resource_id != intent.resource.resource_id
      || self.item_id != intent.item_id
      || self.configuration_digest != intent.configuration_digest
      || self.mapping != intent.mapping
      || self.status != intent.status
    {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}
impl Response {
  /// Checks exact role correlation; partial/unknown effects cannot claim successful projection.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    common::require_correlation(
      self.protocol_version,
      &self.request_id,
      request.protocol_version,
      &request.request_id,
    )
    .map_err(|_| ProtocolError::Invalid)?;
    match (&request.command, &self.outcome) {
      (Command::Check(intent), Outcome::Checked(report)) => report.validate_for(intent),
      (_, Outcome::Failure(failure)) => failure.validate(),
      (Command::Inspect(intent), Outcome::Planned(plan)) => plan.validate_for(intent),
      (Command::Apply(step), Outcome::Progressed(plan)) => plan.validate_for(&step.intent),
      (Command::Observe(intent), Outcome::Observed(receipt)) => receipt.validate_for(intent),
      (Command::Project(intent), Outcome::Applied(receipt)) if receipt.matches => receipt.validate_for(intent),
      (Command::Cancel { target_projection_id }, Outcome::Acknowledged { projection_id })
        if projection_id == target_projection_id =>
      {
        Ok(())
      }
      _ => Err(ProtocolError::Invalid),
    }
  }
}
