//! Current Source and independent Reporter grants remain with their existing owner.
use super::*;
pub(super) struct SourceReaders {
  records: Arc<dyn AuthorizedManagementQueryHandler<GetWorkSourceConfigurationQuery, Error = ApplicationError>>,
  choices: Arc<dyn AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>>,
}
impl SourceReaders {
  pub(super) fn new<Q>(queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<GetWorkSourceConfigurationQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      records: queries.clone(),
      choices: queries,
    }
  }
  pub(super) async fn observe(
    &self,
    context: &ManagementRequestContext,
    target: &FactoryConfiguration,
    repository: &WorkSourceRepositoryRef,
    selected: &WorkSourceMemberRef,
    now: Timestamp,
    end: Timestamp,
  ) -> Vec<FactoryReadinessCheck> {
    use FactoryReadinessState::*;
    let project = target.reference().project_id();
    let Ok(WorkSourceConfigurationRecord::Input(input)) = self
      .records
      .handle_authorized_query(
        context,
        GetWorkSourceConfigurationQuery {
          project_id: project,
          target: WorkSourceScanTarget::Input {
            id: selected.id,
            version: selected.version,
          },
        },
      )
      .await
    else {
      return vec![check("source", Blocked, "unavailable", true, now, end)];
    };
    let definition = input.definition();
    if input.id() != selected.id
      || input.version() != selected.version
      || input.project_id() != project
      || &definition.target != target.reference()
      || &definition.repository != repository
      || input.root_flow() != target.flow().closure.root()
      || !definition.enabled
    {
      return vec![check("source", Blocked, "input_incompatible", true, now, end)];
    }
    let Some(connection) = &definition.connection else {
      return vec![check("source", Unknown, "legacy_authority_unverified", true, now, end)];
    };
    let choices = self
      .choices
      .handle_authorized_query(
        context,
        ListWorkSourceInputChoicesQuery {
          project_id: project,
          connection_id: connection.id,
          connection_version: connection.version,
        },
      )
      .await
      .ok();
    let source = choices
      .as_ref()
      .and_then(|choices| choices.for_binding(connection, &definition.source, IntegrationPurpose::TaskRead));
    let predicate = definition.predicate.as_ref().is_none_or(|predicate| {
      source.is_some_and(|choice| {
        crate::factory_source_configuration::validate_source_predicate(
          predicate,
          choice.predicate_states.as_deref(),
          choice
            .predicate_label_presence
            .as_ref()
            .is_some_and(|values| values.contains(&WorkSourceLabelPresence::None)),
        )
        .is_ok()
      })
    });
    let available = source.is_some() && predicate;
    let mut checks = vec![check(
      "source",
      if available { Ready } else { Blocked },
      if available {
        "configured"
      } else {
        "authority_unavailable"
      },
      true,
      now,
      end,
    )];
    if let Some(reporter) = &definition.reporter {
      let writer = choices
        .as_ref()
        .and_then(|choices| choices.for_binding(connection, reporter, IntegrationPurpose::TaskUpdate));
      let mapping = definition.reporting.as_ref().is_none_or(|mapping| {
        crate::factory_source_configuration::validate_source_reporting(
          mapping,
          writer
            .and_then(|choice| choice.reporting.as_ref())
            .and_then(|reporting| reporting.editable.as_ref()),
        )
        .is_ok()
      });
      let compatible = writer.is_some()
        && mapping
        && crate::factory_stage_source_update::validate_flow_source_updates_for_input(definition, target.flow())
          .is_ok();
      checks.push(check(
        "reporter",
        if compatible { Ready } else { Blocked },
        if compatible {
          "configured"
        } else {
          "authority_unavailable"
        },
        true,
        now,
        end,
      ));
    }
    checks
  }
}
