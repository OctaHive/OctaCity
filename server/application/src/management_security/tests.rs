use std::{
  future::Future,
  str::FromStr,
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use uuid::Uuid;

use super::authorization::resource_identity;
use super::*;

fn request_id() -> ManagementRequestId {
  ManagementRequestId::new(Uuid::from_u128(0xabcdef01_2345_6789_abcd_ef0123456789)).expect("non-nil fixture")
}

fn resource(index: usize) -> ManagementResource {
  ManagementResource::instance(
    ManagementResourceKind::Project,
    ManagementResourceIdentity::new(format!("project-{index}")).expect("bounded fixture"),
  )
  .expect("Project instances are supported")
}

fn representative_resource(kind: ManagementResourceKind) -> ManagementResource {
  let identity = || ManagementResourceIdentity::new("fixture-id").unwrap();
  match kind {
    ManagementResourceKind::ControlPlane
    | ManagementResourceKind::Project
    | ManagementResourceKind::Trigger
    | ManagementResourceKind::AgentPool
    | ManagementResourceKind::Agent
    | ManagementResourceKind::AuditFact => ManagementResource::collection(kind).unwrap(),
    ManagementResourceKind::ProjectPolicy => {
      ManagementResource::owned_collection(kind, ManagementResourceKind::Project, identity()).unwrap()
    }
    ManagementResourceKind::JobEvent => {
      ManagementResource::owned_collection(kind, ManagementResourceKind::Job, identity()).unwrap()
    }
    ManagementResourceKind::BuildLog => {
      ManagementResource::owned_collection(kind, ManagementResourceKind::Project, identity()).unwrap()
    }
    ManagementResourceKind::AgentEnrollment => {
      ManagementResource::owned_collection(kind, ManagementResourceKind::AgentPool, identity()).unwrap()
    }
    ManagementResourceKind::Pipeline
    | ManagementResourceKind::Repository
    | ManagementResourceKind::BuildConfiguration
    | ManagementResourceKind::Schedule
    | ManagementResourceKind::Build
    | ManagementResourceKind::Attempt
    | ManagementResourceKind::Job
    | ManagementResourceKind::Artifact
    | ManagementResourceKind::CacheSession
    | ManagementResourceKind::WebhookIntegration
    | ManagementResourceKind::Retention => ManagementResource::instance(kind, identity()).unwrap(),
  }
}

#[test]
fn actor_construction_rejects_ambiguous_identity_shapes() {
  assert_eq!(
    ManagementActor::new(
      ManagementActorKind::UnauthenticatedManagement,
      Some("invented-subject".to_owned()),
    ),
    Err(ManagementSecurityError::UnexpectedActorIdentity)
  );
  assert_eq!(
    ManagementActor::new(ManagementActorKind::AuthenticatedManagement, None),
    Err(ManagementSecurityError::MissingActorIdentity)
  );
  assert_eq!(
    ManagementActor::authenticated(" subject "),
    Err(ManagementSecurityError::InvalidActorIdentity)
  );
}

#[test]
fn bounded_identity_constructors_reject_oversized_values() {
  assert_eq!(
    ManagementActor::authenticated("a".repeat(MAX_MANAGEMENT_ACTOR_IDENTITY_BYTES + 1)),
    Err(ManagementSecurityError::ActorIdentityTooLong)
  );
  assert_eq!(
    ManagementSecurityScope::new("s".repeat(MAX_MANAGEMENT_SECURITY_SCOPE_BYTES + 1)),
    Err(ManagementSecurityError::InvalidSecurityScope)
  );
  assert_eq!(
    ManagementResourceIdentity::new("r".repeat(MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES + 1)),
    Err(ManagementSecurityError::InvalidResourceIdentity)
  );
}

#[test]
fn resource_construction_rejects_missing_identities_and_every_unsupported_shape() {
  assert_eq!(
    ManagementResourceIdentity::new(""),
    Err(ManagementSecurityError::InvalidResourceIdentity)
  );
  assert_eq!(
    ManagementResource::collection(ManagementResourceKind::Artifact),
    Err(ManagementSecurityError::UnsupportedResourceShape)
  );
  assert_eq!(
    ManagementResource::instance(
      ManagementResourceKind::ControlPlane,
      ManagementResourceIdentity::new("control-plane-1").unwrap(),
    ),
    Err(ManagementSecurityError::UnsupportedResourceShape)
  );
  assert_eq!(
    ManagementResource::owned_collection(
      ManagementResourceKind::Artifact,
      ManagementResourceKind::AgentPool,
      ManagementResourceIdentity::new("pool-1").unwrap(),
    ),
    Err(ManagementSecurityError::UnsupportedResourceShape)
  );

  let artifacts = ManagementResource::owned_collection(
    ManagementResourceKind::Artifact,
    ManagementResourceKind::Build,
    ManagementResourceIdentity::new("build-1").unwrap(),
  )
  .unwrap();
  assert_eq!(artifacts.kind(), ManagementResourceKind::Artifact);
  assert_eq!(
    artifacts.owner().map(|(kind, identity)| (kind, identity.as_str())),
    Some((ManagementResourceKind::Build, "build-1"))
  );
}

#[test]
fn security_scopes_and_request_ids_require_canonical_values() {
  assert!(ManagementSecurityScope::new("tenant:operator-1").is_ok());
  for value in ["", "Uppercase", " leading", "trailing ", "slash/value", "line\nbreak"] {
    assert_eq!(
      ManagementSecurityScope::new(value),
      Err(ManagementSecurityError::InvalidSecurityScope)
    );
  }

  let canonical = Uuid::from_u128(0xabcdef01_2345_6789_abcd_ef0123456789)
    .hyphenated()
    .to_string();
  assert_eq!(ManagementRequestId::from_str(&canonical).unwrap(), request_id());
  assert_eq!(
    ManagementRequestId::from_str(&canonical.to_uppercase()),
    Err(ManagementSecurityError::InvalidRequestId)
  );
  assert_eq!(
    ManagementRequestId::new(Uuid::nil()),
    Err(ManagementSecurityError::InvalidRequestId)
  );
}

#[test]
fn request_context_rejects_untrusted_actor_combinations() {
  let anonymous = ManagementActor::unauthenticated_management();
  let verified_attributes = ManagementRequestAttributes::new(
    ManagementIngress::VerifiedIdentity,
    Some(ManagementClientKind::Interactive),
  );
  assert_eq!(
    ManagementRequestContext::new(
      anonymous,
      ManagementSecurityScope::trusted_network(),
      request_id(),
      verified_attributes,
    ),
    Err(ManagementSecurityError::InvalidAnonymousContext)
  );

  let authenticated = ManagementActor::authenticated("operator-1").unwrap();
  assert_eq!(
    ManagementRequestContext::new(
      authenticated,
      ManagementSecurityScope::new("operator:1").unwrap(),
      request_id(),
      ManagementRequestAttributes::trusted_network(),
    ),
    Err(ManagementSecurityError::InvalidAuthenticatedContext)
  );
  assert_eq!(
    ManagementRequestContext::new(
      ManagementActor::authenticated("operator-1").unwrap(),
      ManagementSecurityScope::trusted_network(),
      request_id(),
      verified_attributes,
    ),
    Err(ManagementSecurityError::InvalidAuthenticatedContext)
  );
}

#[test]
fn canonical_context_carries_only_bounded_safe_facts() {
  let context = ManagementRequestContext::trusted_network(request_id());
  assert_eq!(context.actor().kind(), ManagementActorKind::UnauthenticatedManagement);
  assert_eq!(context.actor().identity(), None);
  assert_eq!(context.security_scope().as_str(), "trusted-network");
  assert_eq!(context.attributes().ingress(), ManagementIngress::TrustedNetwork);
  assert_eq!(context.attributes().client_kind(), None);
  assert_eq!(context.request_id(), request_id());
}

#[test]
fn accepted_context_translates_to_exact_mutation_audit_evidence() {
  let anonymous_context = ManagementRequestContext::trusted_network(request_id());
  let anonymous_audit = crate::MutationAuditContext::try_from(&anonymous_context).unwrap();
  assert_eq!(
    anonymous_audit.actor().kind,
    crate::AuditActorKind::UnauthenticatedManagement
  );
  assert_eq!(anonymous_audit.actor().identity, None);
  assert_eq!(anonymous_audit.security_scope().as_str(), "trusted-network");
  assert_eq!(anonymous_audit.request_identity(), request_id().to_string());

  let authenticated_context = ManagementRequestContext::new(
    ManagementActor::authenticated("operator-1").unwrap(),
    ManagementSecurityScope::new("operator:1").unwrap(),
    request_id(),
    ManagementRequestAttributes::new(
      ManagementIngress::VerifiedIdentity,
      Some(ManagementClientKind::Automation),
    ),
  )
  .unwrap();
  let authenticated_audit = crate::MutationAuditContext::try_from(&authenticated_context).unwrap();
  assert_eq!(
    authenticated_audit.actor().kind,
    crate::AuditActorKind::AuthenticatedManagement
  );
  assert_eq!(authenticated_audit.actor().identity.as_deref(), Some("operator-1"));
  assert_eq!(authenticated_audit.security_scope().as_str(), "operator:1");
  assert_eq!(authenticated_audit.request_identity(), request_id().to_string());
}

#[test]
fn debug_output_redacts_actor_scope_and_resource_identities() {
  let actor = ManagementActor::authenticated("private-subject").unwrap();
  let scope = ManagementSecurityScope::new("private:scope").unwrap();
  let context = ManagementRequestContext::new(
    actor,
    scope,
    request_id(),
    ManagementRequestAttributes::new(
      ManagementIngress::VerifiedIdentity,
      Some(ManagementClientKind::Automation),
    ),
  )
  .unwrap();
  let protected = ManagementResource::instance(
    ManagementResourceKind::Project,
    ManagementResourceIdentity::new("private-resource").unwrap(),
  );
  let protected = protected.unwrap();

  let output = format!("{context:?} {protected:?}");
  assert!(!output.contains("private-subject"));
  assert!(!output.contains("private:scope"));
  assert!(!output.contains("private-resource"));
  assert!(output.contains("<redacted>"));
}

#[test]
fn restricted_visibility_is_non_empty_unique_and_bounded() {
  assert_eq!(
    ManagementVisibility::restricted(Vec::new()),
    Err(ManagementSecurityError::EmptyVisibility)
  );
  assert_eq!(
    ManagementVisibility::restricted([resource(1), resource(1)]),
    Err(ManagementSecurityError::DuplicateVisibilityResource)
  );
  assert_eq!(
    ManagementVisibility::restricted((0..=MAX_MANAGEMENT_VISIBILITY_RESOURCES).map(resource)),
    Err(ManagementSecurityError::VisibilityTooLarge)
  );
  let mut resources = (0..MAX_MANAGEMENT_VISIBILITY_RESOURCES)
    .map(resource)
    .collect::<Vec<_>>();
  resources.push(resource(0));
  assert_eq!(
    ManagementVisibility::restricted(resources),
    Err(ManagementSecurityError::DuplicateVisibilityResource)
  );

  let visibility = ManagementVisibility::restricted([resource(1), resource(2)]).unwrap();
  assert_eq!(visibility.kind(), ManagementVisibilityKind::Restricted);
  assert!(matches!(
    visibility.view(),
    ManagementVisibilityView::Restricted(resources) if resources.len() == 2
  ));
  assert_eq!(ManagementVisibility::all().kind(), ManagementVisibilityKind::All);
  assert_eq!(ManagementVisibility::all().view(), ManagementVisibilityView::All);
  assert_eq!(ManagementVisibility::none().kind(), ManagementVisibilityKind::None);
  assert_eq!(ManagementVisibility::none().view(), ManagementVisibilityView::None);
}

#[test]
fn grant_and_denial_expose_no_policy_internals() {
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  assert_eq!(grant.visibility().kind(), ManagementVisibilityKind::All);

  let denial = ManagementAuthorizationDenial::forbidden();
  assert_eq!(denial.to_string(), "management request is forbidden");
  assert_eq!(format!("{denial:?}"), "ManagementAuthorizationDenial");
}

#[test]
fn trusted_network_policy_covers_every_declared_action_and_resource_kind() {
  let policy = TrustedNetworkManagementPolicy;
  let context = ManagementRequestContext::trusted_network(request_id());

  for action in ManagementAction::ALL {
    for resource_kind in ManagementResourceKind::ALL {
      let resource = representative_resource(resource_kind);
      let grant = run_ready(policy.authorize(&context, action, &resource))
        .expect("canonical trusted-network context must retain current behavior");
      assert_eq!(grant.visibility().kind(), ManagementVisibilityKind::All);
    }
  }
}

#[test]
fn trusted_network_policy_fails_closed_for_every_other_valid_actor_shape() {
  let policy = TrustedNetworkManagementPolicy;
  let resource = ManagementResource::collection(ManagementResourceKind::Project).unwrap();

  for client_kind in [
    None,
    Some(ManagementClientKind::Interactive),
    Some(ManagementClientKind::Automation),
  ] {
    let authenticated = ManagementRequestContext::new(
      ManagementActor::authenticated("operator-1").unwrap(),
      ManagementSecurityScope::new("operator:1").unwrap(),
      request_id(),
      ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, client_kind),
    )
    .unwrap();
    assert_eq!(
      run_ready(policy.authorize(&authenticated, ManagementAction::View, &resource)),
      Err(ManagementAuthorizationDenial::forbidden())
    );
  }

  let augmented_anonymous = ManagementRequestContext::new(
    ManagementActor::unauthenticated_management(),
    ManagementSecurityScope::trusted_network(),
    request_id(),
    ManagementRequestAttributes::new(
      ManagementIngress::TrustedNetwork,
      Some(ManagementClientKind::Interactive),
    ),
  )
  .unwrap();
  assert_eq!(
    run_ready(policy.authorize(&augmented_anonymous, ManagementAction::View, &resource)),
    Err(ManagementAuthorizationDenial::forbidden())
  );
}

