use std::{
  future::Future,
  str::FromStr,
  task::{Context, Poll, Waker},
};

use uuid::Uuid;

use super::*;

fn request_id() -> ManagementRequestId {
  ManagementRequestId::new(Uuid::from_u128(0xabcdef01_2345_6789_abcd_ef0123456789)).expect("non-nil fixture")
}

fn resource(index: usize) -> ManagementResource {
  ManagementResource::instance(
    ManagementResourceKind::Project,
    ManagementResourceIdentity::new(format!("project-{index}")).expect("bounded fixture"),
  )
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

  let visibility = ManagementVisibility::restricted([resource(1), resource(2)]).unwrap();
  assert_eq!(visibility.kind(), ManagementVisibilityKind::Restricted);
  assert_eq!(visibility.resources().unwrap().len(), 2);
  assert_eq!(ManagementVisibility::all().kind(), ManagementVisibilityKind::All);
  assert_eq!(ManagementVisibility::none().kind(), ManagementVisibilityKind::None);
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
      let grant = run_ready(policy.authorize(&context, action, &ManagementResource::collection(resource_kind)))
        .expect("canonical trusted-network context must retain current behavior");
      assert_eq!(grant.visibility().kind(), ManagementVisibilityKind::All);
    }
  }
}

#[test]
fn trusted_network_policy_fails_closed_for_every_other_valid_actor_shape() {
  let policy = TrustedNetworkManagementPolicy;
  let resource = ManagementResource::collection(ManagementResourceKind::Project);

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

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory management policy unexpectedly awaited external I/O"),
  }
}
