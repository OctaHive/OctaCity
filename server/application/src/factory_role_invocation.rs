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
pub struct FactoryRoleInvocation {
  package: FactoryRoleExecutionPackage,
  skills: Option<ProtectedInputManifestV3>,
  secret_variables: BTreeMap<String, LogicalSecretReference>,
}
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
    Ok(Self {
      package,
      skills,
      secret_variables,
    })
  }
  /// Complete immutable execution content, independent of runtime/provider representation.
  pub const fn package(&self) -> &FactoryRoleExecutionPackage {
    &self.package
  }
  /// Complete selected skill transfer, already verified by its retained owner.
  pub const fn skills(&self) -> Option<&ProtectedInputManifestV3> {
    self.skills.as_ref()
  }
  /// Qualified Octa variable names to existing logical credential locators.
  pub const fn secret_variables(&self) -> &BTreeMap<String, LogicalSecretReference> {
    &self.secret_variables
  }
  /// Qualifies complete review authority through the existing purpose-bound credentials.
  /// A narrower task cannot disguise a writable or credential-incompatible profile.
  pub(crate) fn verify_consumer(
    &self,
    consumer: octacity_server_factory::FactoryCredentialConsumer,
    credentials: &octacity_server_factory::FactoryCredentialProfiles,
  ) -> Result<(), ApplicationError> {
    if consumer != octacity_server_factory::FactoryCredentialConsumer::ReadOnlyReview {
      return Ok(());
    }
    let inputs = self.package.inputs();
    let permissions = &inputs.profile.definition().permissions;
    for profile in permissions.secret_profiles() {
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
    let inputs = self.package.inputs();
    inputs
      .task
      .verify_build_intent(intent)
      .map_err(|_| ApplicationError::invalid())?;
    if inputs.profile.definition().build_configuration != intent.profile().build_configuration
      || inputs.task.permissions() != intent.permissions()
      || inputs.task.budget() != intent.budget()
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
}

/// Exact retained selection supplied to installed compilers during input recovery.
/// Implementations qualify installed capabilities, authorize dependencies and recover
/// original pins, never current defaults.
#[async_trait::async_trait]
pub trait FactoryRoleInvocationSource: Send + Sync {
  /// Resolves the package for this immutable Build intent through the required read window.
  /// Called only when no installed static task matches. None means no retained role
  /// invocation is available and cannot authorize a fallback to a static task.
  async fn invocation(
    &self,
    intent: &FlowBuildIntent,
    observed_at: octacity_server_domain::Timestamp,
    required_until: octacity_server_domain::Timestamp,
  ) -> Result<Option<FactoryRoleInvocation>, ApplicationError>;
}

impl<S: octacity_server_store::ProjectSkillImportStore, B: octacity_artifact_store::ArtifactByteReader>
  crate::ProjectSkillImport<S, B>
{
  /// Verifies the selected retained dependencies with the existing Project input reader.
  pub async fn prepare_invocation(
    &self,
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
        self
          .materialize(package.inputs().profile.reference().project_id, selected)
          .await?,
      )
    };
    FactoryRoleInvocation::new(package, skills, secret_variables)
  }
}
