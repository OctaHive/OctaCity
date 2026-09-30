mod support;

use std::sync::Arc;

use octacity_server_domain::{EntityKind, Timestamp, TriggerOccurrenceId};
use octacity_server_store::{
  ClaimTriggerEvaluations, CompleteTriggerEvaluation, FailTriggerEvaluation, ReserveTriggerEvaluation, StoreError,
  TriggerEvaluationReservation, TriggerEvaluationWorkStore as _, WorkerOwner, testing::management_mutation,
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::json;
use support::TestDatabase;
use tokio::sync::Barrier;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn trigger_retry_claims_are_exclusive_and_survive_rolling_restart() {
  let database = TestDatabase::migrated().await;
  let occurrence_id = TriggerOccurrenceId::from_uuid(uuid::Uuid::from_u128(10_200)).unwrap();
  let reservation = ReserveTriggerEvaluation {
    occurrence_id,
    intent_digest: [0x42; 32],
    payload: json!({"configuration_id": "configuration-1"}),
    owner: owner("replica:initial"),
    requested_at: time(1_000),
    claim_expires_at: time(2_000),
  };
  let initial = PostgresStore::new(independent_pool(&database.pool).await)
    .reserve_trigger_evaluation(management_mutation(reservation.clone()))
    .await
    .unwrap();
  assert!(matches!(initial, TriggerEvaluationReservation::Claimed(_)));

  let before_expiry = PostgresStore::new(independent_pool(&database.pool).await)
    .claim_trigger_evaluations(claim("replica:early", 1_999, 3_000))
    .await
    .unwrap();
  assert!(before_expiry.is_empty(), "a live claim must survive owner shutdown");

  let left = PostgresStore::new(independent_pool(&database.pool).await);
  let right = PostgresStore::new(independent_pool(&database.pool).await);
  let barrier = Arc::new(Barrier::new(2));
  let (left_claims, right_claims) = tokio::join!(
    claim_after_barrier(left, claim("replica:left", 2_000, 4_000), barrier.clone()),
    claim_after_barrier(right, claim("replica:right", 2_000, 4_000), barrier),
  );
  let mut recovered = [left_claims.unwrap(), right_claims.unwrap()].concat();
  assert_eq!(recovered.len(), 1, "only one replica may recover abandoned work");
  let recovered = recovered.pop().unwrap();
  assert_eq!(recovered.occurrence_id, occurrence_id);
  assert_eq!(recovered.attempt, 2);

  let replacement = PostgresStore::new(independent_pool(&database.pool).await);
  assert!(matches!(
    replacement
      .complete_trigger_evaluation(CompleteTriggerEvaluation {
        occurrence_id,
        owner: reservation.owner,
        completed_at: time(2_001),
      })
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::Trigger
    })
  ));
  replacement
    .fail_trigger_evaluation(FailTriggerEvaluation {
      occurrence_id,
      owner: recovered.owner,
      diagnostic: "transient VCS outage".to_owned(),
      failed_at: time(2_001),
      retry_at: Some(time(3_000)),
    })
    .await
    .unwrap();

  let restarted = PostgresStore::new(independent_pool(&database.pool).await);
  assert!(
    restarted
      .claim_trigger_evaluations(claim("replica:before-retry", 2_999, 4_000))
      .await
      .unwrap()
      .is_empty(),
    "retry work must remain unavailable until its durable next-attempt time"
  );
  let mut retry = restarted
    .claim_trigger_evaluations(claim("replica:retry", 3_000, 4_000))
    .await
    .unwrap();
  assert_eq!(retry.len(), 1);
  let retry = retry.pop().unwrap();
  assert_eq!(retry.attempt, 3);
  restarted
    .complete_trigger_evaluation(CompleteTriggerEvaluation {
      occurrence_id,
      owner: retry.owner,
      completed_at: time(3_001),
    })
    .await
    .unwrap();

  let after_restart = PostgresStore::new(independent_pool(&database.pool).await);
  assert_eq!(
    after_restart
      .reserve_trigger_evaluation(management_mutation(ReserveTriggerEvaluation {
        owner: owner("replica:replay"),
        requested_at: time(5_000),
        claim_expires_at: time(6_000),
        ..reservation
      }))
      .await
      .unwrap(),
    TriggerEvaluationReservation::Completed
  );
  assert!(
    after_restart
      .claim_trigger_evaluations(claim("replica:idle", 5_000, 6_000))
      .await
      .unwrap()
      .is_empty()
  );

  database.cleanup().await;
}

async fn claim_after_barrier(
  store: PostgresStore,
  request: ClaimTriggerEvaluations,
  barrier: Arc<Barrier>,
) -> Result<Vec<octacity_server_store::TriggerEvaluationClaim>, StoreError> {
  barrier.wait().await;
  store.claim_trigger_evaluations(request).await
}

async fn independent_pool(source: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::PgPool::connect_with((*source.connect_options()).clone())
    .await
    .expect("connect an independent server pool to the test database")
}

fn claim(owner_id: &str, observed_at: i64, claim_expires_at: i64) -> ClaimTriggerEvaluations {
  ClaimTriggerEvaluations::new(owner(owner_id), time(observed_at), time(claim_expires_at), 1).unwrap()
}

fn owner(value: &str) -> WorkerOwner {
  WorkerOwner::new(value).unwrap()
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}