#[derive(Clone, Copy)]
struct FixtureCommand;

impl crate::Command for FixtureCommand {
  type Outcome = &'static str;
}

impl ManagementAuthorizationTarget for FixtureCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Create, ManagementResourceKind::Project);

  fn management_resource(&self) -> ManagementResourceResult {
    Ok(resource(7))
  }
}

#[derive(Clone, Copy)]
struct FixtureQuery;

impl crate::Query for FixtureQuery {
  type Outcome = &'static str;
}

impl ManagementAuthorizationTarget for FixtureQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);

  fn management_resource(&self) -> ManagementResourceResult {
    Ok(resource(8))
  }
}

struct InvalidFixtureTarget;

impl ManagementAuthorizationTarget for InvalidFixtureTarget {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Project);

  fn management_resource(&self) -> ManagementResourceResult {
    ManagementResource::instance(
      ManagementResourceKind::Agent,
      ManagementResourceIdentity::new("agent-1").unwrap(),
    )
  }
}

#[test]
fn authorization_targets_reject_resources_that_disagree_with_their_static_mapping() {
  assert_eq!(
    InvalidFixtureTarget.validated_management_resource(),
    Err(ManagementSecurityError::UnsupportedResourceShape)
  );
  assert_eq!(
    resource_identity("x".repeat(MAX_MANAGEMENT_RESOURCE_IDENTITY_BYTES + 1)),
    Err(ManagementSecurityError::InvalidResourceIdentity)
  );
}

