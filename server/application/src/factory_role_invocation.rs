//! Provider-neutral retained package boundary shared by installed ordinary Build adapters.
use crate::ApplicationError;
use octacity_protocol::ProtectedInputManifestV3;
use octacity_server_factory::{FactoryDigest, FactoryRoleExecutionPackage, FactoryRoleProfile, FlowBuildIntent};
use octacity_server_secrets::LogicalSecretReference;
use std::collections::BTreeMap;

/// Canonical skill directory beneath the existing protected-input root.
pub(crate) const FACTORY_ROLE_SKILL_DIRECTORY: &str = "skills";

/// Binds complete role settings to an installed task without a per-Run digest cycle.
/// Exact stage input/rule/task bytes are separately bound by the retained package digest.
pub fn factory_role_task_digest(base: FactoryDigest, profile: &FactoryRoleProfile) -> FactoryDigest {
  let bytes =
    serde_json::to_vec(&(profile.role(), profile.reference())).expect("validated role/profile references serialize");
  FactoryDigest::sha256("octacity.factory.role-task.v1", &[&base.as_bytes(), &bytes])
}
/// Selected immutable content and qualified ordinary credential-variable bindings.
/// Private content intentionally has no Debug representation; no secret values are accepted.
#[derive(Clone)]
pub struct FactoryRoleInvocation(octacity_server_store::RetainedFactoryRoleInvocation);

impl FactoryRoleInvocation {
  /// Accepts exact retained skill transfer metadata and installed credential mappings.
  /// Byte ownership/retention is checked by the source through the existing retained reader.
  pub fn new(
    package: FactoryRoleExecutionPackage,
    skills: Option<ProtectedInputManifestV3>,
    secret_variables: BTreeMap<String, LogicalSecretReference>,
  ) -> Result<Self, ApplicationError> {
    if secret_variables.values().any(|reference| !package.inputs().profile.definition().execution.environment.values().any(|value| matches!(value, octacity_server_factory::FactoryRoleEnvironmentValue::Secret { reference: selected } if selected == reference))) { return Err(ApplicationError::invalid()); }
    let mut expected = Vec::new();
    for (alias, bundle) in &package.inputs().skills {
      for (path, file) in bundle.files() {
        let relative = octacity_server_factory::FactoryRepositoryPath::new(format!(
          "{FACTORY_ROLE_SKILL_DIRECTORY}/{}/{}",
          alias.as_str(),
          path.as_str()
        ))
        .map_err(|_| ApplicationError::invalid())?;
        expected.push(octacity_protocol::ProtectedInputV3 {
          artifact_id: file.artifact_id().to_string(),
          size_bytes: file.encoded_size(),
          sha256: file.content_digest().to_string(),
          media_type: "application/octet-stream".into(),
          destination: format!("{}/{}", octacity_protocol::PROTECTED_INPUT_ROOT, relative.as_str()),
        });
      }
    }
    expected.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    let total = expected
      .iter()
      .try_fold(0_u64, |sum, file| sum.checked_add(file.size_bytes))
      .ok_or_else(ApplicationError::invalid)?;
    if expected.len() > octacity_server_factory::MAX_CONTEXT_MANIFEST_ENTRIES
      || total > octacity_server_factory::MAX_CONTEXT_MANIFEST_BYTES
    {
      return Err(ApplicationError::invalid());
    }
    match (&skills, expected.is_empty()) {
      (None, true) => {}
      (Some(manifest), false) if manifest.inputs == expected => {
        manifest.validate().map_err(|_| ApplicationError::invalid())?;
      }
      _ => return Err(ApplicationError::invalid()),
    }
    Ok(Self(octacity_server_store::RetainedFactoryRoleInvocation {
      package,
      skills,
      secret_variables,
    }))
  }
  /// Private snapshot retained through the ordinary input publication owner.
  pub(crate) const fn retained(&self) -> &octacity_server_store::RetainedFactoryRoleInvocation {
    &self.0
  }
  pub(crate) fn restore(
    snapshot: octacity_server_store::RetainedFactoryRoleInvocation,
  ) -> Result<Self, ApplicationError> {
    Self::new(snapshot.package, snapshot.skills, snapshot.secret_variables)
  }
  /// Complete immutable execution content, independent of runtime/provider representation.
  pub const fn package(&self) -> &FactoryRoleExecutionPackage {
    &self.0.package
  }
  /// Complete selected skill transfer, already verified by its retained owner.
  pub const fn skills(&self) -> Option<&ProtectedInputManifestV3> {
    self.0.skills.as_ref()
  }
  /// Qualified Octa variable names to existing logical credential locators.
  pub const fn secret_variables(&self) -> &BTreeMap<String, LogicalSecretReference> {
    &self.0.secret_variables
  }
  /// Qualifies complete review authority through the existing purpose-bound credentials.
  /// A narrower task cannot disguise a writable or credential-incompatible profile.
  pub(crate) fn verify_consumer(
    &self,
    consumer: octacity_server_factory::FactoryCredentialConsumer,
    credentials: &octacity_server_factory::FactoryCredentialProfiles,
    mcp_credentials: &BTreeMap<String, String>,
  ) -> Result<(), ApplicationError> {
    if consumer != octacity_server_factory::FactoryCredentialConsumer::ReadOnlyReview {
      return Ok(());
    }
    let inputs = self.0.package.inputs();
    let permissions = &inputs.profile.definition().permissions;
    for profile in permissions.secret_profiles() {
      if mcp_credentials.values().any(|selected| selected == profile.as_str()) {
        continue;
      }
      credentials
        .authorize(inputs.task.node_attempt_id(), consumer, profile)
        .map_err(|_| ApplicationError::invalid())?;
    }
    if permissions.tools().next().is_some()
      || permissions.mounts().any(|mount| {
        mount.mode() == octacity_server_factory::MountMode::ReadWrite
          && !matches!(
            mount.root().as_str(),
            octacity_protocol::FACTORY_OUTPUT_ROOT | octacity_protocol::FACTORY_SCRATCH_ROOT
          )
      })
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }

