//! Installed reader contracts through current managed grants and the common process host.
use super::*;
use crate::runtime::{HostedIssueCodeReviewObservations, IssueCodeReviewReadProfile};
use octacity_server_work_source::WorkAdapterRegistry;
use octacity_work_source_protocol::CredentialClass;

async fn installed() -> (
  Fixture,
  IssueCodeReviewReadProfile,
  IntegrationId,
  IssueCodeReviewSelection,
) {
  installed_with_purpose(IntegrationPurpose::RepositoryRead).await
}
async fn installed_with_purpose(
  purpose: IntegrationPurpose,
) -> (
  Fixture,
  IssueCodeReviewReadProfile,
  IntegrationId,
  IssueCodeReviewSelection,
) {
  let fixture = Fixture::enabled("merge-request-forge").await;
  let mut binding = fixture.profile.read.binding.clone();
  binding.capability = exact("review-reader", 71);
  let (access, id) = managed_access(&fixture, &binding, purpose).await;
  let mut resource = fixture.profile.read.resource.clone();
  resource.credential.class = CredentialClass::RepositoryRead;
  resource.credential.handle = "independent-repository-reader".into();
  let dir = fixture.root.path().join("merge-request-forge");
  let manifest = dir.join("adapter.toml");
  let mut content = std::fs::read_to_string(&manifest).unwrap();
  content.push_str(&format!("\n[protocol.review]\nmin=1\nmax=1\n[capabilities.review]\nchecks=true\ncancel=true\ncredential_class=\"vcs_read\"\nconfiguration_digest={:?}\n", "e".repeat(64)));
  std::fs::write(manifest, content).unwrap();
  std::fs::write(dir.join("review-mode.json"),serde_json::to_vec(&serde_json::json!({"facts":{
    "title":"Merge request title","number":"MR-42","url":null,"author":null,"state":"open","draft":null,"head":"original-head","base":null,"changed_files":null,"additions":0,"deletions":null,"review":null,"checks":null}})).unwrap()).unwrap();
  let selection = IssueCodeReviewSelection {
    source: binding.clone(),
    item: WorkSourceItem {
      resource: ExternalWorkIdentity::new("native-container").unwrap(),
      item: ExternalWorkIdentity::new("native-mr-42").unwrap(),
    },
  };
  let profile = IssueCodeReviewReadProfile {
    managed_access: access,
    project_id: fixture.input.project_id(),
    binding,
    repository: None,
    publication_targets: vec![],
    plugin: fixture.profile.read.plugin.clone(),
    provider_configuration: fixture.profile.read.provider_configuration,
    resource,
    network_hosts: fixture.profile.read.network_hosts.clone(),
    context: fixture.profile.read.context.clone(),
    page_items: 2,
    max_page_bytes: 65536,
  };
  (fixture, profile, id, selection)
}
fn reader(
  fixture: &Fixture,
  profile: IssueCodeReviewReadProfile,
  cancellation: CancellationToken,
) -> HostedIssueCodeReviewObservations {
  HostedIssueCodeReviewObservations::new(
    Arc::new(WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
    vec![profile],
    fixture.clock.clone(),
    settings().operation_timeout,
    settings().cancellation_grace,
    cancellation,
  )
  .unwrap()
}

#[tokio::test]
async fn installed_review_reader_requires_current_independent_repository_grants() {
  let (fixture, profile, id, selection) = installed().await;
  let context = &fixture.profile.read.context;
  let project = fixture.input.project_id();
  let reader = reader(&fixture, profile, CancellationToken::new());
  reader.authorize(context, project, &selection).await.unwrap();
  let calls = fixture.root.path().join("merge-request-forge/review-requests.jsonl");
  assert!(!calls.exists(), "authorization and visibility do not spawn processes");
  let observed = reader.read(context, project, &selection).await.unwrap();
  assert_eq!(observed.selection, selection);
  assert_eq!(observed.title.unwrap().as_str(), "Merge request title");
  assert_eq!(observed.number.unwrap().as_str(), "MR-42");
  assert_eq!(observed.observed_at, time(100));
  assert_eq!(observed.head.unwrap().as_str(), "original-head");
  assert_eq!(observed.additions, Some(0));
  assert!(observed.changed_files.is_none() && observed.draft.is_none() && observed.checks.is_none());
  let before = std::fs::read(&calls).unwrap();
  fixture
    .store
    .publish_integration_project_access(octacity_server_store::testing::management_mutation(
      PublishIntegrationProjectAccess {
        intent: IntegrationProjectAccessIntent {
          connection_id: id,
          connection_version: IntegrationVersion::INITIAL,
          project_id: project,
          expected_current_version: Some(IntegrationVersion::INITIAL),
          resources: vec![],
        },
        idempotency_key: IdempotencyKey::new("revoke-review-read").unwrap(),
        published_at: time(110),
      },
    ))
    .await
    .unwrap();
  assert!(reader.read(context, project, &selection).await.is_err());
  assert_eq!(
    std::fs::read(&calls).unwrap(),
    before,
    "revocation precedes provider and credential IO"
  );
  assert!(
    reader
      .visibility(context, project)
      .await
      .unwrap()
      .allows(&selection.repository_identity()),
    "protected grant revocation preserves installed metadata visibility"
  );
  assert!(fixture.runs().await.is_empty());
  assert!(fixture.bytes.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn installed_check_reads_keep_original_head_and_return_one_page_with_retry_advice() {
  let (fixture, profile, _, selection) = installed().await;
  let directory = fixture.root.path().join("merge-request-forge");
  std::fs::write(directory.join("review-mode.json"),serde_json::to_vec(&serde_json::json!({"checks":[{
    "id":"pipeline-17","name":"Second vocabulary","producer":null,"state":"unknown","conclusion":null,"url":null}],"next_cursor":"next-page"})).unwrap()).unwrap();
  let reader = reader(&fixture, profile, CancellationToken::new());
  let request = crate::runtime::ReadIssueCodeReviewChecks {
    project_id: fixture.input.project_id(),
    selection,
    head: ImmutableRevision::new("original-head").unwrap(),
    read_id: FactoryDigest::from_bytes([81; 32]),
    continuation: None,
    limit: 50,
  };
  let page = reader
    .read_checks(&fixture.profile.read.context, request.clone())
    .await
    .unwrap();
  assert_eq!(page.head, "original-head");
  assert_eq!(page.items[0].name, "Second vocabulary");
  assert!(page.items[0].conclusion.is_none());
  assert_eq!(page.next_cursor.as_deref(), Some("next-page"));
  let calls = directory.join("review-requests.jsonl");
  let captured: serde_json::Value = serde_json::from_str(std::fs::read_to_string(&calls).unwrap().trim()).unwrap();
  assert_eq!(captured["message"]["command"]["payload"]["max_items"], 2);
  assert_eq!(captured["message"]["command"]["payload"]["read_id"], "51".repeat(32));
  std::fs::write(
    directory.join("review-mode.json"),
    br#"{"failure":{"class":"transient","retry_after_millis":2000}}"#,
  )
  .unwrap();
  assert!(matches!(
    reader.read_checks(&fixture.profile.read.context, request).await,
    Err(ApplicationError::SourceDependency {
      class: WorkSourceFailure::RateLimited,
      retry_after_millis: Some(2000)
    })
  ));
  assert_eq!(
    std::fs::read_to_string(&calls).unwrap().lines().count(),
    2,
    "each explicit read makes one bounded attempt; retry hints do not trigger hidden retries or paging"
  );
}

#[tokio::test]
async fn lineage_requires_an_explicit_local_repository_and_exact_publication_policy_mapping() {
  let (fixture, mut profile, _, selection) = installed().await;
  let repository = RepositoryId::generate();
  let target = exact("original-publication-policy", 82);
  let candidate = CandidateSubject::new(
    ExactSubject::new(
      fixture.input.project_id(),
      repository,
      ImmutableRevision::new("base").unwrap(),
    ),
    ImmutableRevision::new("head").unwrap(),
    FactoryDigest::from_bytes([83; 32]),
  );
  let context = &fixture.profile.read.context;
  let unavailable = reader(&fixture, profile.clone(), CancellationToken::new());
  assert!(
    unavailable
      .authorize_candidate(context, &selection, &candidate)
      .await
      .is_err()
  );
  profile.repository = Some(repository);
  profile.publication_targets = vec![target.clone()];
  let installed = reader(&fixture, profile, CancellationToken::new());
  installed
    .authorize_candidate(context, &selection, &candidate)
    .await
    .unwrap();
  installed
    .authorize_publication(context, fixture.input.project_id(), &selection, &target)
    .await
    .unwrap();
  assert!(
    installed
      .authorize_publication(
        context,
        fixture.input.project_id(),
        &selection,
        &exact("another-mirror", 84)
      )
      .await
      .is_err()
  );
  let wrong = CandidateSubject::new(
    ExactSubject::new(
      fixture.input.project_id(),
      RepositoryId::generate(),
      ImmutableRevision::new("base").unwrap(),
    ),
    ImmutableRevision::new("head").unwrap(),
    FactoryDigest::from_bytes([83; 32]),
  );
  assert!(
    installed
      .authorize_candidate(context, &selection, &wrong)
      .await
      .is_err()
  );
  assert!(
    !fixture
      .root
      .path()
      .join("merge-request-forge/review-requests.jsonl")
      .exists(),
    "mapping verification never spawns a provider or resolves credentials"
  );
}

#[tokio::test]
async fn ticket_or_delivery_grants_cannot_authorize_repository_observations() {
  for purpose in [
    IntegrationPurpose::TaskRead,
    IntegrationPurpose::TaskUpdate,
    IntegrationPurpose::Delivery,
  ] {
    let (fixture, profile, _, selection) = installed_with_purpose(purpose).await;
    let reader = reader(&fixture, profile, CancellationToken::new());
    assert!(
      reader
        .read(&fixture.profile.read.context, fixture.input.project_id(), &selection)
        .await
        .is_err()
    );
    assert!(
      !fixture
        .root
        .path()
        .join("merge-request-forge/review-requests.jsonl")
        .exists()
    );
  }
}

#[tokio::test]
async fn review_read_supervision_keeps_original_time_and_discards_revoked_or_cancelled_responses() {
  for outcome in ["observed", "revoked", "cancelled", "timeout"] {
    let (fixture, profile, id, selection) = installed().await;
    let mode = fixture.root.path().join("merge-request-forge/review-mode.json");
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&mode).unwrap()).unwrap();
    value["pause"] = serde_json::json!(true);
    std::fs::write(mode, serde_json::to_vec(&value).unwrap()).unwrap();
    let cancellation = CancellationToken::new();
    let reader = Arc::new(
      HostedIssueCodeReviewObservations::new(
        Arc::new(WorkAdapterRegistry::discover(fixture.root.path()).unwrap()),
        vec![profile],
        fixture.clock.clone(),
        if outcome == "timeout" {
          Duration::from_secs(1)
        } else {
          settings().operation_timeout
        },
        settings().cancellation_grace,
        cancellation.clone(),
      )
      .unwrap(),
    );
    let task = tokio::spawn({
      let context = fixture.profile.read.context.clone();
      let reader = reader.clone();
      let project = fixture.input.project_id();
      async move { reader.read(&context, project, &selection).await }
    });
    if outcome != "timeout" {
      fixture.wait_for_provider_file("review-reading").await;
      fixture.clock.0.store(200, Ordering::SeqCst);
    }
    if outcome == "revoked" {
      fixture
        .store
        .publish_integration_project_access(octacity_server_store::testing::management_mutation(
          PublishIntegrationProjectAccess {
            intent: IntegrationProjectAccessIntent {
              connection_id: id,
              connection_version: IntegrationVersion::INITIAL,
              project_id: fixture.input.project_id(),
              expected_current_version: Some(IntegrationVersion::INITIAL),
              resources: vec![],
            },
            idempotency_key: IdempotencyKey::new("revoke-in-flight-review").unwrap(),
            published_at: time(110),
          },
        ))
        .await
        .unwrap();
    }
    if outcome == "cancelled" {
      cancellation.cancel();
    } else if outcome != "timeout" {
      fixture.release_provider();
    }
    let result = tokio::time::timeout(TEST_WAIT, task).await.unwrap().unwrap();
    match outcome {
      "observed" => assert_eq!(result.unwrap().observed_at, time(100)),
      "cancelled" => assert!(matches!(
        result,
        Err(ApplicationError::SourceDependency {
          class: WorkSourceFailure::Cancelled,
          ..
        })
      )),
      "timeout" => assert!(matches!(
        result,
        Err(ApplicationError::SourceDependency {
          class: WorkSourceFailure::Timeout,
          ..
        })
      )),
      _ => assert!(result.is_err()),
    }
    assert!(fixture.runs().await.is_empty());
  }
}

#[tokio::test]
async fn installed_review_observation_retains_bounded_external_checks_for_its_original_head() {
  let (fixture, profile, _, selection) = installed().await;
  let directory = fixture.root.path().join("merge-request-forge");
  let path = directory.join("review-mode.json");
  let mut mode: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
  mode["checks"] = serde_json::json!([
    {"id":"ci-1","name":"contract","producer":"native-ci","state":"completed","conclusion":"skipped","url":"https://provider.example/private-thumbnail"},
    {"id":"ci-2","name":"audit","producer":null,"state":"unknown","conclusion":null,"url":null}
  ]);
  mode["next_cursor"] = serde_json::json!("opaque-next");
  std::fs::write(path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let reader = reader(&fixture, profile, CancellationToken::new());
  let observation = reader
    .read(&fixture.profile.read.context, fixture.input.project_id(), &selection)
    .await
    .unwrap();
  let checks = observation
    .check_details
    .as_ref()
    .expect("explicit review observation retains typed CI through the existing reader");
  assert_eq!(checks.head.as_str(), "original-head");
  assert_eq!(checks.observed_at, observation.observed_at);
  assert_eq!(checks.items[0].state, IssueCheckState::Skipped);
  assert_eq!(checks.items[1].state, IssueCheckState::Indeterminate);
  assert!(
    checks.has_more,
    "a partial provider page cannot establish complete checks or acceptance"
  );
  assert!(!serde_json::to_string(&checks).unwrap().contains("private-thumbnail"));
  let calls = std::fs::read_to_string(directory.join("review-requests.jsonl")).unwrap();
  assert_eq!(
    calls.lines().count(),
    2,
    "one metadata and one bounded original-head check observation"
  );
}