struct RecordingPolicy {
  decision: Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial>,
  calls: Mutex<Vec<(ManagementAction, ManagementResource)>>,
}

impl RecordingPolicy {
  fn allow(grant: ManagementAuthorizationGrant) -> Self {
    Self {
      decision: Ok(grant),
      calls: Mutex::new(Vec::new()),
    }
  }

  fn deny() -> Self {
    Self {
      decision: Err(ManagementAuthorizationDenial::forbidden()),
      calls: Mutex::new(Vec::new()),
    }
  }
}

#[async_trait]
impl ManagementAuthorizationPolicy for RecordingPolicy {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    self.calls.lock().unwrap().push((action, resource.clone()));
    self.decision.clone()
  }
}

#[derive(Default)]
struct CommandSpy {
  calls: AtomicUsize,
  observed: Mutex<Option<(ManagementRequestContext, ManagementAuthorizationGrant)>>,
}

#[async_trait]
impl ManagementCommandUseCase<FixtureCommand> for CommandSpy {
  type Error = std::convert::Infallible;

  async fn execute_management_command(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    _command: FixtureCommand,
  ) -> Result<&'static str, Self::Error> {
    self.calls.fetch_add(1, Ordering::SeqCst);
    *self.observed.lock().unwrap() = Some((context.clone(), grant.clone()));
    Ok("command")
  }
}

