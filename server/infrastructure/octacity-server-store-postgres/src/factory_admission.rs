use std::str::FromStr as _;

use octacity_server_domain::{EntityKind, RepositoryVersion, Timestamp};
use octacity_server_factory::{
  BudgetUsage, DecisionSignalProgress, FactoryDigest, FactoryKey, FactoryLifecycleProgress, FactoryRun,
  FactoryRunState, FactoryRunVersion, WorkEnvelope,
};
use octacity_server_store::{
  AdmitFactoryWork, FactoryAdmissionContext, FactoryAdmissionMutationOutcome, FactoryAdmissionProbe, FactoryAuditFact,
  FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryOutboxRecord, ManagementMutation, MutationAuditContext,
  MutationDisposition, PublishedFactoryAdmission, ReadFactoryAdmissionContext, StoreError, StoreInputError,
  StoreOperation,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json};

use crate::{
  database::{classify, unavailable},
  discovery::{positive, timestamp},
  mutation::{
    MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome, stable_record_id,
  },
};

#[derive(Serialize)]
struct AdmissionFingerprint<'a> {
  source: &'a FactoryKey,
  security_scope: &'a str,
  external_identity: &'a octacity_server_factory::ExternalWorkIdentity,
  intent_digest: FactoryDigest,
}

#[derive(FromRow)]
struct AdmissionRow {
  envelope: Json<WorkEnvelope>,
  intent_digest: Vec<u8>,
  run_id: uuid::Uuid,
  state: String,
  version: i64,
  admitted_at_millis: i64,
}

pub(crate) async fn replay(
  pool: &PgPool,
  probe: &FactoryAdmissionProbe,
) -> Result<Option<FactoryAdmissionMutationOutcome>, StoreError> {
  let identity = replay_identity(probe)?;
  if let Some((digest, Json(outcome))) = sqlx::query_as::<_, (Vec<u8>, Json<Value>)>(
    "SELECT request_digest, outcome FROM idempotency_records \
     WHERE scope = $1 AND security_scope = $2 AND idempotency_key = $3",
  )
  .bind(identity.kind.scope())
  .bind(identity.persisted_security_scope())
  .bind(&identity.key)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  {
    if digest != identity.request_digest {
      return Err(conflict(EntityKind::FactoryRun));
    }
    return Ok(Some(replay_outcome(outcome)?));
  }

  let row = sqlx::query_as::<_, AdmissionRow>(
    "SELECT work.envelope, work.intent_digest, run.id AS run_id, run.state, run.version, \
            FLOOR(EXTRACT(EPOCH FROM run.admitted_at) * 1000)::BIGINT AS admitted_at_millis \
     FROM factory_work_envelopes AS work \
     JOIN factory_runs AS run ON run.work_envelope_id = work.id \
     WHERE work.source_kind = $1 AND work.security_scope_digest = $2 AND work.external_identity = $3",
  )
  .bind(probe.source_scope.source.as_str())
  .bind(security_scope_digest(probe).as_bytes().as_slice())
  .bind(probe.external_identity.as_str())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let Some(row) = row else {
    return Ok(None);
  };
  if row.intent_digest.as_slice() != probe.intent_digest.as_bytes() {
    return Err(conflict(EntityKind::FactoryRun));
  }
  Ok(Some(FactoryAdmissionMutationOutcome {
    disposition: MutationDisposition::Replayed,
    admission: decode_admission(row)?,
  }))
}

pub(crate) async fn context(
  pool: &PgPool,
  request: ReadFactoryAdmissionContext,
) -> Result<FactoryAdmissionContext, StoreError> {
  crate::discovery::require_project(pool, request.project_id).await?;
  let configuration =
    crate::factory_configuration::read(pool, request.configuration_id, request.configuration_version).await?;
  if configuration.configuration.reference().project_id() != request.project_id {
    return Err(not_found(EntityKind::FactoryConfiguration));
  }
  let repository =
    crate::configuration_query::repository(pool, request.repository_id, request.repository_version).await?;
  if repository.project_id != request.project_id {
    return Err(not_found(EntityKind::Repository));
  }
  Ok(FactoryAdmissionContext {
    configuration,
    repository,
  })
}

