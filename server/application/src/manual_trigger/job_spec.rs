use octacity_server_domain::{BuildId, ImmutableRevision};
use octacity_server_job::{JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecTemplate, derive_job_spec_template};
use octacity_server_pipeline::PipelineNode;

use super::{
  model::{ManualSourceSelection, ManualTriggerContext, ManualTriggerError},
  preparation::PreparedManualTrigger,
};

pub(super) struct JobSpecTemplateDeriver {
  build: JobSpecBuildSnapshot,
  policy: JobSpecPolicySnapshot,
}

impl JobSpecTemplateDeriver {
  pub(super) fn new(
    context: &ManualTriggerContext,
    build_id: BuildId,
    prepared: &PreparedManualTrigger,
    immutable_revision: &ImmutableRevision,
  ) -> Result<Self, ManualTriggerError> {
    let build = JobSpecBuildSnapshot::new(
      build_id,
      immutable_revision.clone(),
      source_reference(&prepared.source),
      context.repository.definition.repository_locator.clone(),
      prepared.parameters.clone(),
    )
    .map_err(ManualTriggerError::JobSpec)?;
    Ok(Self {
      build,
      policy: crate::build_configuration_job_policy::configuration_job_policy(
        context,
        context.configuration.definition.secrets_profile.clone(),
      )
      .map_err(ManualTriggerError::JobSpec)?,
    })
  }

  pub(super) fn derive(&self, node: &PipelineNode) -> Result<JobSpecTemplate, ManualTriggerError> {
    derive_job_spec_template(&self.build, node.id().clone(), node.template().clone(), &self.policy)
      .map_err(ManualTriggerError::JobSpec)
  }
}

fn source_reference(source: &ManualSourceSelection) -> Option<octacity_server_domain::SourceReference> {
  match source {
    ManualSourceSelection::Reference(reference) => Some(reference.clone()),
    ManualSourceSelection::DefaultReference | ManualSourceSelection::ExactRevision(_) => None,
  }
}
