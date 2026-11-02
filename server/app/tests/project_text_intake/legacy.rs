//! Frozen pre-policy admission for the pre-0093 migration rehearsal.
//! A current application must never be used as the old writer or granted a production bypass.
use super::*;

pub(super) async fn admit(
  pool: &sqlx::PgPool,
  store: &PostgresStore,
  submission: &ProjectTextSubmission,
) -> PublishedFactoryAdmission {
  let selected = submission.intent.target;
  let configuration = store
    .factory_configuration_version(selected.configuration_id, selected.configuration_version)
    .await
    .unwrap()
    .configuration;
  let retained = store
    .reserve_project_text(management_mutation(ReserveProjectText {
      submission: submission.clone(),
      now: at(20),
    }))
    .await
    .unwrap();
  let revision = ImmutableRevision::new("original-base").unwrap();
  let retained = store
    .freeze_project_text(retained.guard(at(20)), revision.clone())
    .await
    .unwrap();
  let mut retained = store.publish_project_text(retained.guard(at(20))).await.unwrap();
  let intent = retained.admission_intent().unwrap();
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    intent.external_identity.clone(),
    ExactSubject::new(selected.project_id, selected.repository_id.unwrap(), revision),
    retained.work_artifacts().unwrap(),
    intent.classification.clone(),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = AdmittedFlow::from_configuration(&configuration, &run).unwrap();
  let admission = postgres::legacy_factory_admission::seed(
    pool,
    AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: "manual".parse().unwrap(),
          security_scope: ManagementSecurityScope::trusted_network(),
        },
        external_identity: intent.external_identity.clone(),
        intent_digest: factory_admission_intent_digest(&intent, "octacity.factory.manual-admission.v1"),
        idempotency_key: intent.idempotency_key,
      },
      repository_version: selected.repository_version,
      work,
      run,
      flow,
      admitted_at: retained.reserved_at,
    },
  )
  .await;
  retained.admission = Some(admission.clone());
  sqlx::query("UPDATE project_text_preparations SET document=$2 WHERE id=$1")
    .bind(retained.id.as_bytes().as_slice())
    .bind(sqlx::types::Json(&retained))
    .execute(pool)
    .await
    .unwrap();
  admission
}