pub(crate) async fn admit(
  pool: &PgPool,
  request: ManagementMutation<AdmitFactoryWork>,
) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
  let (request, audit) = request.into_parts();
  validate_admission(&request, &audit)?;
  let identity = mutation_identity(&request.probe, request.admitted_at, &audit)?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_outcome(outcome),
  };
  let configuration = lock_configuration(&mut transaction, &request).await?;
  require_repository(&mut transaction, &request).await?;
  let active: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM factory_runs \
     WHERE factory_configuration_id = $1 AND factory_configuration_version = $2 \
       AND visible \
       AND state NOT IN ('rejected', 'cancelled', 'completed')",
  )
  .bind(request.work.configuration().id().as_uuid())
  .bind(version_number(request.work.configuration().version().get())?)
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if u64::try_from(active).map_err(|_| StoreError::Unavailable)?
    >= u64::from(configuration.wip_limits().max_active_runs())
  {
    return Err(conflict(EntityKind::FactoryRun));
  }

  let envelope_bytes = serde_json::to_vec(&request.work).map_err(|_| StoreError::Unavailable)?;
  let envelope_digest = FactoryDigest::sha256("octacity.factory.work-envelope.v1", &[&envelope_bytes]);
  let subject_digest = subject_digest(&request.work);
  sqlx::query(
    "INSERT INTO factory_work_envelopes \
       (id, project_id, factory_configuration_id, factory_configuration_version, source_kind, \
        security_scope_digest, external_identity, repository_id, repository_version, exact_revision, \
        intent_digest, envelope_digest, envelope, admitted_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, \
             to_timestamp($14::double precision / 1000.0))",
  )
  .bind(request.work.id().as_uuid())
  .bind(request.work.subject().project_id().as_uuid())
  .bind(request.work.configuration().id().as_uuid())
  .bind(version_number(request.work.configuration().version().get())?)
  .bind(request.probe.source_scope.source.as_str())
  .bind(security_scope_digest(&request.probe).as_bytes().as_slice())
  .bind(request.probe.external_identity.as_str())
  .bind(request.work.subject().repository_id().as_uuid())
  .bind(repository_version_number(request.repository_version)?)
  .bind(request.work.subject().base_revision().as_str())
  .bind(request.probe.intent_digest.as_bytes().as_slice())
  .bind(envelope_digest.as_bytes().as_slice())
  .bind(Json(&request.work))
  .bind(request.admitted_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::FactoryRun))?;
  sqlx::query(
    "INSERT INTO factory_runs \
       (id, project_id, work_envelope_id, factory_configuration_id, factory_configuration_version, \
        state, version, subject_digest, admitted_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, \
             to_timestamp($9::double precision / 1000.0), to_timestamp($9::double precision / 1000.0))",
  )
  .bind(request.run.id().as_uuid())
  .bind(request.work.subject().project_id().as_uuid())
  .bind(request.work.id().as_uuid())
  .bind(request.work.configuration().id().as_uuid())
  .bind(version_number(request.work.configuration().version().get())?)
  .bind(request.run.state().as_str())
  .bind(version_number(request.run.version().get())?)
  .bind(subject_digest.as_bytes().as_slice())
  .bind(request.admitted_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::FactoryRun))?;

  crate::factory_run::insert_artifact_reference(
    &mut transaction,
    request.run.id(),
    request.work.artifacts().task(),
    octacity_server_store::FactoryArtifactRole::Task,
    None,
    request.admitted_at,
  )
  .await?;
  crate::factory_run::insert_artifact_reference(
    &mut transaction,
    request.run.id(),
    request.work.artifacts().acceptance(),
    octacity_server_store::FactoryArtifactRole::Acceptance,
    None,
    request.admitted_at,
  )
  .await?;
  for artifact_id in request.work.artifacts().specifications() {
    crate::factory_run::insert_artifact_reference(
      &mut transaction,
      request.run.id(),
      *artifact_id,
      octacity_server_store::FactoryArtifactRole::Specification,
      None,
      request.admitted_at,
    )
    .await?;
  }

  let budget = FactoryBudgetRecord::new(
    request.run.id(),
    FactoryRunVersion::INITIAL,
    BudgetUsage::default(),
    request.admitted_at,
  );
  let lifecycle = FactoryLifecycleCheckpoint::new(
    request.run.id(),
    FactoryRunVersion::INITIAL,
    FactoryLifecycleProgress::Admitted,
    DecisionSignalProgress::Disabled,
    false,
    request.admitted_at,
  );
  insert_initial_projection(&mut transaction, &request, &budget, &lifecycle).await?;
  let outbox = admission_outbox(&request);
  insert_factory_outbox(&mut transaction, &outbox).await?;
  insert_audit_link(&mut transaction, &request, &audit, &identity).await?;

  let admission = PublishedFactoryAdmission {
    work: request.work,
    run: request.run,
    admitted_at: request.admitted_at,
  };
  let outcome = FactoryAdmissionMutationOutcome {
    disposition: MutationDisposition::Applied,
    admission,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts::management(
      &audit,
      outcome.admission.run.id().to_string(),
      json!({
        "factory_configuration_id": outcome.admission.run.configuration().id(),
        "project_id": outcome.admission.run.subject().project_id(),
        "state": outcome.admission.run.state().as_str(),
      }),
      json!({
        "event": MutationKind::AdmitFactoryWork.outbox_topic(),
        "factory_run_id": outcome.admission.run.id(),
        "project_id": outcome.admission.run.subject().project_id(),
      }),
    ),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

fn mutation_identity(
  probe: &FactoryAdmissionProbe,
  occurred_at: Timestamp,
  audit: &MutationAuditContext,
) -> Result<MutationIdentity, StoreError> {
  let fingerprint = AdmissionFingerprint {
    source: &probe.source_scope.source,
    security_scope: probe.source_scope.security_scope.as_str(),
    external_identity: &probe.external_identity,
    intent_digest: probe.intent_digest,
  };
  MutationIdentity::new_management(
    audit,
    MutationKind::AdmitFactoryWork,
    probe.idempotency_key.to_string(),
    occurred_at,
    EntityKind::FactoryRun,
    &fingerprint,
  )
}

fn replay_identity(probe: &FactoryAdmissionProbe) -> Result<MutationIdentity, StoreError> {
  let fingerprint = AdmissionFingerprint {
    source: &probe.source_scope.source,
    security_scope: probe.source_scope.security_scope.as_str(),
    external_identity: &probe.external_identity,
    intent_digest: probe.intent_digest,
  };
  MutationIdentity::new_management_replay(
    &probe.source_scope.security_scope,
    MutationKind::AdmitFactoryWork,
    probe.idempotency_key.to_string(),
    Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
    EntityKind::FactoryRun,
    &fingerprint,
  )
}

fn validate_admission(request: &AdmitFactoryWork, audit: &MutationAuditContext) -> Result<(), StoreError> {
  request.validate(audit.security_scope())
}

async fn lock_configuration(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AdmitFactoryWork,
) -> Result<octacity_server_factory::FactoryConfiguration, StoreError> {
  let row: Option<(Json<octacity_server_factory::FactoryConfiguration>, bool)> = sqlx::query_as(
    "SELECT definition, enabled FROM factory_configuration_versions \
     WHERE factory_configuration_id = $1 AND version = $2 FOR KEY SHARE",
  )
  .bind(request.work.configuration().id().as_uuid())
  .bind(version_number(request.work.configuration().version().get())?)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let Some((Json(configuration), enabled)) = row else {
    return Err(not_found(EntityKind::FactoryConfiguration));
  };
  if !enabled || !configuration.is_enabled() || configuration.reference() != request.work.configuration() {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::AdmitFactoryWork,
      source: StoreInputError::InvalidFactoryAdmission,
    });
  }
  Ok(configuration)
}

