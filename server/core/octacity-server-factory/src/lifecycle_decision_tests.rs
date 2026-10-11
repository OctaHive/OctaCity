use crate::*;

#[test]
fn technical_lifecycle_rejects_retired_business_phase_documents() {
  for legacy in [
    r#"{"Stage":{"target":"Implementation","progress":"Ready"}}"#,
    r#"{"Evaluating":"PlanRequired"}"#,
    r#"{"ReadyForDelivery":"AwaitingApproval"}"#,
    r#"{"Delivering":"Ready"}"#,
    r#"{"Escalated":"Disabled"}"#,
    r#"{"Rejected":"Disabled"}"#,
    r#"{"Cancelled":"Disabled"}"#,
    r#""Completed""#,
  ] {
    assert!(
      serde_json::from_str::<FactoryLifecycleProgress>(legacy).is_err(),
      "retired logical phases cannot be restored as technical Run progress: {legacy}"
    );
  }
}

#[test]
fn technical_progress_preserves_root_identity_and_terminal_immutability() {
  let root = FlowRunId::generate();
  let running = FactoryLifecycleProgress::FlowRunning { root };
  let resolved = FactoryLifecycleProgress::FlowResolved {
    root,
    terminal: FactoryKey::new("custom.done").unwrap(),
  };
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Admitted,
      &FactoryLifecycleProgress::Admitted,
      FactoryRunState::Running,
      &running
    )
    .is_ok()
  );
  assert!(
    validate_lifecycle_transition(FactoryRunState::Running, &running, FactoryRunState::Waiting, &running).is_ok()
  );
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Waiting,
      &running,
      FactoryRunState::Completed,
      &resolved
    )
    .is_ok()
  );
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Running,
      &running,
      FactoryRunState::Running,
      &FactoryLifecycleProgress::FlowRunning {
        root: FlowRunId::generate()
      }
    )
    .is_err()
  );
  assert!(
    validate_lifecycle_transition(
      FactoryRunState::Completed,
      &resolved,
      FactoryRunState::Running,
      &running
    )
    .is_err()
  );
  assert!(validate_lifecycle_progress(FactoryRunState::Completed, &running).is_err());
  assert!(validate_lifecycle_progress(FactoryRunState::Cancelled, &resolved).is_err());
}

#[test]
fn claims_reject_stale_fences_and_exclusive_deadlines() {
  use octacity_server_domain::Timestamp;
  let at = |value| Timestamp::from_unix_millis(value).unwrap();
  let fence = FactoryClaimFence::new(FactoryDigest::from_bytes([1; 32]));
  let claim = FactoryClaim::new(fence, at(10), at(20)).unwrap();
  assert!(claim.verify_fence(fence, at(10)).is_ok());
  assert!(claim.verify_fence(fence, at(19)).is_ok());
  assert_eq!(claim.verify_fence(fence, at(20)), Err(FactoryError::ExpiredClaim));
  assert_eq!(claim.verify_fence(fence, at(9)), Err(FactoryError::InvalidClaim));
  assert_eq!(
    claim.verify_fence(FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])), at(11)),
    Err(FactoryError::StaleClaim)
  );
  assert_eq!(
    FactoryClaim::new(fence, at(10), at(10)),
    Err(FactoryError::InvalidClaim)
  );
}

#[test]
fn technical_progress_rejects_unknown_authority_fields() {
  let root = FlowRunId::generate();
  let wire = serde_json::json!({"FlowRunning": {"root":root, "stage":"implementation"}});
  assert!(serde_json::from_value::<FactoryLifecycleProgress>(wire).is_err());
}
