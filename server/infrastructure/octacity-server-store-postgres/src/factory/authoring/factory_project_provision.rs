//! Explicit initialization over the existing policy journal and mutation transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  factory::authoring::factory_project_policy::{append_policy, lock_project},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::*;
use octacity_server_store::*;
use serde_json::json;
use sqlx::types::Json;

pub(crate) async fn read<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
) -> Result<Option<FactoryProjectPolicyProvisioning>, StoreError> {
  let row: Option<Json<FactoryProjectPolicyProvisioning>> =
    sqlx::query_scalar("SELECT document FROM factory_project_policy_provisioning WHERE project_id=$1")
      .bind(project.as_uuid())
      .fetch_optional(executor)
      .await
      .map_err(unavailable)?;
  row
    .map(|Json(value)| {
      if value.project_id == project
        && value.standard_launch.project_id == project
        && value.standard_access.project_id == project
      {
        Ok(value)
      } else {
        Err(StoreError::Unavailable)
      }
    })
    .transpose()
}

pub(crate) async fn provision(
  store: &PostgresStore,
  request: ManagementMutation<ProvisionFactoryProjectPolicies>,
) -> Result<ProvisionFactoryProjectPoliciesOutcome, StoreError> {
  let (request, audit) = request.into_parts();
  request.intent.validate()?;
  let project = request.intent.project_id;
  let identity = MutationIdentity::new_management(
    &audit,
    MutationKind::ProvisionFactoryProjectPolicies,
    request.idempotency_key.to_string(),
    request.provisioned_at,
    EntityKind::Project,
    &request.intent,
  )?;
  let mut tx = match crate::mutation::begin(store.pool(), &identity).await? {
    MutationStart::Replay(value) => {
      let mut outcome: ProvisionFactoryProjectPoliciesOutcome = decode_outcome(value)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
    MutationStart::Fresh(tx) => tx,
  };
  lock_project(&mut *tx, project).await?;
  let existing = read(&mut *tx, project).await?;
  let mut installed = None;
  let created = existing.is_none();
  let provisioning = match existing {
    Some(value) => value,
    None => {
      let declared = match &request.intent.designated_access {
        None => Err(FactoryPolicyProvisionPrerequisite::NoDesignatedAccessDefault),
        Some(reference) => {
          installed = Some(store.factory_capabilities().availability(project).await?);
          match installed.as_ref().expect("loaded installed declarations") {
            FactoryConfigurationAvailability::Available(choices) => {
              factory_policy_access_declaration(choices, reference)
            }
            FactoryConfigurationAvailability::Disabled => {
              Err(FactoryPolicyProvisionPrerequisite::ReferenceNotInstalled)
            }
          }
        }
      };
      let (permissions, access_origin, access_prerequisite) = match declared {
        Ok(permissions) => (permissions, request.intent.designated_access.clone(), None),
        Err(reason) => (FactoryPermissionSet::deny_all(), None, Some(reason)),
      };
      let launch = FactoryLaunchPolicy::publish(
        FactoryLaunchPolicyId::generate(),
        project,
        FactoryLaunchPolicyDefinition {
          name: FactoryText::new("Standard").map_err(|_| invalid())?,
          allow_manual: true,
          allow_automatic_source: true,
        },
      )
      .map_err(|_| invalid())?;
      let access = FactoryAccessProfile::publish(
        FactoryAccessProfileId::generate(),
        project,
        FactoryAccessProfileDefinition {
          name: FactoryText::new("Standard").map_err(|_| invalid())?,
          permissions,
        },
      )
      .map_err(|_| invalid())?;
      let value = FactoryProjectPolicyProvisioning {
        project_id: project,
        standard_launch: launch.reference().clone(),
        standard_access: access.reference().clone(),
        access_origin,
        access_prerequisite,
      };
      append_policy(&mut *tx, &FactoryProjectPolicyRecord::Launch(launch)).await?;
      append_policy(&mut *tx, &FactoryProjectPolicyRecord::Access(access.into())).await?;
      sqlx::query("INSERT INTO factory_project_policy_provisioning(project_id,document) VALUES ($1,$2)")
        .bind(project.as_uuid())
        .bind(Json(&value))
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
      value
    }
  };
  let current: Option<Json<FactoryProjectPolicyDefaults>> = sqlx::query_scalar(
    "SELECT document FROM factory_project_policy_defaults WHERE project_id=$1 ORDER BY version DESC LIMIT 1",
  )
  .bind(project.as_uuid())
  .fetch_optional(&mut *tx)
  .await
  .map_err(unavailable)?;
  let defaults = match current {
    Some(Json(value)) if value.project_id == project => value,
    Some(_) => return Err(StoreError::Unavailable),
    None => {
      let value = FactoryProjectPolicyDefaults {
        project_id: project,
        version: 1,
        launch_policy: Some(provisioning.standard_launch.clone()),
        access_profile: Some(provisioning.standard_access.clone()),
      };
      sqlx::query("INSERT INTO factory_project_policy_defaults(project_id,version,document) VALUES ($1,1,$2)")
        .bind(project.as_uuid())
        .bind(Json(&value))
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
      value
    }
  };
  let mut mappings = Vec::new();
  let mut unresolved = Vec::new();
  for legacy in &request.intent.legacy_references {
    if let Some(mapping) =
      crate::factory::authoring::factory_project_policy_mapping::read(&mut *tx, project, legacy).await?
    {
      mappings.push(mapping);
      continue;
    }
    if installed.is_none() {
      installed = Some(store.factory_capabilities().availability(project).await?);
    }
    match crate::factory::authoring::factory_project_policy_mapping::import(
      &mut tx,
      project,
      legacy,
      installed.as_ref().expect("loaded installed declarations"),
      &provisioning.standard_launch,
    )
    .await?
    {
      Ok(mapping) => mappings.push(mapping),
      Err(missing) => unresolved.push(missing),
    }
  }
  let metadata =
    json!({"created":created,"provisioning":provisioning,"mappings":mappings.len(),"unresolved":unresolved.len()});
  let outcome = ProvisionFactoryProjectPoliciesOutcome {
    disposition: MutationDisposition::Applied,
    created,
    provisioning,
    defaults,
    mappings,
    unresolved,
    provisioned_at: request.provisioned_at,
  };
  crate::mutation::commit(
    tx,
    &identity,
    MutationFacts::management(&audit, project.to_string(), metadata.clone(), metadata),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) fn invalid() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::ProvisionFactoryProjectPolicies,
    source: StoreInputError::InvalidFactoryProjectPolicy,
  }
}
