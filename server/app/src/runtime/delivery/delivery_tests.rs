//! Trusted process bridge: ceiling validation precedes materialization and spawn.
use super::{
  FactoryDeliveryInputs, FactoryDeliveryMaterializer, FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft,
  FactoryDeliveryTargetPolicy, HostedFactoryCandidatePublisher,
};
use octacity_delivery_protocol::ReviewMetadata;
use octacity_server_application::{
  ApplicationError, FactoryCandidatePublicationRequest, FactoryCandidatePublisher, FactoryExecutionClock,
};
use octacity_server_delivery::DeliveryAdapterRegistry;
use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::*;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn key(s: &str) -> FactoryKey {
  FactoryKey::new(s).unwrap()
}
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(10))
  }
}
struct NoMaterial;
#[async_trait::async_trait]
impl FactoryDeliveryMaterializer for NoMaterial {
  async fn stage(
    &self,
    _: &FactoryCandidatePublicationRequest,
    _: &FactoryDeliveryInputs,
  ) -> Result<(), ApplicationError> {
    panic!("denied delivery must not expose protected input handles")
  }
}
fn fixture(plugin: ImmutableReference) -> (FactoryDeliveryPolicy, FactoryCandidatePublicationRequest) {
  fixture_for(
    plugin,
    ProjectId::generate(),
    RepositoryId::generate(),
    "refs/heads/main",
    FactoryDigest::from_bytes([5; 32]),
  )
}
fn fixture_for(
  plugin: ImmutableReference,
  project: ProjectId,
  repository: RepositoryId,
  base_reference: &str,
  configuration_digest: FactoryDigest,
) -> (FactoryDeliveryPolicy, FactoryCandidatePublicationRequest) {
  let policy = FactoryDeliveryPolicy::new(
    key("review-policy"),
    key("v1"),
    FactoryDeliveryPolicyDraft {
      plugin,
      configuration_digest,
      project_id: project,
      repository_id: repository,
      repository_locator: "repository:accepted-project".into(),
      credential_profile: key("delivery-writer"),
      credential_handle: "protected-delivery-grant".into(),
      network_hosts: vec![NetworkHost::new("forge.example").unwrap()],
      target: FactoryDeliveryTargetPolicy {
        base_reference: base_reference.into(),
        candidate_reference_prefix: "refs/heads/review/accepted-candidate-".into(),
      },
      review: ReviewMetadata {
        title: "Accepted candidate".into(),
        summary: "Verified".into(),
        draft: true,
      },
    },
  )
  .unwrap();
  let target = policy.reference().clone();
  let request = FactoryCandidatePublicationRequest {
    operation_id: FactoryDigest::from_bytes([1; 32]),
    work_id: WorkEnvelopeId::generate(),
    cycle_id: WorkflowCycleId::generate(),
    accepted_input_digest: FactoryDigest::from_bytes([6; 32]),
    candidate: CandidateSubject::new(
      ExactSubject::new(project, repository, ImmutableRevision::new("1".repeat(40)).unwrap()),
      ImmutableRevision::new("2".repeat(40)).unwrap(),
      FactoryDigest::from_bytes([9; 32]),
    ),
    bundle: FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([3; 32]), 100).unwrap(),
    manifest: FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([2; 32]), 100).unwrap(),
    target,
    review_metadata: None,
    require_review_url: false,
    permissions: FactoryPermissionSet::default(),
    budget: BudgetLimit::new(1, 1000, 1, 1, 1_048_576).unwrap(),
    deadline: at(1000),
    publish_until: None,
    started_at: at(1),
    authority_expires_at: at(1000),
  };
  (policy, request)
}
#[tokio::test]
async fn delivery_cannot_resolve_credentials_outside_the_frozen_ceiling() {
  let root = tempfile::tempdir().unwrap();
  let registry = DeliveryAdapterRegistry::discover(root.path()).unwrap();
  let (policy, request) = fixture(ImmutableReference::new(
    key("provider-plugin"),
    key("v1"),
    FactoryDigest::from_bytes([4; 32]),
  ));
  let publisher = HostedFactoryCandidatePublisher::new(
    Arc::new(registry),
    vec![policy],
    Arc::new(NoMaterial),
    Arc::new(Clock),
    Duration::from_secs(1),
    Duration::from_millis(50),
    CancellationToken::new(),
  )
  .unwrap();
  assert!(matches!(
    publisher.publish(&request).await,
    Err(ApplicationError::InvalidInput)
  ));
}

