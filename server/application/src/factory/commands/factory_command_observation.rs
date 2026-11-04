//! Symptom-independent qualification of installed ordinary command observations.
//! The ordinary output source owns Job, Attempt, compiled program and publication
//! verification; this contract binds its structured report to the frozen input.
//! Environment/criterion identifiers establish consistency. Actual readiness is
//! observed by the qualified installed program/runtime, never inferred from an identifier.
use crate::OrdinaryBuildApplicationError;
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Operator-selected observation vocabulary and criterion, frozen with the command installation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCommandObservationContract {
  /// Expected environment contract; the qualified command preflight observes readiness.
  pub environment: ImmutableReference,
  /// Exact reviewed criterion, independently of any model-authored claim.
  pub criterion: ImmutableReference,
  /// Closed bounded observation data, without a self-reported verdict.
  pub observations: FlowDataSchema,
  /// Required coverage/preconditions before absence can mean a negative finding.
  pub completion: FlowPredicate,
  /// Configured criterion evaluated only over structured observations.
  pub symptom: FlowPredicate,
}

impl FactoryCommandObservationContract {
  /// Canonical closed report envelope around the installation's bounded observations.
  /// Preset resolution and the installed output binding reuse this exact schema.
  pub fn report_schema(&self) -> Result<FlowDataSchema, OrdinaryBuildApplicationError> {
    let invalid = || OrdinaryBuildApplicationError::Invalid;
    let text = |min_bytes, max_bytes| FlowValueSchema::String { min_bytes, max_bytes };
    let pin = FlowValueSchema::Object {
      fields: required_fields([
        ("identity", text(1, MAX_FACTORY_KEY_BYTES)),
        ("version", text(1, MAX_FACTORY_KEY_BYTES)),
        ("digest", text(64, 64)),
      ])?,
    };
    let subject = FlowValueSchema::Object {
      fields: required_fields([
        ("project_id", text(36, 36)),
        ("repository_id", text(36, 36)),
        (
          "base_revision",
          text(1, octacity_server_domain::MAX_IMMUTABLE_REVISION_BYTES),
        ),
      ])?,
    };
    let mut fields = required_fields([
      ("input_digest", text(64, 64)),
      ("subject", subject),
      ("command", pin.clone()),
      ("environment", pin.clone()),
      ("criterion", pin),
      (
        "status",
        FlowValueSchema::Enum {
          values: vec![
            serde_json::json!(CommandObservationStatus::Completed),
            serde_json::json!(CommandObservationStatus::EnvironmentUnavailable),
            serde_json::json!(CommandObservationStatus::NeedsInput),
          ],
        },
      ),
      (
        "finding",
        FlowValueSchema::Enum {
          values: vec![
            serde_json::json!(FactoryCommandFinding::Matched),
            serde_json::json!(FactoryCommandFinding::NotMatched),
            serde_json::json!(FactoryCommandFinding::Indeterminate),
          ],
        },
      ),
    ])?;
    fields.insert(
      FactoryKey::new("observations").map_err(|_| invalid())?,
      FlowFieldSchema::optional(self.observations.shape().clone()),
    );
    FlowDataSchema::new(
      FactoryKey::new("factory.command-observation").map_err(|_| invalid())?,
      FactoryKey::new("v1").map_err(|_| invalid())?,
      FlowValueSchema::Object { fields },
    )
    .map_err(|_| invalid())
  }

  /// Rejects unbounded or malformed criteria before any command can be dispatched.
  pub fn validate(&self) -> Result<(), OrdinaryBuildApplicationError> {
    self.program().map(|_| ())
  }

  fn program(&self) -> Result<FlowGateProgram, OrdinaryBuildApplicationError> {
    let invalid = || OrdinaryBuildApplicationError::Invalid;
    self.report_schema()?;
    let matched = FactoryKey::new("matched").map_err(|_| invalid())?;
    let not_matched = FactoryKey::new("not_matched").map_err(|_| invalid())?;
    let indeterminate = FactoryKey::new("indeterminate").map_err(|_| invalid())?;
    FlowGateProgram::new(
      vec![
        FlowGateRule {
          outcome: matched,
          when: self.symptom.clone(),
        },
        FlowGateRule {
          outcome: not_matched,
          when: FlowPredicate::Not {
            predicate: Box::new(self.symptom.clone()),
          },
        },
      ],
      indeterminate,
    )
    .and_then(|program| program.with_precondition(self.completion.clone()))
    .map_err(|_| invalid())
  }
}

