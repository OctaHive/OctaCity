use std::sync::atomic::{AtomicBool, Ordering};

use octacity_server_factory::{FactoryCredentialConsumer, FactoryCredentialProfiles, FactoryKey, StageAttemptId};
use octacity_server_secrets::DelegatedGrantCredential;

use crate::{FactoryCredentialResolutionError, resolve_factory_credential};

const MATERIAL: &[u8] = b"materialized-provider-credential";

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn profiles() -> FactoryCredentialProfiles {
  FactoryCredentialProfiles::new(
    key("model-coding"),
    key("model-evaluation"),
    key("source-read"),
    key("delivery-write"),
  )
  .unwrap()
}

#[test]
fn coding_cannot_materialize_delivery_identity() {
  let invoked = AtomicBool::new(false);
  let result = resolve_factory_credential(
    &profiles(),
    StageAttemptId::generate(),
    FactoryCredentialConsumer::CodingStage,
    &key("delivery-write"),
    |_| {
      invoked.store(true, Ordering::SeqCst);
      DelegatedGrantCredential::new(MATERIAL.to_vec())
    },
  );

  assert!(matches!(result, Err(FactoryCredentialResolutionError::Scope(_))));
  assert!(!invoked.load(Ordering::SeqCst));
}

#[test]
fn authorized_material_is_transient_and_redacted_from_diagnostics() {
  for (consumer, profile) in [
    (FactoryCredentialConsumer::CodingStage, "model-coding"),
    (FactoryCredentialConsumer::EvaluationStage, "model-evaluation"),
    (FactoryCredentialConsumer::SourceResolver, "source-read"),
    (FactoryCredentialConsumer::DeliveryAdapter, "delivery-write"),
  ] {
    let credential =
      resolve_factory_credential(&profiles(), StageAttemptId::generate(), consumer, &key(profile), |_| {
        DelegatedGrantCredential::new(MATERIAL.to_vec())
      })
      .unwrap();
    let diagnostic = format!("{credential:?}");
    assert!(!diagnostic.contains("materialized-provider-credential"));
    assert!(diagnostic.contains("[REDACTED]"));
    assert_eq!(credential.authorization().consumer(), consumer);
    assert_eq!(credential.into_material().expose_for_delivery(), MATERIAL);
  }
}
