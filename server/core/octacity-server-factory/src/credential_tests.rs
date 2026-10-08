use crate::{FactoryCredentialConsumer, FactoryCredentialProfiles, FactoryError, FactoryKey, StageAttemptId};

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
fn credential_profiles_are_distinct_and_serialize_without_material() {
  let profiles = profiles();
  let encoded = serde_json::to_string(&profiles).unwrap();
  assert!(encoded.contains("model-coding"));
  assert!(!encoded.contains("credential_value"));
  assert_eq!(
    serde_json::from_str::<FactoryCredentialProfiles>(&encoded).unwrap(),
    profiles
  );

  assert!(matches!(
    FactoryCredentialProfiles::new(key("shared"), key("shared"), key("source-read"), key("delivery-write"),),
    Err(FactoryError::InvalidReference {
      relationship: "distinct credential profiles"
    })
  ));
}

#[test]
fn credential_authority_is_bound_to_stage_and_trusted_consumer() {
  let stage = StageAttemptId::generate();
  let authorized = profiles()
    .authorize(stage, FactoryCredentialConsumer::CodingStage, &key("model-coding"))
    .unwrap();
  assert_eq!(authorized.stage_attempt_id(), stage);
  assert_eq!(authorized.consumer(), FactoryCredentialConsumer::CodingStage);
  assert_eq!(authorized.profile(), &key("model-coding"));

  assert!(matches!(
    profiles().authorize(stage, FactoryCredentialConsumer::CodingStage, &key("delivery-write")),
    Err(FactoryError::InvalidReference {
      relationship: "credential profile consumer scope"
    })
  ));
}
