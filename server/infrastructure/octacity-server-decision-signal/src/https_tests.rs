use super::{capability, key, request};
use octacity_server_factory::{DecisionSignalMode, DecisionSignalState};

#[path = "../../../tests/octacity-release-harness/tests/factory_pilot/jev.rs"]
mod jev_fixture;

#[tokio::test]
async fn installed_jev_https_connector_accepts_only_an_explicitly_trusted_private_ca() {
  const CREDENTIAL: &str = "sk-glm5-https-fixture-private-canary";
  let fixture = jev_fixture::JevHttpsFixture::start("jev-1.13-20260917", CREDENTIAL).await;
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let mut untrusted = fixture.connector_configuration(&capability(&request));
  untrusted.as_object_mut().unwrap().remove("root_certificates_pem");
  let provider = crate::install_decision_signal_connector(
    &key("jev"),
    &serde_json::to_vec(&untrusted).unwrap(),
    CREDENTIAL.to_owned(),
  )
  .unwrap();
  let failure = provider.evaluate(&request).await.unwrap_err();
  assert_eq!(failure.state(), DecisionSignalState::Unavailable);
  assert!(failure.usage().is_none());
  assert!(
    fixture.captured().is_empty(),
    "untrusted TLS cannot reach the HTTP provider"
  );
  assert!(!format!("{failure:?}").contains(CREDENTIAL));
  let configuration = fixture.connector_configuration(&capability(&request));
  let provider = crate::install_decision_signal_connector(
    &key("jev"),
    &serde_json::to_vec(&configuration).unwrap(),
    CREDENTIAL.to_owned(),
  )
  .expect("an operator's explicit private root must qualify this exact HTTPS connector");
  let result = provider.evaluate(&request).await.unwrap();
  assert_eq!(result.request_id(), request.id());
  assert_eq!(result.model(), request.model());
  assert_eq!(result.usage().tokens, 10);
  let captured = fixture.captured();
  assert_eq!(captured.len(), 1);
  assert_eq!(captured[0].method, "POST");
  assert_eq!(captured[0].path, "/api/v1/systemone/");
  assert_eq!(
    captured[0].idempotency_key.as_deref(),
    Some(request.id().as_uuid().to_string().as_str())
  );
  assert!(captured[0].credential_matched);
  assert_eq!(captured[0].body["model"], "jev-1.13");
  fixture.shutdown().await;
}

#[tokio::test]
async fn a_lost_https_response_cannot_be_observed_as_free_or_redispatched() {
  const CREDENTIAL: &str = "sk-glm5-https-unknown-private-canary";
  let fixture = jev_fixture::JevHttpsFixture::start_with_mode(
    "jev-1.13-20260917",
    CREDENTIAL,
    jev_fixture::JevResponseMode::DropAfterAccept,
  )
  .await;
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let configuration = fixture.connector_configuration(&capability(&request));
  let provider = crate::install_decision_signal_connector(
    &key("jev"),
    &serde_json::to_vec(&configuration).unwrap(),
    CREDENTIAL.to_owned(),
  )
  .unwrap();
  let failure = provider.evaluate(&request).await.unwrap_err();
  assert_eq!(failure.state(), DecisionSignalState::Unavailable);
  assert!(failure.usage().is_none());
  assert_eq!(fixture.captured().len(), 1, "the provider accepted exactly one POST");
  assert!(provider.observe_evaluation(&request).await.is_none());
  assert_eq!(
    fixture.captured().len(),
    1,
    "readonly recovery must not redispatch a POST or guess an external request id"
  );
  assert!(!format!("{failure:?}").contains(CREDENTIAL));
  fixture.shutdown().await;
}

#[tokio::test]
async fn explicit_private_roots_preserve_hostname_verification_and_bounded_safe_configuration() {
  const CREDENTIAL: &str = "sk-glm5-https-config-private-canary";
  let fixture = jev_fixture::JevHttpsFixture::start("jev-1.13-20260917", CREDENTIAL).await;
  let request = request("jev-1.13-20260917", DecisionSignalMode::Shadow);
  let configuration = fixture.connector_configuration(&capability(&request));
  let root = fixture.root_certificate_pem();
  for roots in [
    serde_json::json!(["private-malformed-pem-canary"]),
    serde_json::json!(["x".repeat(16 * 1024 + 1)]),
    serde_json::json!(vec![root; 9]),
    serde_json::json!([format!("{root}{root}")]),
  ] {
    let mut invalid = configuration.clone();
    invalid["root_certificates_pem"] = roots;
    let error = crate::install_decision_signal_connector(
      &key("jev"),
      &serde_json::to_vec(&invalid).unwrap(),
      CREDENTIAL.to_owned(),
    )
    .err()
    .expect("private roots must remain a bounded list of individual PEM certificates");
    assert_eq!(error.to_string(), "Decision Signal connector configuration is invalid");
    assert!(!format!("{error:?}").contains("canary"));
  }
  let mut wrong_host = configuration;
  wrong_host["endpoint"] = serde_json::json!(fixture.endpoint().replace("localhost", "127.0.0.1"));
  let provider = crate::install_decision_signal_connector(
    &key("jev"),
    &serde_json::to_vec(&wrong_host).unwrap(),
    CREDENTIAL.to_owned(),
  )
  .unwrap();
  let failure = provider.evaluate(&request).await.unwrap_err();
  assert_eq!(failure.state(), DecisionSignalState::Unavailable);
  assert!(failure.usage().is_none());
  assert!(
    fixture.captured().is_empty(),
    "explicit trust must not disable hostname validation"
  );
  fixture.shutdown().await;
}
