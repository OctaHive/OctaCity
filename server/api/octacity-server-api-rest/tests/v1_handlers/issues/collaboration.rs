//! Shared HTTP fixture with real Issue, discussion, document and material owners.
use super::*;
use octacity_server_application::testing::{InMemoryArtifactBytes, InMemoryFactoryConfigurationStore};
use octacity_server_application::*;
use std::sync::atomic::AtomicI64;

pub(super) struct Clock(pub(super) AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(self.0.load(Ordering::SeqCst)).unwrap())
  }
}

pub(super) struct Fixture {
  pub(super) issues: Arc<InMemoryProjectStore>,
  pub(super) store: Arc<InMemoryFactoryConfigurationStore>,
  bytes: Arc<InMemoryArtifactBytes>,
  pub(super) clock: Arc<Clock>,
}
impl Fixture {
  pub(super) async fn new() -> Self {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(PROJECT.parse().unwrap()).unwrap();
    Self {
      issues: manual_issue_store(PROJECT.parse().unwrap()).await,
      store,
      bytes: Arc::new(InMemoryArtifactBytes::default()),
      clock: Arc::new(Clock(AtomicI64::new(1000))),
    }
  }
  pub(super) fn set_time(&self, now: i64) {
    self.clock.0.store(now, Ordering::SeqCst);
  }
  pub(super) fn router(&self, policy: Arc<dyn ManagementAuthorizationPolicy>) -> axum::Router {
    let service = Arc::new(IssueHandlers::new(self.issues.clone(), policy.clone()));
    let documents = Arc::new(
      ProjectTextIntake::new(
        self.store.clone(),
        self.bytes.clone(),
        Arc::new(ExactRevisionResolver),
        policy.clone(),
        self.clock.clone(),
      )
      .with_issue_documents(self.issues.clone()),
    );
    let discussion = Arc::new(IssueDiscussionHandlers::new(
      self.issues.clone(),
      policy.clone(),
      self.clock.clone(),
    ));
    let materials = Arc::new(IssueMaterialHandlers::new(
      self.issues.clone(),
      self.store.clone(),
      policy.clone(),
      self.clock.clone(),
    ));
    let peer = Arc::new(RecordingApplication::default());
    management_router_with_application(
      || true,
      recording_management_application(peer.clone(), peer)
        .with_issues(
          IssueManagementApplication::new(
            Arc::new(AuthorizedCommandHandler::new(policy.clone(), service.clone())),
            Arc::new(AuthorizedQueryHandler::new(policy.clone(), service)),
          )
          .with_materials(Arc::new(AuthorizedCommandHandler::new(policy.clone(), materials))),
        )
        .with_issue_documents(
          Arc::new(AuthorizedCommandHandler::new(policy.clone(), documents.clone())),
          Arc::new(AuthorizedQueryHandler::new(policy.clone(), documents.clone())),
        )
        .with_issue_discussion(octacity_server_api_rest::v1::IssueDiscussionManagementApplication::new(
          Arc::new(AuthorizedCommandHandler::new(policy.clone(), discussion.clone())),
          Arc::new(AuthorizedQueryHandler::new(policy.clone(), discussion)),
        ))
        .with_project_documents(
          Arc::new(AuthorizedCommandHandler::new(policy.clone(), documents.clone())),
          Arc::new(AuthorizedQueryHandler::new(policy, documents)),
        ),
    )
  }
}
pub(super) async fn create(router: &axum::Router, key: &str) -> String {
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), key, None, &draft().to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  document(response).await["resource"]["id"].as_str().unwrap().to_owned()
}

pub(super) fn verified(mut request: Request<Body>, actor: &str, scope: &str) -> Request<Body> {
  request.extensions_mut().insert(
    octacity_server_api_rest::integrations::ProtectedIntegrationRequest::new(
      ManagementRequestContext::new(
        ManagementActor::authenticated(actor).unwrap(),
        ManagementSecurityScope::new(scope).unwrap(),
        ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap(),
        ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
      )
      .unwrap(),
    )
    .unwrap(),
  );
  request
}

pub(super) struct Rights(pub(super) Arc<Mutex<BTreeSet<(ManagementResourceKind, ManagementAction)>>>);
#[async_trait]
impl ManagementAuthorizationPolicy for Rights {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if self.0.lock().unwrap().contains(&(resource.kind(), action)) {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
