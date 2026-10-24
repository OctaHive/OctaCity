//! Initial role packages over immutable catalog versions and retained input readers.
use crate::{
  ApplicationError, FactoryCodexBuildProfile, FactoryNodeContextSource, FactoryRoleInvocation,
  FactoryRoleInvocationSource,
};
use octacity_artifact_store::ArtifactByteReader;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryRoleProfileStore, IntegrationAdministrationStore, IntegrationConnectionStore, IntegrationProjectAccessStore,
  ProjectSkillImportStore,
};
use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

/// Resolves the first package; replay remains owned by the existing publication journal.
/// No current defaults, provider requests or ambient skill lookup participate.
pub struct FactoryStoredCodexRoleInvocationSource<S, B> {
  store: Arc<S>,
  bytes: Arc<B>,
  context: Arc<dyn FactoryNodeContextSource>,
  profiles: Vec<FactoryCodexBuildProfile>,
}
impl<S, B> FactoryStoredCodexRoleInvocationSource<S, B> {
  /// Shares the installed compiler policies, exact catalog and existing retained readers.
  pub fn new(
    store: Arc<S>,
    bytes: Arc<B>,
    context: Arc<dyn FactoryNodeContextSource>,
    profiles: Vec<FactoryCodexBuildProfile>,
  ) -> Self {
    Self {
      store,
      bytes,
      context,
      profiles,
    }
  }
}
#[async_trait::async_trait]
impl<
  S: FactoryRoleProfileStore
    + ProjectSkillImportStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore,
  B: ArtifactByteReader,
> FactoryRoleInvocationSource for FactoryStoredCodexRoleInvocationSource<S, B>
{
  async fn authorize_dependencies(&self, invocation: &FactoryRoleInvocation) -> Result<(), ApplicationError> {
    self.authorize_profile(&invocation.package().inputs().profile).await
  }
  async fn invocation(
    &self,
    intent: &FlowBuildIntent,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<Option<FactoryRoleInvocation>, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let Some(selected) = intent.selected_profile() else {
      return Ok(None);
    };
    if required_until <= observed_at || required_until > intent.node().deadline() {
      return Err(invalid());
    }
    let profile = self
      .store
      .factory_role_profile(selected.project_id, selected.id, Some(selected.version))
      .await?;
    if profile.reference() != selected {
      return Err(invalid());
    }
    self.authorize_profile(&profile).await?;
    let role = self
      .store
      .factory_role(selected.project_id, profile.role().id, Some(profile.role().version))
      .await?;
    if role.reference() != profile.role() {
      return Err(invalid());
    }
    let installed = self
      .profiles
      .iter()
      .find(|installed| installed.matches_role_intent(intent, Some(&profile)))
      .ok_or_else(invalid)?;
    if !matches!(
      installed.consumer,
      FactoryCredentialConsumer::ReadOnlyReview | FactoryCredentialConsumer::ToolReasoning
    ) {
      return Err(invalid());
    }
    let context = intent.input().context().ok_or_else(invalid)?;
    let bytes = self
      .context
      .read_context(intent.input(), observed_at, required_until)
      .await?;
    let rendered = crate::factory_codex::render_context(intent, &bytes).map_err(|_| invalid())?;
    let mut rules = Vec::new();
    let mut tasks = Vec::new();
    let mut specifications = Vec::new();
    let mut prior_findings = BTreeSet::new();
    for entry in context.entries() {
      match entry.source_kind() {
        ContextSourceKind::Policy => {
          let FactoryContextReference::Artifact(artifact) = entry.source() else {
            return Err(invalid());
          };
          rules.push(FactoryRoleProjectRule {
            source: entry.logical_identity().clone(),
            instructions: String::from_utf8(bytes.get(&artifact.artifact_id()).ok_or_else(invalid)?.clone())
              .map_err(|_| invalid())?,
          });
        }
        ContextSourceKind::Task | ContextSourceKind::Specification => {
          let FactoryContextReference::Artifact(artifact) = entry.source() else {
            return Err(invalid());
          };
          if entry.source_kind() == ContextSourceKind::Task {
            tasks.push(artifact.clone());
          } else {
            specifications.push(artifact.clone());
          }
        }
        ContextSourceKind::TypedResult => {
          prior_findings.insert(entry.content_digest());
        }
        _ => {}
      }
    }
    let task = tasks.first().cloned().ok_or_else(invalid)?;
    specifications.extend(tasks.into_iter().skip(1));
    let prompt = crate::factory_codex::render_role_prompt(
      intent,
      &installed.instructions,
      Some(&rendered),
      Some(crate::factory_codex::RolePrompt {
        rules: &rules,
        role: &role,
        profile: &profile,
      }),
    )
    .map_err(|_| invalid())?;
    let operation = intent.operation_id().map_err(|_| invalid())?;
    let envelope = FactoryTaskEnvelope::new(
      FactoryTaskDeclaration {
        id: TaskEnvelopeId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &operation.as_bytes()))
          .map_err(|_| invalid())?,
        node_attempt_id: intent.node().id(),
        subject: context.subject().clone(),
        mode: FactoryTaskMode::Observe,
      },
      FactoryTaskDefinition {
        task,
        specifications,
        prior_findings: prior_findings.into_iter().collect(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        result_schema: FactoryTaskResultSchema::FlowV1,
        deliverables: vec![],
      },
      context,
      profile.definition().toolchain.clone(),
      FactoryTaskControlDigests {
        policy: intent.profile().task_digest,
        prompt: crate::codex_prompt_digest(&prompt),
        prompt_provenance: crate::codex_prompt_provenance_digest(&prompt),
        // The exact installed/selected task policy also controls provenance validation.
        provenance: intent.profile().task_digest,
      },
    )
    .map_err(|_| invalid())?;
    let mut skills = BTreeMap::new();
    for (alias, reference) in role.effective_skills(&profile).map_err(|_| invalid())? {
      let imported = self
        .store
        .project_skill_import(selected.project_id, &reference)
        .await?
        .ok_or_else(invalid)?;
      skills.insert(alias, imported.intent.bundle);
    }
    let package = FactoryRoleExecutionPackage::new(FactoryRoleExecutionPackageInput {
      project_rules: rules,
      role,
      profile,
      skills,
      task: envelope,
      context: context.clone(),
    })
    .map_err(|_| invalid())?;
    let invocation = crate::factory_role_invocation::prepare_factory_role_invocation(
      self.store.as_ref(),
      self.bytes.as_ref(),
      package,
      BTreeMap::new(),
    )
    .await?;
    invocation.verify_intent(intent)?;
    Ok(Some(invocation))
  }
}

impl<S: IntegrationProjectAccessStore + IntegrationConnectionStore + IntegrationAdministrationStore, B>
  FactoryStoredCodexRoleInvocationSource<S, B>
{
  async fn authorize_profile(&self, profile: &FactoryRoleProfile) -> Result<(), ApplicationError> {
    for binding in profile.definition().execution.integrations.values() {
      if !crate::factory_role_profile_catalog::factory_role_integration_available(self.store.as_ref(), binding).await? {
        return Err(ApplicationError::capability_unavailable());
      }
    }
    Ok(())
  }
}