  /// Rechecks immutable Node Attempt, Context, exact toolchain and ordinary authority.
  pub fn verify_intent(&self, intent: &FlowBuildIntent) -> Result<(), ApplicationError> {
    self
      .retained()
      .verify_intent(intent)
      .map_err(|_| ApplicationError::invalid())
  }
}

/// Exact retained selection supplied to installed compilers during input recovery.
/// Implementations qualify installed capabilities, authorize dependencies and recover
/// original pins, never current defaults.
#[async_trait::async_trait]
pub trait FactoryRoleInvocationSource: Send + Sync {
  /// Rechecks current managed access for a retained package without reading authoring catalogs.
  /// Sources without that authority may recover packages with no managed connections.
  async fn authorize_dependencies(&self, invocation: &FactoryRoleInvocation) -> Result<(), ApplicationError> {
    if invocation
      .package()
      .inputs()
      .profile
      .definition()
      .execution
      .integrations
      .is_empty()
    {
      Ok(())
    } else {
      Err(ApplicationError::capability_unavailable())
    }
  }

  /// Resolves the package for this immutable Build intent through the required read window.
  /// Required for selected profiles and unmatched role-bound task identities.
  /// None cannot authorize a fallback to a static task. Committed packages are
  /// recovered through the publication store before this source is consulted.
  async fn invocation(
    &self,
    intent: &FlowBuildIntent,
    observed_at: octacity_server_domain::Timestamp,
    required_until: octacity_server_domain::Timestamp,
  ) -> Result<Option<FactoryRoleInvocation>, ApplicationError>;
}

pub(crate) async fn prepare_factory_role_invocation<
  S: octacity_server_store::ProjectSkillImportStore,
  B: octacity_artifact_store::ArtifactByteReader,
>(
  store: &S,
  bytes: &B,
  package: FactoryRoleExecutionPackage,
  secret_variables: BTreeMap<String, LogicalSecretReference>,
) -> Result<FactoryRoleInvocation, ApplicationError> {
  let selected = package
    .inputs()
    .skills
    .iter()
    .map(|(alias, bundle)| {
      Ok((
        octacity_server_factory::FactoryRepositoryPath::new(format!(
          "{FACTORY_ROLE_SKILL_DIRECTORY}/{}",
          alias.as_str()
        ))
        .map_err(|_| ApplicationError::invalid())?,
        bundle.clone(),
      ))
    })
    .collect::<Result<BTreeMap<_, _>, ApplicationError>>()?;
  let skills = if selected.is_empty() {
    None
  } else {
    Some(
      crate::factory_skill_import::materialize_project_skills(
        store,
        bytes,
        package.inputs().profile.reference().project_id,
        selected,
      )
      .await?,
    )
  };
  FactoryRoleInvocation::new(package, skills, secret_variables)
}
