use std::collections::{BTreeMap, BTreeSet};

use octacity_server_domain::{BuildId, Timestamp};
use serde_json::json;

use crate::testing::{
  InMemoryStore, authoritative_store_contract_fixture, compatible_snapshot, verify_in_memory_agent_credential_contract,
  verify_in_memory_log_search_index_contract, verify_in_memory_store_contract,
};
use crate::{
  AppendJobEvents, DurableJobEvent, EventSequence, IdempotencyKey, JobClaim, JobClaimOutcome, JobEventKind,
  JobExecutionStore, LeaseAccess, LeaseFence, LeaseWindow, ListProjects, MAX_IDEMPOTENCY_KEY_BYTES,
  MAX_JOB_EVENT_BATCH_SIZE, MAX_JOB_EVENT_PAYLOAD_BYTES, MAX_MATERIALIZED_JOBS, MAX_PROJECT_PAGE_SIZE,
  MAX_WEBHOOK_HEADER_NAME_BYTES, MutationDisposition, RegistrationEpoch, StoreError, StoreInputError, StoreOperation,
  TriggerAcceptanceStore, TriggerEventKind, UnmanagedWebhookDefinition, WEBHOOK_ADAPTER_SHA256_BYTES,
  canonical_webhook_headers,
};

#[test]
fn in_memory_adapter_satisfies_the_authoritative_store_contract() {
  verify_in_memory_store_contract();
}

#[test]
fn materialized_job_count_is_bounded_before_template_validation() {
  let fixture = authoritative_store_contract_fixture();
  let jobs = vec![fixture.request.jobs[0].clone(); MAX_MATERIALIZED_JOBS + 1];
  let unrelated_build = crate::test_support::id::<BuildId>(999);

  assert_eq!(
    crate::cicd::model::validate_materialized_jobs(StoreOperation::AcceptTrigger, unrelated_build, &jobs),
    Err(StoreError::invalid(
      StoreOperation::AcceptTrigger,
      StoreInputError::TooManyJobs,
    )),
  );
}

#[test]
fn ordinary_build_acceptance_rejects_job_source_mode_and_revision_substitution() {
  let free = crate::testing::source_free_store_contract_fixture().request;
  let repository = authoritative_store_contract_fixture().request;
  free.validate().unwrap();
  repository.validate().unwrap();
  let mut free_with_checkout = free.clone();
  free_with_checkout.jobs = repository.jobs.clone();
  let mut repository_without_checkout = repository.clone();
  repository_without_checkout.jobs = free.jobs;
  let mut changed_revision = repository;
  changed_revision.build.immutable_revision =
    Some(octacity_server_domain::ImmutableRevision::new("another-revision").unwrap());
  for request in [free_with_checkout, repository_without_checkout, changed_revision] {
    assert!(
      request.validate().is_err(),
      "Job source authority must match the retained Build intent"
    );
  }
}

#[test]
fn immutable_build_input_retains_explicit_source_absence_and_rejects_mixed_vcs_facts() {
  let mut build = authoritative_store_contract_fixture().request.build;
  let historical = serde_json::to_value(&build).unwrap();
  assert!(historical.get("source").is_none());
  let mut conflicting = build.clone();
  conflicting.input_snapshot = json!({"parameters": {}, "source": {"kind": "source_free_v1"}});
  assert!(
    conflicting.validate().is_err(),
    "repository facts must agree with retained source intent"
  );
  build.repository_id = None;
  build.repository_version = None;
  build.immutable_revision = None;
  build.source = Some(octacity_server_domain::BuildSourceMode::SourceFreeV1);
  build.input_snapshot = json!({"parameters": {}, "source": {"kind": "source_free_v1"}});
  build.validate().unwrap();
  let retained = serde_json::to_value(&build).unwrap();
  assert_eq!(retained["source"], json!({"kind": "source_free_v1"}));
  for field in ["repository_id", "repository_version", "immutable_revision"] {
    assert!(retained.get(field).is_none(), "source-free input must omit {field}");
  }
  build.repository_id = Some(crate::test_support::id(34));
  assert!(
    build.validate().is_err(),
    "source-free mode must reject a repository identity"
  );
  build.repository_id = None;
  build.source = None;
  assert!(
    build.validate().is_err(),
    "source absence must be explicit and versioned"
  );
}

