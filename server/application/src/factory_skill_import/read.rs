//! Explicit private-content reads over the original Project skill import and object reader.
use super::*;

/// Maximum UTF-8 file body expanded by the Console preview.
pub const MAX_PROJECT_SKILL_PREVIEW_BYTES: u64 = 64 * 1024;
/// Reads one retained skill version and optionally one exact file, without executing its content.
#[derive(Clone, Debug)]
pub struct GetProjectSkillQuery {
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Exact imported identity, version and digest.
  pub reference: octacity_server_factory::ImmutableReference,
  /// Explicit relative file selection; omission reads only the inventory.
  pub path: Option<FactoryRepositoryPath>,
}
/// Authorized exact inventory; content is expanded only for the selected file.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ProjectSkillDetails {
  /// Original retained file tree.
  pub bundle: FactoryRoleSkillBundle,
  /// Optional bounded text preview, without executing Markdown, HTML or scripts.
  pub file: Option<ProjectSkillFilePreview>,
}
/// Selected file metadata and a bounded UTF-8 body; binary/large files retain metadata only.
#[derive(Clone, serde::Serialize)]
pub struct ProjectSkillFilePreview {
  /// Exact selected path in the bundle.
  pub path: FactoryRepositoryPath,
  /// Original retained size.
  pub size_bytes: u64,
  /// Escaped by the Console; absent for binary or oversized files.
  pub text: Option<String>,
}
impl std::fmt::Debug for ProjectSkillFilePreview {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ProjectSkillFilePreview")
      .field("path", &self.path)
      .finish_non_exhaustive()
  }
}
impl Query for GetProjectSkillQuery {
  type Outcome = ProjectSkillDetails;
}
impl ManagementAuthorizationTarget for GetProjectSkillQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
#[async_trait::async_trait]
impl<S: ProjectSkillImportStore, B: ArtifactByteReader> ManagementQueryUseCase<GetProjectSkillQuery>
  for ProjectSkillImport<S, B>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: GetProjectSkillQuery,
  ) -> Result<ProjectSkillDetails, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    let record = self
      .store
      .project_skill_import(query.project_id, &query.reference)
      .await?
      .ok_or_else(ApplicationError::unavailable)?;
    let file = if let Some(path) = query.path {
      let reference = record
        .intent
        .bundle
        .files()
        .get(&path)
        .ok_or_else(ApplicationError::unavailable)?;
      let input = record
        .inputs()?
        .into_iter()
        .find(|input| input.artifact_id == reference.artifact_id())
        .ok_or_else(ApplicationError::unavailable)?;
      let authorize = || {
        crate::factory_configuration_choices::require_authorized_reference(
          self.policy.as_ref(),
          context,
          ManagementAction::Download,
          ManagementResourceKind::Artifact,
          input.artifact_id,
        )
      };
      authorize().await?;
      let text = if input.size_bytes <= MAX_PROJECT_SKILL_PREVIEW_BYTES {
        String::from_utf8(read_verified(self.bytes.as_ref(), &input).await?).ok()
      } else {
        None
      };
      authorize().await?;
      Some(ProjectSkillFilePreview {
        path,
        size_bytes: input.size_bytes,
        text,
      })
    } else {
      None
    };
    Ok(ProjectSkillDetails {
      bundle: record.intent.bundle,
      file,
    })
  }
}
