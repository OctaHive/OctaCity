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
    immutable_revision: Option<&ImmutableRevision>,
  ) -> Result<Self, ManualTriggerError> {
    let invalid = || ManualTriggerError::Invalid(super::model::ManualTriggerInputError::ContextMismatch);
    let build = match (context.repository.as_ref(), immutable_revision, prepared.source.mode()) {
      (Some(repository), Some(revision), None) => JobSpecBuildSnapshot::new(
        build_id,
        revision.clone(),
        source_reference(prepared.source.repository().ok_or_else(invalid)?),
        repository.definition.repository_locator.clone(),
        prepared.parameters.clone(),
      )
      .map_err(ManualTriggerError::JobSpec)?,
      (None, None, Some(_)) => JobSpecBuildSnapshot::source_free(build_id, prepared.parameters.clone()),
      _ => return Err(invalid()),
    };
    if prepared.source.mode().is_some() {
      // Ordinary managed Pipeline intent cannot create tool or secret authority beyond Configuration/Project policy.
      for node in context.pipeline.dag.nodes() {
        let program: octacity_server_job::ManagedJobSpecIntent = serde_json::from_value(node.template().clone())
          .map_err(|_| ManualTriggerError::JobSpec(octacity_server_job::JobSpecDerivationError::InvalidTemplate))?;
        if program.factory().is_some()
          || !program.permissions().tools.is_empty()
          || program.execution().tool_control.is_some()
          || program.permissions().secret_profiles.iter().any(|name| {
            context
              .configuration
              .definition
              .secrets_profile
              .as_ref()
              .is_none_or(|allowed| allowed.as_str() != name)
          })
        {
          return Err(invalid());
        }
      }
    }
    Ok(Self {
      build,
      policy: crate::build_configuration_job_policy::configuration_job_policy(
        context,
        if prepared.source.mode().is_some() {
          None
        } else {
          context.configuration.definition.secrets_profile.clone()
        },
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