#[test]
fn accepted_build_requires_a_positive_typed_job_concurrency_limit() {
  let mut request = authoritative_store_contract_fixture().request;
  request.build.project_job_concurrency_limit = 0;

  assert_eq!(
    request.validate(),
    Err(StoreError::invalid(
      StoreOperation::AcceptTrigger,
      StoreInputError::InvalidBuildSchedulingPolicy,
    )),
  );
}

#[test]
fn failed_ready_signing_rolls_back_the_complete_in_memory_acceptance() {
  let fixture = authoritative_store_contract_fixture();
  let store = InMemoryStore::new();
  store.seed_authoritative_contract_prerequisites(&fixture).unwrap();
  let mut invalid = fixture.request.clone();
  invalid.accepted_at = Timestamp::from_unix_millis(-1).unwrap();
  crate::test_support::run_ready(
    async {
      assert_eq!(
        store.accept_trigger(invalid).await.unwrap_err(),
        StoreError::Unavailable
      );
      assert_eq!(
        store.accept_trigger(fixture.request).await.unwrap().disposition,
        MutationDisposition::Applied
      );
    },
    "the in-memory transaction unexpectedly yielded",
  );
}

#[test]
fn incompatible_ready_work_does_not_block_a_later_compatible_job() {
  let mut fixture = authoritative_store_contract_fixture();
  let blocked_job = fixture.request.jobs[0].id;
  fixture.request.jobs[0]
    .requirements
    .labels
    .insert("factory-enforcement".to_owned(), "missing".to_owned());
  fixture.request.jobs[1].dependencies.clear();
  let expected_job = fixture.request.jobs[1].id;
  let store = InMemoryStore::new();
  store.seed_authoritative_contract_prerequisites(&fixture).unwrap();

  crate::test_support::run_ready(
    async {
      store.accept_trigger(fixture.request).await.unwrap();
      let outcome = store
        .claim_ready_job(
          JobClaim::new(
            crate::test_support::id(990),
            LeaseFence::from_bytes([9; 32]),
            fixture.agent_id,
            fixture.registration_epoch,
            fixture.allowed_pool,
            compatible_snapshot(),
            LeaseWindow::new(crate::test_support::time(1_000), crate::test_support::time(2_000)).unwrap(),
          )
          .unwrap(),
        )
        .await
        .unwrap();
      let JobClaimOutcome::Claimed(grant) = outcome else {
        panic!("the compatible Job behind an incompatible ready Job must remain schedulable");
      };
      assert_eq!(grant.job_id, expected_job);
      assert_ne!(grant.job_id, blocked_job);
    },
    "the in-memory placement check unexpectedly yielded",
  );
}

#[test]
fn in_memory_adapter_satisfies_the_agent_credential_contract() {
  verify_in_memory_agent_credential_contract();
}

#[test]
fn in_memory_adapter_satisfies_the_log_search_index_contract() {
  verify_in_memory_log_search_index_contract();
}

#[test]
fn in_memory_adapter_satisfies_the_project_store_contract() {
  crate::testing::verify_in_memory_project_store_contract();
}

#[test]
fn in_memory_adapter_satisfies_the_pipeline_store_contract() {
  crate::testing::verify_in_memory_pipeline_store_contract();
}

#[test]
fn in_memory_adapter_satisfies_the_configuration_store_contract() {
  crate::testing::verify_in_memory_configuration_store_contract();
}

#[test]
fn in_memory_adapters_satisfy_the_definition_discovery_contract() {
  crate::testing::verify_in_memory_definition_discovery_contract();
}

#[test]
fn in_memory_adapter_satisfies_the_build_discovery_contract() {
  crate::testing::verify_in_memory_build_discovery_contract();
}

#[test]
fn in_memory_adapter_satisfies_the_agent_pool_store_contract() {
  crate::testing::verify_in_memory_agent_pool_store_contract();
}

