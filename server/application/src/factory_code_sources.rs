//! Code availability is an authorized projection of existing Repository definitions.
use crate::{ApplicationError, FactoryValidationDiagnostics, ManagementAuthorizationPolicy, ManagementRequestContext};
use octacity_server_domain::ProjectId;
use octacity_server_factory::{FactoryDigest, FactoryFlowConfiguration, FactoryRepositoryRef};
use octacity_server_store::{ConfigurationStore, PublishedRepository};

/// Shared exact reference over the Repository owner's immutable definition.
pub(super) fn repository_reference(repository: &PublishedRepository) -> Result<FactoryRepositoryRef, ApplicationError> {
  Ok(FactoryRepositoryRef {
    project_id: repository.project_id,
    id: repository.id,
    version: repository.version,
    definition_digest: FactoryDigest::content_sha256(
      &serde_json::to_vec(&repository.definition).map_err(|_| ApplicationError::invalid())?,
    ),
  })
}

/// Metadata validation performs no VCS resolution, credential read or permission mutation.
pub(super) async fn validate_sources(
  closure: &octacity_server_factory::PinnedFlowDefinitionClosure,
  project: ProjectId,
  repositories: Option<&dyn ConfigurationStore>,
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
) -> Result<(), ApplicationError> {
  for definition in closure.definitions() {
    for (alias, source) in definition.code_sources().into_iter().flatten() {
      let field = format!("/code_sources/{alias}/repository");
      let reference = &source.repository;
      if reference.project_id != project {
        return Err(FactoryValidationDiagnostics::invalid(&field));
      }
      let repositories = repositories.ok_or_else(|| FactoryValidationDiagnostics::unavailable(&field))?;
      crate::factory_configuration_choices::require_visible_reference(
        policy,
        context,
        crate::ManagementResourceKind::Repository,
        reference.id,
      )
      .await
      .map_err(|_| FactoryValidationDiagnostics::unavailable(&field))?;
      let repository = repositories
        .repository_version(reference.id, reference.version)
        .await
        .map_err(|_| FactoryValidationDiagnostics::unavailable(&field))?;
      if repository_reference(&repository)? != *reference {
        return Err(FactoryValidationDiagnostics::invalid(&field));
      }
      crate::manual_trigger::select_source(&source.selection, &repository)
        .map_err(|_| FactoryValidationDiagnostics::invalid(format!("/code_sources/{alias}/selection")))?;
      for node in definition.nodes() {
        if let Some(octacity_server_factory::FlowCodeBinding::Source {
          source: selected,
          revision: octacity_server_factory::FlowCodeRevision::ExactRevision { revision },
          ..
        }) = node.code()
          && selected == alias
        {
          crate::manual_trigger::select_source(
            &octacity_server_domain::RepositorySourceSelection::ExactRevision(revision.clone()),
            &repository,
          )
          .map_err(|_| invalid_binding(node.key(), "/code/revision"))?;
        }
      }
    }
  }
  Ok(())
}

/// Maps domain locations to the ordinary validation report without deriving authority in the UI.
pub(super) fn validation_error(error: &octacity_server_factory::FactoryError) -> Option<ApplicationError> {
  let octacity_server_factory::FactoryError::InvalidCodeBinding { context } = error else {
    return None;
  };
  Some(invalid_binding(&context.node, context.field))
}
fn invalid_binding(node: &octacity_server_factory::FactoryKey, field: &str) -> ApplicationError {
  let ApplicationError::FactoryValidation(mut report) = FactoryValidationDiagnostics::invalid(field) else {
    unreachable!();
  };
  report.errors[0].node = Some(node.clone());
  ApplicationError::FactoryValidation(report)
}

/// Resolves only used declarations after the entire catalogue passed authorization.
/// All pins exist before admission publication or any protected execution I/O.
pub(super) async fn resolve_used_sources(
  flow: &FactoryFlowConfiguration,
  repositories: Option<&dyn ConfigurationStore>,
  revisions: &dyn crate::RevisionResolver,
) -> Result<Vec<octacity_server_factory::FlowCodePin>, ApplicationError> {
  use octacity_server_factory::{ExactSubject, FlowCodeBinding, FlowCodePin};
  let mut pins = vec![];
  for definition in flow.closure.definitions() {
    let used = definition
      .nodes()
      .iter()
      .filter_map(|node| match node.code() {
        Some(FlowCodeBinding::Source {
          source,
          revision: octacity_server_factory::FlowCodeRevision::AdmittedBase,
          ..
        }) => Some(source.clone()),
        _ => None,
      })
      .collect::<std::collections::BTreeSet<_>>();
    for alias in used {
      let source = definition
        .code_sources()
        .and_then(|sources| sources.get(&alias))
        .ok_or_else(ApplicationError::invalid)?;
      let repository = repositories
        .ok_or_else(ApplicationError::invalid)?
        .repository_version(source.repository.id, source.repository.version)
        .await?;
      let selection = crate::manual_trigger::select_source(&source.selection, &repository)
        .map_err(|_| ApplicationError::invalid())?;
      let revision = revisions
        .resolve(crate::RevisionResolutionRequest { repository, selection })
        .await
        .map_err(crate::factory_admission::map_revision_error)?;
      pins.push(FlowCodePin {
        definition: definition.reference(),
        source: alias,
        subject: ExactSubject::new(source.repository.project_id, source.repository.id, revision),
      });
    }
  }
  Ok(pins)
}
