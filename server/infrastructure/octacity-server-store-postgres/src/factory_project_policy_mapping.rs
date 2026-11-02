//! Exact compatibility provenance, independent of current heads or defaults.
use crate::{database::unavailable, factory_project_policy::append_policy, factory_project_provision::invalid};
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::*;
use sqlx::types::Json;

pub(super) async fn read<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
  legacy: &FactoryLegacyPolicyReference,
) -> Result<Option<FactoryLegacyPolicyMapping>, StoreError> {
  let row: Option<Json<FactoryLegacyPolicyMapping>> =
    sqlx::query_scalar("SELECT document FROM factory_project_policy_mappings WHERE project_id=$1 AND legacy_key=$2")
      .bind(project.as_uuid())
      .bind(legacy_key(legacy)?)
      .fetch_optional(executor)
      .await
      .map_err(unavailable)?;
  row
    .map(|Json(value)| {
      let kind_matches = matches!(
        (&value.legacy, &value.target),
        (
          FactoryLegacyPolicyReference::Launch(_),
          FactoryProjectPolicyRef::Launch(_)
        ) | (
          FactoryLegacyPolicyReference::Access(_),
          FactoryProjectPolicyRef::Access(_)
        )
      );
      if value.project_id != project || value.legacy != *legacy || value.target.project_id() != project || !kind_matches
      {
        Err(StoreError::Unavailable)
      } else {
        Ok(value)
      }
    })
    .transpose()
}

pub(super) async fn import(
  tx: &mut sqlx::PgConnection,
  project: ProjectId,
  legacy: &FactoryLegacyPolicyReference,
  installed: &FactoryConfigurationAvailability,
) -> Result<Result<FactoryLegacyPolicyMapping, FactoryLegacyPolicyUnresolved>, StoreError> {
  let unresolved = |reason| FactoryLegacyPolicyUnresolved {
    legacy: legacy.clone(),
    reason,
  };
  let FactoryConfigurationAvailability::Available(choices) = installed else {
    return Ok(Err(unresolved(
      FactoryPolicyProvisionPrerequisite::ReferenceNotInstalled,
    )));
  };
  let (kind, reference) = legacy.installed_choice();
  if !choices
    .entries()
    .references
    .iter()
    .any(|entry| entry.kind == kind && entry.reference == *reference)
  {
    return Ok(Err(unresolved(
      FactoryPolicyProvisionPrerequisite::ReferenceNotInstalled,
    )));
  }
  let name =
    FactoryText::new(format!("Imported {} {}", reference.identity(), reference.version())).map_err(|_| invalid())?;
  let policy = match legacy {
    FactoryLegacyPolicyReference::Launch(_) => FactoryProjectPolicyRecord::Launch(
      FactoryLaunchPolicy::publish(
        FactoryLaunchPolicyId::generate(),
        project,
        FactoryLaunchPolicyDefinition {
          name,
          allow_manual: true,
          allow_automatic_source: true,
        },
      )
      .map_err(|_| invalid())?,
    ),
    FactoryLegacyPolicyReference::Access(_) => {
      let permissions = match factory_policy_access_declaration(choices, reference) {
        Ok(value) => value,
        Err(reason) => return Ok(Err(unresolved(reason))),
      };
      FactoryProjectPolicyRecord::Access(
        FactoryAccessProfile::publish(
          FactoryAccessProfileId::generate(),
          project,
          FactoryAccessProfileDefinition { name, permissions },
        )
        .map_err(|_| invalid())?
        .into(),
      )
    }
  };
  let mapping = FactoryLegacyPolicyMapping {
    project_id: project,
    legacy: legacy.clone(),
    target: policy.reference(),
  };
  append_policy(&mut *tx, &policy).await?;
  sqlx::query("INSERT INTO factory_project_policy_mappings(project_id,legacy_key,document) VALUES ($1,$2,$3)")
    .bind(project.as_uuid())
    .bind(legacy_key(legacy)?)
    .bind(Json(&mapping))
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  Ok(Ok(mapping))
}

fn legacy_key(legacy: &FactoryLegacyPolicyReference) -> Result<Vec<u8>, StoreError> {
  let bytes = serde_json::to_vec(legacy).map_err(|_| invalid())?;
  Ok(
    FactoryDigest::sha256("octacity.factory.legacy-policy-reference.v1", &[&bytes])
      .as_bytes()
      .to_vec(),
  )
}
