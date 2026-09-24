use std::collections::BTreeMap;

use octacity_protocol::{AGENT_PROTOCOL_VERSION, ExecutionSpec, JobBinding, JobSpecV1, SourceSpec};
use octacity_server_domain::{AttemptNumber, JobId, PipelineNodeId, Timestamp};
use serde_json::Value;

use super::{
  DerivedJobSpec, JobExecutionTemplate, JobSpecBuildSnapshot, JobSpecDerivationError, JobSpecPolicySnapshot,
  JobSpecSigner, JobSpecTemplate,
};

/// Derives stable server-owned intent from immutable Build, Pipeline, and policy snapshots.
pub fn derive_job_spec_template(
  build: &JobSpecBuildSnapshot,
  pipeline_node_id: PipelineNodeId,
  template: Value,
  policy: &JobSpecPolicySnapshot,
) -> Result<JobSpecTemplate, JobSpecDerivationError> {
  let execution: JobExecutionTemplate =
    serde_json::from_value(template).map_err(|_| JobSpecDerivationError::InvalidTemplate)?;
  JobSpecTemplate::new(build, pipeline_node_id, execution, policy)
}

/// Signs one persisted template for a concrete transition to `Ready`.
pub fn sign_ready_job_spec(
  template: &JobSpecTemplate,
  attempt_number: AttemptNumber,
  job_id: JobId,
  issued_at: Timestamp,
  signer: &JobSpecSigner,
) -> Result<DerivedJobSpec, JobSpecDerivationError> {
  template.validate()?;
  let attempt = u32::try_from(attempt_number.get()).map_err(|_| JobSpecDerivationError::AttemptOutOfRange)?;
  let issued_at =
    u64::try_from(issued_at.unix_millis().div_euclid(1_000)).map_err(|_| JobSpecDerivationError::InvalidIssueTime)?;
  let spec = wire_job_spec(template, attempt, job_id.to_string(), issued_at)?;
  let envelope = signer.sign(&spec).map_err(JobSpecDerivationError::Signing)?;
  Ok(DerivedJobSpec { envelope })
}

pub(super) fn validate_protocol_template(template: &JobSpecTemplate) -> Result<(), JobSpecDerivationError> {
  let job_id = "00000000-0000-0000-0000-000000000001".to_owned();
  let spec = wire_job_spec(template, 1, job_id.clone(), 0)?;
  spec
    .validate(&JobBinding {
      job_id: &job_id,
      attempt: 1,
      now: 0,
    })
    .map_err(|_| JobSpecDerivationError::InvalidPolicy)
}

fn wire_job_spec(
  template: &JobSpecTemplate,
  attempt: u32,
  job_id: String,
  issued_at: u64,
) -> Result<JobSpecV1, JobSpecDerivationError> {
  let expires_at = issued_at
    .checked_add(template.policy().validity.get())
    .ok_or(JobSpecDerivationError::ValidityOverflow)?;
  let mut source_parameters = BTreeMap::new();
  source_parameters.insert(
    template.policy().source.repository_parameter.clone(),
    Value::String(template.repository_locator().to_owned()),
  );
  let execution = template.execution();
  Ok(JobSpecV1 {
    protocol_version: AGENT_PROTOCOL_VERSION,
    job_id,
    attempt,
    issued_at,
    expires_at,
    source: SourceSpec {
      provider: template.policy().source.provider.clone(),
      plugin_version: template.policy().source.plugin_version.clone(),
      plugin_sha256: template.policy().source.plugin_sha256.clone(),
      revision: template.immutable_revision().as_str().to_owned(),
      reference: template
        .source_reference()
        .map(|reference| reference.as_str().to_owned()),
      parameters: source_parameters,
    },
    octa: template.policy().octa.clone(),
    execution: ExecutionSpec {
      octafile: execution.octafile.clone(),
      commands: execution.commands.clone(),
      variables: execution_variables(template.parameters())?,
      arguments: execution.arguments.clone(),
      concurrency: execution.concurrency,
      parallel: execution.parallel,
      failfast: execution.failfast,
      secrets_profile: template.policy().secrets_profile.as_ref().map(ToString::to_string),
    },
    runtime: template.policy().runtime.clone(),
    cache: template.policy().cache.clone(),
    outputs: template.policy().outputs.clone(),
  })
}

pub(super) fn validate_execution_template(template: &JobExecutionTemplate) -> Result<(), JobSpecDerivationError> {
  if template.commands.is_empty()
    || template
      .commands
      .iter()
      .any(|command| command.is_empty() || command.trim() != command || command.chars().any(char::is_control))
  {
    return Err(JobSpecDerivationError::InvalidTemplate);
  }
  Ok(())
}

pub(super) fn execution_variables(
  parameters: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, String>, JobSpecDerivationError> {
  parameters
    .iter()
    .map(|(name, value)| {
      let value = match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => {
          return Err(JobSpecDerivationError::InvalidParameter(name.clone()));
        }
      };
      Ok((name.clone(), value))
    })
    .collect()
}