fn required_fields(
  fields: impl IntoIterator<Item = (&'static str, FlowValueSchema)>,
) -> Result<BTreeMap<FactoryKey, FlowFieldSchema>, OrdinaryBuildApplicationError> {
  fields
    .into_iter()
    .map(|(name, shape)| {
      Ok((
        FactoryKey::new(name).map_err(|_| OrdinaryBuildApplicationError::Invalid)?,
        FlowFieldSchema::required(shape),
      ))
    })
    .collect()
}

/// A completed observation can match, not match or remain indeterminate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryCommandFinding {
  /// The completed criterion was observed.
  Matched,
  /// Complete applicable observations independently rule out that criterion.
  NotMatched,
  /// Missing coverage or execution conditions establish neither finding.
  Indeterminate,
}

/// Safe immutable primary-report choice, without program, parameters or credentials.
/// This describes the declared contract; Agent/runtime qualification stays separate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCommandObservationBinding {
  /// Exact installed command configuration.
  pub command: ImmutableReference,
  /// Executed ordinary command tool.
  pub tool: ImmutableReference,
  /// Qualified execution plugin.
  pub plugin: ImmutableReference,
  /// Frozen program, criteria, schemas and execution ceilings.
  pub task_digest: FactoryDigest,
  /// Project-owned ordinary Build configuration.
  pub build_configuration: BuildConfigurationRef,
  /// Explicit primary output selected by a consuming node.
  pub result_output: FactoryKey,
  /// Canonical report schema including selected observations.
  pub schema: ImmutableReference,
  /// Reviewed expected environment.
  pub environment: ImmutableReference,
  /// Reviewed expected criterion.
  pub criterion: ImmutableReference,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum CommandObservationStatus {
  Completed,
  EnvironmentUnavailable,
  NeedsInput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandObservationReport {
  input_digest: FactoryDigest,
  subject: ExactSubject,
  command: ImmutableReference,
  environment: ImmutableReference,
  criterion: ImmutableReference,
  status: CommandObservationStatus,
  observations: Option<serde_json::Value>,
  finding: FactoryCommandFinding,
}

/// Checks a qualified command's bounded report against its original immutable execution.
/// Success alone, an exit code or a model response never supplies the criterion verdict.
pub fn verify_factory_command_observation(
  contract: &FactoryCommandObservationContract,
  intent: &FlowBuildIntent,
  value: &serde_json::Value,
) -> Result<FactoryCommandFinding, OrdinaryBuildApplicationError> {
  let invalid = || OrdinaryBuildApplicationError::Invalid;
  let program = contract.program()?;
  if intent.node().node_kind() != FlowNodeKind::BuildCommand
    || !matches!(intent.input().context().map(|context| context.subject()),
      Some(FactoryTaskSubject::Exact(subject)) if subject == intent.input().subject())
    || serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > MAX_FLOW_DATA_BYTES)
  {
    return Err(invalid());
  }
  let report: CommandObservationReport = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
  if report.input_digest != intent.input().digest().map_err(|_| invalid())?
    || &report.subject != intent.input().subject()
    || report.command != intent.profile().model_or_tool
    || report.environment != contract.environment
    || report.criterion != contract.criterion
  {
    return Err(invalid());
  }
  let observations = report
    .observations
    .map(|value| FlowPayload::new(&contract.observations, value))
    .transpose()
    .map_err(|_| invalid())?;
  if !matches!(report.status, CommandObservationStatus::Completed) {
    return (report.finding == FactoryCommandFinding::Indeterminate)
      .then_some(FactoryCommandFinding::Indeterminate)
      .ok_or_else(invalid);
  }
  let observations = observations.ok_or_else(invalid)?;
  let selected = program.select(&observations).map_err(|_| invalid())?;
  let finding = match selected.as_str() {
    "matched" => FactoryCommandFinding::Matched,
    "not_matched" => FactoryCommandFinding::NotMatched,
    _ => FactoryCommandFinding::Indeterminate,
  };
  if report.finding != finding {
    return Err(invalid());
  }
  Ok(finding)
}
