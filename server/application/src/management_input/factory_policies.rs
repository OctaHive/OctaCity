//! Typed policy request construction; no transport caller computes publication digests.
use super::*;
impl ManagementInputFactory {
  /// Parses explicit initial authority and bounded backfill without accepting client declarations.
  pub fn provision_factory_project_policies(
    &self,
    project: &str,
    draft: crate::FactoryProjectPolicyProvisionDraft,
    key: &str,
    now: i64,
  ) -> Result<crate::ProvisionFactoryProjectPoliciesCommand, ManagementInputError> {
    let intent = octacity_server_store::FactoryProjectProvisionIntent {
      project_id: parse(project, "project_id")?,
      designated_access: draft.designated_access,
      legacy_references: draft.legacy_references,
    };
    intent
      .validate()
      .map_err(|_| ManagementInputError::Invalid("policy provisioning"))?;
    Ok(crate::ProvisionFactoryProjectPoliciesCommand {
      request: octacity_server_store::ProvisionFactoryProjectPolicies {
        intent,
        idempotency_key: parse(key, "idempotency_key")?,
        provisioned_at: timestamp(now)?,
      },
    })
  }

  /// Resolves only an explicit legacy installation reference; Project pins use exact content reads.
  pub fn resolve_factory_legacy_policy(
    &self,
    project: &str,
    legacy: crate::FactoryLegacyPolicyReference,
  ) -> Result<crate::ResolveFactoryLegacyPolicyQuery, ManagementInputError> {
    Ok(crate::ResolveFactoryLegacyPolicyQuery {
      project_id: parse(project, "project_id")?,
      legacy,
    })
  }

  /// Parses a bounded typed continuation returned by the current-policy catalog.
  pub fn list_factory_project_policies(
    &self,
    project: &str,
    after: Option<&str>,
    limit: u16,
  ) -> Result<crate::ListFactoryProjectPoliciesQuery, ManagementInputError> {
    let after = after
      .map(|value| {
        serde_json::from_str::<crate::FactoryProjectPolicyRef>(value)
          .map_err(|_| ManagementInputError::Invalid("policy cursor"))
      })
      .transpose()?;
    let project_id = parse(project, "project_id")?;
    if after.as_ref().is_some_and(|r| r.project_id() != project_id) {
      return Err(ManagementInputError::Invalid("policy cursor"));
    }
    let page = crate::CurrentDefinitionPageInput::try_new(project_id, after, limit)
      .map_err(|_| ManagementInputError::Invalid("limit"))?;
    Ok(crate::ListFactoryProjectPoliciesQuery { page })
  }
  /// Reads explicit defaults without provisioning them.
  pub fn get_factory_project_policy_defaults(
    &self,
    project: &str,
  ) -> Result<crate::GetFactoryProjectPolicyDefaultsQuery, ManagementInputError> {
    Ok(crate::GetFactoryProjectPolicyDefaultsQuery {
      project_id: parse(project, "project_id")?,
    })
  }
  /// Validates the entire reviewed default selection before the durable mutation.
  pub fn select_factory_project_policy_defaults(
    &self,
    project: &str,
    draft: crate::FactoryProjectPolicyDefaultsDraft,
    key: &str,
    now: i64,
  ) -> Result<crate::SelectFactoryProjectPolicyDefaultsCommand, ManagementInputError> {
    let intent = octacity_server_store::FactoryProjectPolicyDefaultsIntent {
      project_id: parse(project, "project_id")?,
      expected_version: draft.expected_version,
      launch_policy: draft.launch_policy,
      access_profile: draft.access_profile,
    };
    intent
      .defaults()
      .map_err(|_| ManagementInputError::Invalid("policy defaults"))?;
    Ok(crate::SelectFactoryProjectPolicyDefaultsCommand {
      intent,
      idempotency_key: parse(key, "idempotency_key")?,
      published_at: timestamp(now)?,
    })
  }

  /// Validates the same original candidate without a mutation key or clock.
  pub fn validate_factory_project_policy(
    &self,
    project: &str,
    draft: crate::FactoryProjectPolicyDraft,
  ) -> Result<crate::ValidateFactoryProjectPolicyQuery, ManagementInputError> {
    Ok(crate::ValidateFactoryProjectPolicyQuery {
      intent: draft
        .publication(parse(project, "project_id")?)
        .map_err(|_| ManagementInputError::Invalid("project policy"))?,
    })
  }

  /// Parses the owning Project, original draft and scoped publication identity.
  pub fn publish_factory_project_policy(
    &self,
    project: &str,
    draft: crate::FactoryProjectPolicyDraft,
    key: &str,
    now: i64,
  ) -> Result<crate::PublishFactoryProjectPolicyCommand, ManagementInputError> {
    Ok(crate::PublishFactoryProjectPolicyCommand {
      intent: draft
        .publication(parse(project, "project_id")?)
        .map_err(|_| ManagementInputError::Invalid("project policy"))?,
      idempotency_key: parse(key, "idempotency_key")?,
      published_at: timestamp(now)?,
    })
  }
  /// Constructs an exact typed reference; a mismatched digest cannot select another revision.
  pub fn get_factory_project_policy(
    &self,
    project: &str,
    kind: &str,
    id: &str,
    revision: u64,
    digest: &str,
  ) -> Result<crate::GetFactoryProjectPolicyQuery, ManagementInputError> {
    use octacity_server_factory::*;
    let project_id = parse(project, "project_id")?;
    let digest = parse(digest, "digest")?;
    let reference = match kind {
      "launch" => crate::FactoryProjectPolicyRef::Launch(FactoryLaunchPolicyRef {
        project_id,
        id: parse(id, "policy_id")?,
        version: FactoryLaunchPolicyVersion::new(revision).map_err(|_| ManagementInputError::Invalid("version"))?,
        digest,
      }),
      "access" => crate::FactoryProjectPolicyRef::Access(FactoryAccessProfileRef {
        project_id,
        id: parse(id, "policy_id")?,
        version: FactoryAccessProfileVersion::new(revision).map_err(|_| ManagementInputError::Invalid("version"))?,
        digest,
      }),
      _ => return Err(ManagementInputError::Invalid("policy kind")),
    };
    Ok(crate::GetFactoryProjectPolicyQuery { reference })
  }
}
