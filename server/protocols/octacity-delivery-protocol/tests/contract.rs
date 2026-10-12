use octacity_delivery_protocol::decode_request;

#[test]
fn delivery_requires_its_write_credential_class_and_rejects_read_only_callers() {
  let bytes = include_bytes!("../fixtures/publish-v1.json");
  assert!(
    decode_request(bytes).is_ok(),
    "the canonical publication contract must be supported"
  );
  for class in ["vcs_read", "repository_job", "work_reporter"] {
    let mut request: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    request["command"]["payload"]["repository"]["credential"]["class"] = serde_json::json!(class);
    assert!(
      decode_request(&serde_json::to_vec(&request).unwrap()).is_err(),
      "{class} cannot authorize publication"
    );
  }
}

#[test]
fn delivery_observation_must_match_the_exact_accepted_scope_and_remote_head() {
  use octacity_delivery_protocol::decode_response;
  let request = decode_request(include_bytes!("../fixtures/publish-v1.json")).unwrap();
  let original: serde_json::Value = serde_json::from_slice(include_bytes!("../fixtures/publish-v1.json")).unwrap();
  let intent = &original["command"]["payload"];
  let response = serde_json::json!({
    "protocol_version":1,"request_id":"request-1","outcome":{"status":"observed","payload":{
      "delivery_id":intent["delivery_id"],"repository_id":"repository-1",
      "base_revision":intent["base_revision"],"candidate_revision":intent["candidate_revision"],
      "changeset_digest":intent["changeset_digest"],"decision_digest":intent["decision_digest"],
      "configuration_digest":intent["configuration_digest"],
      "target":intent["target"],"branch_reference":"refs/heads/review/accepted-candidate",
      "remote_head":intent["candidate_revision"],
      "review":{"identity":"review-23","state":"open","head":intent["candidate_revision"],"base":intent["base_revision"]}
    }}
  });
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  let mut repair = response.clone();
  repair["outcome"]["status"] = serde_json::json!("repair_required");
  let octacity_delivery_protocol::Command::Publish(intent) = request.command.clone() else {
    unreachable!()
  };
  let observation = octacity_delivery_protocol::Request {
    command: octacity_delivery_protocol::Command::Observe(intent),
    ..request.clone()
  };
  assert!(decode_response(&serde_json::to_vec(&repair).unwrap(), &observation).is_ok());
  assert!(
    decode_response(&serde_json::to_vec(&repair).unwrap(), &request).is_err(),
    "repair is not publication success"
  );
  repair["outcome"]["payload"]["review"]["state"] = serde_json::json!("closed");
  assert!(decode_response(&serde_json::to_vec(&repair).unwrap(), &observation).is_err());

  for (pointer, value) in [
    ("/request_id", serde_json::json!("foreign-request")),
    ("/outcome/payload/delivery_id", serde_json::json!("e".repeat(64))),
    ("/outcome/payload/remote_head", serde_json::json!("3".repeat(40))),
    ("/outcome/payload/decision_digest", serde_json::json!("f".repeat(64))),
    (
      "/outcome/payload/configuration_digest",
      serde_json::json!("f".repeat(64)),
    ),
    ("/outcome/payload/review/head", serde_json::json!("4".repeat(40))),
  ] {
    let mut replaced = response.clone();
    *replaced.pointer_mut(pointer).unwrap() = value;
    assert!(
      decode_response(&serde_json::to_vec(&replaced).unwrap(), &request).is_err(),
      "substitution at {pointer} cannot claim delivery"
    );
  }
}

#[test]
fn a_write_adapter_must_advertise_observe_before_retry_and_the_delivery_credential_class() {
  use octacity_delivery_protocol::AdapterManifest;
  let value = serde_json::json!({
    "adapter_id":"provider-plugin", "executable_sha256":"a".repeat(64),
    "protocol":{"min":1,"max":1},
    "capabilities":{"observe":true,"publish":true,"cancel":true,
      "idempotency":"observe_before_retry","credential_class":"delivery_write","configuration_digest":"e".repeat(64)}
  });
  let selected: AdapterManifest = serde_json::from_value(value.clone()).unwrap();
  selected.validate().unwrap();
  let mut unsafe_choice = value.clone();
  unsafe_choice["capabilities"]["observe"] = serde_json::json!(false);
  let selected: AdapterManifest = serde_json::from_value(unsafe_choice).unwrap();
  assert!(
    selected.validate().is_err(),
    "publication without observation cannot be safely replayed"
  );
  let mut read_only = value;
  read_only["capabilities"]["credential_class"] = serde_json::json!("vcs_read");
  assert!(serde_json::from_value::<AdapterManifest>(read_only).is_err());
}

#[test]
fn network_ceilings_admit_only_bounded_unique_exact_hosts() {
  let original: serde_json::Value = serde_json::from_slice(include_bytes!("../fixtures/publish-v1.json")).unwrap();
  for hosts in [
    serde_json::json!([]),
    serde_json::json!(["forge.invalid", "forge.invalid"]),
    serde_json::json!(["forge.invalid", "*"]),
    serde_json::json!(["https://forge.invalid"]),
    serde_json::json!(["forge.invalid:443"]),
  ] {
    let mut request = original.clone();
    request["command"]["payload"]["network_hosts"] = hosts;
    assert!(
      decode_request(&serde_json::to_vec(&request).unwrap()).is_err(),
      "invalid network ceilings cannot be admitted"
    );
  }
}
