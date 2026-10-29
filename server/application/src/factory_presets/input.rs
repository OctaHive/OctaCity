//! Prospective inputs reuse installed metadata and pure ordinary input validation.
use super::*;

impl<S> FactoryPresetHandlers<S> {
  pub(super) async fn resolve_input(
    &self,
    context: &ManagementRequestContext,
    project: octacity_server_domain::ProjectId,
    selection: FactoryPresetInputSelection,
    target: &FactoryConfiguration,
    repository: &WorkSourceRepositoryRef,
  ) -> Result<WorkSourceInputIntent, ApplicationError> {
    let fail = ApplicationError::capability_unavailable;
    let choices = self
      .sources
      .as_ref()
      .ok_or_else(fail)?
      .handle_authorized_query(
        context,
        ListWorkSourceInputChoicesQuery {
          project_id: project,
          connection_id: selection.connection.id,
          connection_version: selection.connection.version,
        },
      )
      .await
      .map_err(|_| fail())?;
    let source = choices
      .for_binding(&selection.connection, &selection.source, IntegrationPurpose::TaskRead)
      .ok_or_else(fail)?;
    if let Some(predicate) = &selection.predicate {
      crate::factory_source_configuration::validate_source_predicate(
        predicate,
        source.predicate_states.as_deref(),
        source
          .predicate_label_presence
          .as_ref()
          .is_some_and(|values| values.contains(&WorkSourceLabelPresence::None)),
      )?;
    }
    let reporter = match &selection.reporter {
      Some(binding) => Some(
        choices
          .for_binding(&selection.connection, binding, IntegrationPurpose::TaskUpdate)
          .ok_or_else(fail)?,
      ),
      None => None,
    };
    if reporter.map(|r| &r.connection_configuration) != selection.connection.reporter_configuration.as_ref() {
      return Err(fail());
    }
    if let Some(mapping) = &selection.reporting {
      crate::factory_source_configuration::validate_source_reporting(
        mapping,
        reporter
          .and_then(|r| r.reporting.as_ref())
          .and_then(|r| r.editable.as_ref()),
      )?;
    }
    let capability = |choice: &WorkSourceInputChoice, role| WorkSourceCapability {
      project_id: project,
      role,
      binding: choice.binding.clone(),
    };
    let source = capability(source, WorkSourceCapabilityRole::Source);
    let reporter = reporter.map(|choice| capability(choice, WorkSourceCapabilityRole::Reporter));
    let definition = WorkSourceInputDraft {
      target: target.reference().clone(),
      repository: repository.clone(),
      source: selection.source,
      reporter: selection.reporter,
      connection: Some(selection.connection),
      reporting: selection.reporting,
      predicate: selection.predicate,
      enabled: false,
    };
    WorkSourceInput::publish(
      selection.id,
      project,
      definition.clone(),
      WorkSourceInputContext {
        target,
        repository,
        source: Some(&source),
        reporter: reporter.as_ref(),
      },
    )
    .map_err(|_| fail())?;
    Ok(WorkSourceInputIntent {
      id: selection.id,
      expected_version: None,
      definition,
    })
  }
}