async fn require_repository(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AdmitFactoryWork,
) -> Result<(), StoreError> {
  let owner: Option<uuid::Uuid> = sqlx::query_scalar(
    "SELECT repository.project_id FROM repository_versions AS version \
     JOIN repositories AS repository ON repository.id = version.repository_id \
     WHERE version.repository_id = $1 AND version.version = $2 FOR KEY SHARE",
  )
  .bind(request.work.subject().repository_id().as_uuid())
  .bind(repository_version_number(request.repository_version)?)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  match owner {
    Some(project_id) if project_id == request.work.subject().project_id().as_uuid() => Ok(()),
    _ => Err(not_found(EntityKind::Repository)),
  }
}

async fn insert_initial_projection(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AdmitFactoryWork,
  budget: &FactoryBudgetRecord,
  lifecycle: &FactoryLifecycleCheckpoint,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_run_budgets (id, run_id, run_version, usage, recorded_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0))",
  )
  .bind(budget.id.as_bytes().as_slice())
  .bind(request.run.id().as_uuid())
  .bind(version_number(budget.run_version.get())?)
  .bind(Json(budget.usage))
  .bind(budget.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query(
    "INSERT INTO factory_lifecycle_checkpoints \
       (id, run_id, run_version, lifecycle, cancellation_requested, recorded_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
  )
  .bind(lifecycle.id.as_bytes().as_slice())
  .bind(request.run.id().as_uuid())
  .bind(version_number(lifecycle.run_version.get())?)
  .bind(Json(
    json!({"progress": lifecycle.progress, "signal": lifecycle.signal}),
  ))
  .bind(lifecycle.cancellation_requested)
  .bind(lifecycle.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query(
    "INSERT INTO factory_run_current (run_id, run_version, budget_id, lifecycle_checkpoint_id) \
     VALUES ($1, $2, $3, $4)",
  )
  .bind(request.run.id().as_uuid())
  .bind(version_number(request.run.version().get())?)
  .bind(budget.id.as_bytes().as_slice())
  .bind(lifecycle.id.as_bytes().as_slice())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

fn admission_outbox(request: &AdmitFactoryWork) -> FactoryOutboxRecord {
  let operation_id = FactoryDigest::sha256(
    "octacity.factory.admission-outbox-operation.v1",
    &[
      request.run.id().as_uuid().as_bytes(),
      &request.probe.intent_digest.as_bytes(),
    ],
  );
  FactoryOutboxRecord::pending(
    operation_id,
    request.run.id(),
    FactoryKey::new("factory.run.admitted").expect("static Factory key is valid"),
    request.probe.intent_digest,
    request.admitted_at,
    request.admitted_at,
  )
}

async fn insert_factory_outbox(
  transaction: &mut Transaction<'_, Postgres>,
  record: &FactoryOutboxRecord,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_outbox_records \
       (id, operation_id, run_id, kind, input_digest, state, attempt, available_at, recorded_at) \
     VALUES ($1, $2, $3, $4, $5, 'pending', $6, \
             to_timestamp($7::double precision / 1000.0), to_timestamp($8::double precision / 1000.0))",
  )
  .bind(record.id.as_bytes().as_slice())
  .bind(record.operation_id.as_bytes().as_slice())
  .bind(record.run_id.as_uuid())
  .bind(record.kind.as_str())
  .bind(record.input_digest.as_bytes().as_slice())
  .bind(i32::from(record.attempt))
  .bind(record.available_at.unix_millis())
  .bind(record.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn insert_audit_link(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AdmitFactoryWork,
  audit: &MutationAuditContext,
  identity: &MutationIdentity,
) -> Result<(), StoreError> {
  let actor_identity_digest = audit
    .actor()
    .identity
    .as_deref()
    .map(|identity| FactoryDigest::sha256("octacity.factory.audit-actor.v1", &[identity.as_bytes()]));
  let request_digest = FactoryDigest::sha256(
    "octacity.factory.audit-request.v1",
    &[audit.request_identity().as_bytes()],
  );
  let fact = FactoryAuditFact::new(
    request.run.id(),
    audit.actor().kind,
    actor_identity_digest,
    FactoryKey::new("work.admitted").expect("static Factory key is valid"),
    request_digest,
    FactoryKey::new("accepted").expect("static Factory key is valid"),
    request.admitted_at,
  );
  sqlx::query(
    "INSERT INTO factory_audit_links \
       (id, run_id, audit_fact_id, actor_kind, actor_identity_digest, operation, \
        request_identity_digest, outcome, recorded_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0))",
  )
  .bind(fact.id.as_bytes().as_slice())
  .bind(request.run.id().as_uuid())
  .bind(stable_record_id("audit", identity))
  .bind(fact.actor_kind.as_str())
  .bind(fact.actor_identity_digest.map(FactoryDigest::as_bytes))
  .bind(fact.operation.as_str())
  .bind(fact.request_identity_digest.as_bytes().as_slice())
  .bind(fact.outcome.as_str())
  .bind(fact.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

fn decode_admission(row: AdmissionRow) -> Result<PublishedFactoryAdmission, StoreError> {
  let work = row.envelope.0;
  let state = FactoryRunState::from_str(&row.state).map_err(|_| StoreError::Unavailable)?;
  let version = FactoryRunVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?;
  let run = FactoryRun::restore(
    octacity_server_factory::FactoryRunId::from_uuid(row.run_id).map_err(|_| StoreError::Unavailable)?,
    work.configuration().clone(),
    &work,
    work.subject().clone(),
    state,
    version,
  )
  .map_err(|_| StoreError::Unavailable)?;
  Ok(PublishedFactoryAdmission {
    work,
    run,
    admitted_at: timestamp(row.admitted_at_millis)?,
  })
}

fn replay_outcome(value: Value) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
  let mut outcome: FactoryAdmissionMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn security_scope_digest(probe: &FactoryAdmissionProbe) -> FactoryDigest {
  FactoryDigest::sha256(
    "octacity.factory.security-scope.v1",
    &[probe.source_scope.security_scope.as_str().as_bytes()],
  )
}

fn subject_digest(work: &WorkEnvelope) -> FactoryDigest {
  FactoryDigest::sha256(
    "octacity.factory.subject.v1",
    &[
      work.subject().project_id().as_uuid().as_bytes(),
      work.subject().repository_id().as_uuid().as_bytes(),
      work.subject().base_revision().as_str().as_bytes(),
    ],
  )
}

fn version_number(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| StoreError::Unavailable)
}

fn repository_version_number(value: RepositoryVersion) -> Result<i64, StoreError> {
  version_number(value.get())
}

const fn not_found(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}

const fn conflict(entity: EntityKind) -> StoreError {
  StoreError::Conflict { entity }
}