#[derive(Default)]
struct QuerySpy {
  calls: AtomicUsize,
  observed: Mutex<Option<(ManagementRequestContext, ManagementAuthorizationGrant)>>,
}

#[async_trait]
impl ManagementQueryUseCase<FixtureQuery> for QuerySpy {
  type Error = std::convert::Infallible;

  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    _query: FixtureQuery,
  ) -> Result<&'static str, Self::Error> {
    self.calls.fetch_add(1, Ordering::SeqCst);
    *self.observed.lock().unwrap() = Some((context.clone(), grant.clone()));
    Ok("query")
  }
}

#[test]
fn decorators_pass_the_exact_context_and_policy_grant_to_allowed_use_cases() {
  let context = ManagementRequestContext::trusted_network(request_id());
  let visibility = ManagementVisibility::restricted([resource(9)]).unwrap();
  let grant = ManagementAuthorizationGrant::new(visibility);
  let policy = Arc::new(RecordingPolicy::allow(grant.clone()));
  let command_spy = Arc::new(CommandSpy::default());
  let query_spy = Arc::new(QuerySpy::default());
  let command_handler = AuthorizedCommandHandler::new(policy.clone(), command_spy.clone());
  let query_handler = AuthorizedQueryHandler::new(policy.clone(), query_spy.clone());

  assert_eq!(
    run_ready(command_handler.handle_command(&context, FixtureCommand)).unwrap(),
    "command"
  );
  assert_eq!(
    run_ready(query_handler.handle_query(&context, FixtureQuery)).unwrap(),
    "query"
  );
  assert_eq!(command_spy.calls.load(Ordering::SeqCst), 1);
  assert_eq!(query_spy.calls.load(Ordering::SeqCst), 1);
  assert_eq!(
    *command_spy.observed.lock().unwrap(),
    Some((context.clone(), grant.clone()))
  );
  assert_eq!(*query_spy.observed.lock().unwrap(), Some((context, grant)));
  assert_eq!(
    *policy.calls.lock().unwrap(),
    vec![
      (ManagementAction::Create, resource(7)),
      (ManagementAction::View, resource(8))
    ]
  );
}