#[test]
fn agent_credentials_are_redacted_by_all_formatters() {
  let marker = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  let secret = crate::CredentialSecret::from_bytes(*b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
  let debug = format!("{secret:?}");
  let display = format!("{secret}");
  assert!(!debug.contains(marker));
  assert!(!display.contains(marker));
  assert_eq!(debug, "CredentialSecret([REDACTED])");
  assert_eq!(display, "[REDACTED]");
}

#[test]
fn durable_events_compute_canonical_digests_and_enforce_bounds() {
  let time = Timestamp::from_unix_millis(1).unwrap();
  let kind = JobEventKind::new("progress").unwrap();
  let left = DurableJobEvent::new(
    EventSequence::new(1).unwrap(),
    kind.clone(),
    time,
    json!({"b": 2, "a": 1}),
  )
  .unwrap();
  let right = DurableJobEvent::new(
    EventSequence::new(1).unwrap(),
    kind.clone(),
    time,
    json!({"a": 1, "b": 2}),
  )
  .unwrap();
  assert_eq!(left.digest(), right.digest());

  assert_eq!(
    DurableJobEvent::new(
      EventSequence::new(2).unwrap(),
      kind,
      time,
      json!({"text": "x".repeat(MAX_JOB_EVENT_PAYLOAD_BYTES)}),
    )
    .unwrap_err(),
    StoreInputError::EventPayloadTooLarge
  );
}

#[test]
fn event_batches_are_bounded_even_when_callers_mutate_public_collections() {
  let mut events = Vec::with_capacity(MAX_JOB_EVENT_BATCH_SIZE + 1);
  for sequence in 1..=MAX_JOB_EVENT_BATCH_SIZE + 1 {
    events.push(
      DurableJobEvent::new(
        EventSequence::new(u64::try_from(sequence).unwrap()).unwrap(),
        JobEventKind::new("progress").unwrap(),
        Timestamp::from_unix_millis(1).unwrap(),
        json!({}),
      )
      .unwrap(),
    );
  }
  let lease = LeaseAccess {
    lease_id: "00000000-0000-0000-0000-000000000001".parse().unwrap(),
    fence: LeaseFence::from_bytes([1; 32]),
    agent_id: "00000000-0000-0000-0000-000000000002".parse().unwrap(),
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  };
  assert_eq!(
    AppendJobEvents::new(lease, events, Timestamp::from_unix_millis(2).unwrap()).unwrap_err(),
    StoreError::InvalidInput {
      operation: StoreOperation::AppendJobEvents,
      source: StoreInputError::EventBatchTooLarge,
    }
  );
}

#[test]
fn project_query_and_idempotency_inputs_are_bounded() {
  assert_eq!(IdempotencyKey::new(""), Err(StoreInputError::InvalidIdempotencyKey));
  assert_eq!(
    IdempotencyKey::new("x".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1)),
    Err(StoreInputError::InvalidIdempotencyKey)
  );
  for invalid_limit in [0, MAX_PROJECT_PAGE_SIZE + 1] {
    assert_eq!(
      ListProjects::new(None, None, invalid_limit, crate::ProjectListVisibility::all()),
      Err(StoreError::InvalidInput {
        operation: StoreOperation::ListProjects,
        source: StoreInputError::InvalidProjectPageSize,
      })
    );
  }
}

#[test]
fn webhook_header_validation_is_shared_by_construction_and_store_revalidation() {
  for invalid in [
    vec!["x-signature".to_owned(), "x-signature".to_owned()],
    vec!["X-Signature".to_owned()],
    vec!["x".repeat(MAX_WEBHOOK_HEADER_NAME_BYTES + 1)],
  ] {
    assert_eq!(
      canonical_webhook_headers(invalid),
      Err(StoreInputError::InvalidWebhookDefinition)
    );
  }

  let definition = UnmanagedWebhookDefinition {
    adapter_id: "fixture".to_owned(),
    adapter_sha256: "0".repeat(WEBHOOK_ADAPTER_SHA256_BYTES),
    verification_material_handle: "secret:webhook".to_owned(),
    verification_headers: BTreeSet::from(["x".repeat(MAX_WEBHOOK_HEADER_NAME_BYTES + 1)]),
    repository_id: crate::test_support::id(101),
    event_kind: TriggerEventKind::new("push").unwrap(),
    parameters: BTreeMap::new(),
    priority: 0,
  };
  assert_eq!(definition.validate(), Err(StoreInputError::InvalidWebhookDefinition));
}