#[cfg(unix)]
struct Material(std::sync::atomic::AtomicUsize);
#[cfg(unix)]
#[async_trait::async_trait]
impl FactoryDeliveryMaterializer for Material {
  async fn stage(
    &self,
    request: &FactoryCandidatePublicationRequest,
    inputs: &FactoryDeliveryInputs,
  ) -> Result<(), ApplicationError> {
    assert_eq!(inputs.bundle.sha256, request.bundle.content_digest().to_string());
    assert_eq!(inputs.manifest.sha256, request.manifest.content_digest().to_string());
    assert_ne!(inputs.bundle.handle, inputs.manifest.handle);
    self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
  }
}
#[cfg(unix)]
#[tokio::test]
async fn exact_delivery_is_hosted_under_its_ceiling_and_retains_remote_facts() {
  hosted_publication(false, false, Some("refs/heads/production")).await;
}
#[cfg(unix)]
#[tokio::test]
async fn hosted_observation_routes_an_owned_review_requiring_repair_to_publication() {
  hosted_publication(true, false, None).await;
}
#[cfg(unix)]
#[tokio::test]
async fn one_installed_publication_policy_supports_distinct_operations_and_stable_replay() {
  hosted_publication(false, true, None).await;
}
#[cfg(unix)]
async fn hosted_publication(repair_required: bool, distinct_operations: bool, prepared_target: Option<&str>) {
  use sha2::{Digest as _, Sha256};
  use std::{fs, os::unix::fs::PermissionsExt as _};
  let root = tempfile::tempdir().unwrap();
  let directory = root.path().join("provider-plugin");
  fs::create_dir(&directory).unwrap();
  let (_, original) = fixture(ImmutableReference::new(
    key("provider-plugin"),
    key("v1"),
    FactoryDigest::from_bytes([4; 32]),
  ));
  let configuration_digest = if prepared_target.is_some() {
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["credentials", "inputs"] {
      let path = directory.join(name);
      fs::create_dir(&path).unwrap();
      fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let configuration = format!(
      r#"
config_version = 1
api_base = "https://forge.example/"
api_version = "2026-03-10"
git_path = "/bin/sh"
git_sha256 = {:?}
credential_directory = "credentials"
input_directory = "inputs"
[repositories."repository:accepted-project"]
repository_id = "{}"
owner = "team"
name = "actual-repository"
remote = "https://forge.example/team/actual-repository.git"
"#,
      hex::encode(Sha256::digest(fs::read("/bin/sh").unwrap())),
      original.candidate.exact().repository_id(),
    );
    let path = directory.join("github-plugin.toml");
    fs::write(&path, &configuration).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    hex::encode(Sha256::digest(configuration))
      .parse::<FactoryDigest>()
      .unwrap()
  } else {
    FactoryDigest::from_bytes([5; 32])
  };
  let base_reference = prepared_target.unwrap_or("refs/heads/main");
  let metadata = serde_json::json!({"title":"Prepared PR","summary":"Verified candidate"});
  let review = serde_json::to_string(&ReviewMetadata {
    title: "Prepared PR".into(),
    summary: "Verified candidate".into(),
    draft: true,
  })
  .unwrap();
  let check_publish = if prepared_target.is_some() {
    format!(
      "case \"$frame\" in\n*'\"base_reference\":\"{base_reference}\"'*'\"review\":{review}'*) ;;\n*) exit 93 ;;\nesac\n"
    )
  } else {
    String::new()
  };
  let response = serde_json::json!({"protocol_version":1,"request_id":"01".repeat(32),"outcome":{"status":if repair_required {"repair_required"} else {"observed"},"payload":{
    "delivery_id":"01".repeat(32),"repository_id":original.candidate.exact().repository_id(),
    "base_revision":"1".repeat(40),"candidate_revision":"2".repeat(40),"changeset_digest":"02".repeat(32),
    "decision_digest":"06".repeat(32),"configuration_digest":configuration_digest,
    "target":{"base_reference":base_reference,"candidate_reference":format!("refs/heads/review/accepted-candidate-{}","01".repeat(32))},
    "branch_reference":format!("refs/heads/review/accepted-candidate-{}","01".repeat(32)),"remote_head":"2".repeat(40),
    "review":{"identity":"23","state":"open","head":"2".repeat(40),"base":"1".repeat(40)}}}});
  let mut second_response = response.clone();
  if distinct_operations {
    second_response["request_id"] = serde_json::json!("08".repeat(32));
    second_response["outcome"]["payload"]["delivery_id"] = serde_json::json!("08".repeat(32));
    second_response["outcome"]["payload"]["target"]["candidate_reference"] =
      serde_json::json!(format!("refs/heads/review/accepted-candidate-{}", "08".repeat(32)));
    second_response["outcome"]["payload"]["branch_reference"] =
      second_response["outcome"]["payload"]["target"]["candidate_reference"].clone();
    second_response["outcome"]["payload"]["candidate_revision"] = serde_json::json!("4".repeat(40));
    second_response["outcome"]["payload"]["remote_head"] = serde_json::json!("4".repeat(40));
    second_response["outcome"]["payload"]["review"]["head"] = serde_json::json!("4".repeat(40));
    second_response["outcome"]["payload"]["review"]["identity"] = serde_json::json!("24");
  }
  let ack = serde_json::json!({"protocol_version":1,"request_id":"01".repeat(32),"outcome":{"status":"acknowledged","payload":{"delivery_id":"01".repeat(32)}}});
  let denied = serde_json::json!({"protocol_version":1,"request_id":"01".repeat(32),
    "outcome":{"status":"failure","payload":{"class":"permanent","retry_after_millis":null}}});
  let script = format!(
    "#!/bin/sh\nIFS= read -r frame || exit 91\nif [ \"${{HOME+x}}\" = x ]; then exit 92; fi\ncase \"$frame\" in *'\"operation\":\"publish\"'*) {check_publish} : ;; esac\ncase \"$frame\" in\n*'\"operation\":\"cancel\"'*) printf '%s\\n' '{ack}';;\n*'\"title\":\"Blocked by provider\"'*) printf '%s\\n' '{denied}';;\n*'0808080808080808080808080808080808080808080808080808080808080808'*) printf '%s\\n' '{second_response}';;\n*) printf '%s\\n' '{response}';;\nesac\n"
  );
  let sha = hex::encode(Sha256::digest(script.as_bytes()));
  fs::write(directory.join("plugin"), &script).unwrap();
  fs::set_permissions(directory.join("plugin"), fs::Permissions::from_mode(0o700)).unwrap();
  fs::write(directory.join("adapter.toml"),format!("manifest_version = 1\nadapter_id = \"provider-plugin\"\nexecutable = \"plugin\"\nexecutable_sha256 = {sha:?}\n[protocol]\nmin = 1\nmax = 1\n[capabilities]\nobserve = true\npublish = true\ncancel = true\nidempotency = \"observe_before_retry\"\ncredential_class = \"delivery_write\"\nconfiguration_digest = {:?}\n",configuration_digest.to_string())).unwrap();
  let plugin = ImmutableReference::new(key("provider-plugin"), key("v1"), sha.parse().unwrap());
  let (mut policy, mut request) = fixture_for(
    plugin.clone(),
    original.candidate.exact().project_id(),
    original.candidate.exact().repository_id(),
    base_reference,
    configuration_digest,
  );
  request.require_review_url = prepared_target.is_some();
  request.review_metadata = prepared_target.map(|_| serde_json::from_value(metadata).unwrap());
  request.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin],
    secret_profiles: vec![key("delivery-writer")],
    network_hosts: vec![NetworkHost::new("forge.example").unwrap()],
    ..Default::default()
  })
  .unwrap();
  let material = Arc::new(Material(std::sync::atomic::AtomicUsize::new(0)));
  let registry = DeliveryAdapterRegistry::discover(root.path()).unwrap();
  if prepared_target.is_some() {
    super::factory_delivery_file::install_github_review_links(
      &mut policy,
      registry.resolve("provider-plugin", &sha).unwrap(),
    )
    .unwrap();
  }
  let publisher = HostedFactoryCandidatePublisher::new(
    Arc::new(registry),
    vec![policy],
    material.clone(),
    Arc::new(Clock),
    Duration::from_secs(1),
    Duration::from_millis(50),
    CancellationToken::new(),
  )
  .unwrap();
  if prepared_target.is_some() {
    // The stub adapter returns a receipt without reading GitHub files. Resolving
    // its link in the host must use the provider settings pinned at installation.
    fs::remove_file(directory.join("github-plugin.toml")).unwrap();
  }
  if repair_required {
    assert!(
      publisher
        .observe(&request)
        .await
        .expect("repair observation must remain recoverable")
        .is_none()
    );
    assert_eq!(material.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    return;
  }
  assert_ne!(request.candidate.changeset_digest(), request.manifest.content_digest());
  let publication = publisher
    .publish(&request)
    .await
    .expect("the exact authorized candidate must be hosted");
  assert_eq!(publication.operation_id, request.operation_id);
  assert_eq!(publication.candidate, request.candidate);
  let facts = serde_json::to_value(publication.facts).unwrap();
  assert_eq!(facts["review_identity"], "23");
  if prepared_target.is_some() {
    assert_eq!(
      facts["review_url"],
      "https://forge.example/team/actual-repository/pull/23"
    );
    let mut expired = request.clone();
    expired.publish_until = Some(at(10));
    assert!(matches!(
      publisher.publish(&expired).await,
      Err(ApplicationError::PublicationRejected(
        octacity_server_application::FactoryPublicationRejection::ValidationExpired
      ))
    ));
    let observed = publisher.observe(&expired).await.unwrap().unwrap();
    assert_eq!(observed.receipt, publication.receipt);

    use octacity_server_application::FactoryPublicationRejection;
    for (title, reason) in [
      ("Blocked by provider", FactoryPublicationRejection::ProviderRejected),
      ("   ", FactoryPublicationRejection::InvalidReviewMetadata),
    ] {
      let mut rejected = request.clone();
      rejected.review_metadata = Some(serde_json::from_value(serde_json::json!({"title":title,"summary":""})).unwrap());
      assert!(matches!(
        publisher.observe(&rejected).await,
        Err(ApplicationError::PublicationRejected(actual)) if actual == reason
      ));
    }

    // Granting the Flow action does not also grant its selected provider.
    let mut missing_provider = request.clone();
    missing_provider.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![ImmutableReference::new(
        key("candidate.publisher"),
        key("v1"),
        FactoryDigest::from_bytes([4; 32]),
      )],
      secret_profiles: vec![key("delivery-writer")],
      network_hosts: vec![NetworkHost::new("forge.example").unwrap()],
      ..Default::default()
    })
    .unwrap();
    assert!(matches!(
      publisher.publish(&missing_provider).await,
      Err(ApplicationError::InvalidInput)
    ));
  }
  assert_eq!(facts["remote_head"], "2".repeat(40));
  assert_eq!(material.0.load(std::sync::atomic::Ordering::SeqCst), 1);
  if distinct_operations {
    let mut second = request.clone();
    second.operation_id = FactoryDigest::from_bytes([8; 32]);
    second.work_id = WorkEnvelopeId::generate();
    second.candidate = CandidateSubject::new(
      second.candidate.exact().clone(),
      octacity_server_domain::ImmutableRevision::new("4".repeat(40)).unwrap(),
      second.candidate.changeset_digest(),
    );
    let published = publisher
      .publish(&second)
      .await
      .expect("a second operation must use its own reference under the same policy");
    assert_ne!(
      serde_json::to_value(&published.facts).unwrap()["branch_reference"],
      facts["branch_reference"]
    );
    let replay = publisher.observe(&request).await.unwrap().unwrap();
    assert_eq!(
      serde_json::to_value(replay.facts).unwrap()["branch_reference"],
      facts["branch_reference"]
    );
    return;
  }
  let stopped = publisher
    .cancel(&request, at(10))
    .await
    .expect("stopping the original identity cannot replay publication")
    .unwrap();
  assert_eq!(stopped.elapsed_millis, 9);
  assert_eq!(material.0.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_delivery_stops_the_original_host_before_acknowledging_completion() {
  use sha2::{Digest as _, Sha256};
  use std::{fs, os::unix::fs::PermissionsExt as _};
  let root = tempfile::tempdir().unwrap();
  let directory = root.path().join("provider-plugin");
  fs::create_dir(&directory).unwrap();
  let ack = serde_json::json!({"protocol_version":1,"request_id":"01".repeat(32),"outcome":{"status":"acknowledged","payload":{"delivery_id":"01".repeat(32)}}});
  let cancelled = serde_json::json!({"protocol_version":1,"request_id":"01".repeat(32),"outcome":{"status":"failure","payload":{"class":"cancelled","retry_after_millis":null}}});
  let script = format!(
    "#!/bin/sh\nIFS= read -r frame || exit 91\ncase \"$frame\" in\n*'\"operation\":\"cancel\"'*) printf '%s\\n' '{ack}';;\n*) : > active\nIFS= read -r cancel || exit 92\nprintf '%s\\n' '{cancelled}';;\nesac\n"
  );
  let sha = hex::encode(Sha256::digest(script.as_bytes()));
  fs::write(directory.join("plugin"), &script).unwrap();
  fs::set_permissions(directory.join("plugin"), fs::Permissions::from_mode(0o700)).unwrap();
  fs::write(directory.join("adapter.toml"),format!("manifest_version = 1\nadapter_id = \"provider-plugin\"\nexecutable = \"plugin\"\nexecutable_sha256 = {sha:?}\n[protocol]\nmin = 1\nmax = 1\n[capabilities]\nobserve = true\npublish = true\ncancel = true\nidempotency = \"observe_before_retry\"\ncredential_class = \"delivery_write\"\nconfiguration_digest = {:?}\n","05".repeat(32))).unwrap();
  let plugin = ImmutableReference::new(key("provider-plugin"), key("v1"), sha.parse().unwrap());
  let (policy, mut request) = fixture(plugin.clone());
  request.permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![plugin],
    secret_profiles: vec![key("delivery-writer")],
    network_hosts: vec![NetworkHost::new("forge.example").unwrap()],
    ..Default::default()
  })
  .unwrap();
  let publisher = Arc::new(
    HostedFactoryCandidatePublisher::new(
      Arc::new(DeliveryAdapterRegistry::discover(root.path()).unwrap()),
      vec![policy],
      Arc::new(Material(std::sync::atomic::AtomicUsize::new(0))),
      Arc::new(Clock),
      Duration::from_secs(1),
      Duration::from_millis(50),
      CancellationToken::new(),
    )
    .unwrap(),
  );
  let owned = publisher.clone();
  let intent = request.clone();
  let publication = tokio::spawn(async move { owned.publish(&intent).await });
  tokio::time::timeout(Duration::from_secs(1), async {
    while !directory.join("active").exists() {
      tokio::task::yield_now().await;
    }
  })
  .await
  .unwrap();
  assert!(
    publisher.cancel(&request, at(10)).await.unwrap().is_none(),
    "a second process cannot acknowledge a still-active original host"
  );
  assert!(
    tokio::time::timeout(Duration::from_millis(300), publication)
      .await
      .expect("cancellation must stop the original host promptly")
      .unwrap()
      .is_err()
  );
  assert!(publisher.cancel(&request, at(10)).await.unwrap().is_some());
}