#[test]
fn decorators_do_not_dispatch_denied_operations_and_preserve_request_correlation() {
  let context = ManagementRequestContext::trusted_network(request_id());
  let policy = Arc::new(RecordingPolicy::deny());
  let command_spy = Arc::new(CommandSpy::default());
  let query_spy = Arc::new(QuerySpy::default());
  let command_handler = AuthorizedCommandHandler::new(policy.clone(), command_spy.clone());
  let query_handler = AuthorizedQueryHandler::new(policy, query_spy.clone());

  let command_error = run_ready(command_handler.handle_command(&context, FixtureCommand)).unwrap_err();
  let query_error = run_ready(query_handler.handle_query(&context, FixtureQuery)).unwrap_err();

  assert_eq!(command_spy.calls.load(Ordering::SeqCst), 0);
  assert_eq!(query_spy.calls.load(Ordering::SeqCst), 0);
  assert_eq!(command_error.forbidden().unwrap().request_id(), context.request_id());
  assert_eq!(query_error.forbidden().unwrap().request_id(), context.request_id());
  assert!(command_error.application().is_none());
  assert!(query_error.application().is_none());
  assert!(!format!("{command_error:?} {query_error:?}").contains("project-"));
}

#[test]
fn authorized_handlers_remain_mandatory_after_type_erasure() {
  type ErasedCommand = dyn AuthorizedManagementCommandHandler<FixtureCommand, Error = std::convert::Infallible>;
  type ErasedQuery = dyn AuthorizedManagementQueryHandler<FixtureQuery, Error = std::convert::Infallible>;

  let context = ManagementRequestContext::trusted_network(request_id());
  let command_spy = Arc::new(CommandSpy::default());
  let query_spy = Arc::new(QuerySpy::default());
  let command: Arc<ErasedCommand> = Arc::new(AuthorizedCommandHandler::new(
    Arc::new(RecordingPolicy::deny()),
    command_spy.clone(),
  ));
  let query: Arc<ErasedQuery> = Arc::new(AuthorizedQueryHandler::new(
    Arc::new(RecordingPolicy::deny()),
    query_spy.clone(),
  ));

  let command_error = run_ready(command.handle_authorized_command(&context, FixtureCommand)).unwrap_err();
  let query_error = run_ready(query.handle_authorized_query(&context, FixtureQuery)).unwrap_err();

  assert_eq!(command_spy.calls.load(Ordering::SeqCst), 0);
  assert_eq!(query_spy.calls.load(Ordering::SeqCst), 0);
  assert_eq!(command_error.forbidden().unwrap().request_id(), context.request_id());
  assert_eq!(query_error.forbidden().unwrap().request_id(), context.request_id());
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory management policy unexpectedly awaited external I/O"),
  }
}
