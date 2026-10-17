use octacity_work_source_protocol::{decode_request, decode_response};
use serde_json::{Value, json};

fn cancellation(family: &str, target: &str) -> Value {
  json!({"family":family,"message":{"protocol_version":1,"request_id":"request-1",
    "command":{"operation":"cancel","payload":{target:"a".repeat(64)}}}})
}

#[test]
fn source_cancellation_is_correlated_without_reporter_or_delivery_authority() {
  let wire = cancellation("source", "target_scan_id");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  assert_eq!(serde_json::to_value(&request).unwrap(), wire);
  let response = json!({"family":"source","message":{"protocol_version":1,"request_id":"request-1",
    "outcome":{"status":"acknowledged","payload":{"scan_id":"a".repeat(64)}}}});
  let decoded = decode_response(&serde_json::to_vec(&response).unwrap(), &request).unwrap();
  assert_eq!(serde_json::to_value(decoded).unwrap(), response);
  let mut foreign = response;
  foreign["message"]["request_id"] = json!("foreign");
  assert!(decode_response(&serde_json::to_vec(&foreign).unwrap(), &request).is_err());
}

#[test]
fn scan_requires_a_separate_read_grant_and_bounded_installed_selection() {
  let original: Value = serde_json::from_slice(include_bytes!("../fixtures/scan-v1.json")).unwrap();
  let decode = |wire: &Value| decode_request(&serde_json::to_vec(wire).unwrap());
  let request = decode(&original).expect("a provider-neutral read-only scan is supported");
  let debug = format!("{request:?}");
  assert!(!debug.contains("private-source-grant"));
  assert!(!debug.contains("installed-ticket-space"));
  for (pointer, value) in [
    (
      "/message/command/payload/resource/credential/class",
      json!("delivery_write"),
    ),
    (
      "/message/command/payload/resource/credential/class",
      json!("work_reporter_write"),
    ),
    ("/message/command/payload/max_items", json!(0)),
    ("/message/command/payload/max_items", json!(101)),
    ("/message/command/payload/max_snapshot_bytes", json!(1048577)),
    ("/message/command/payload/max_page_bytes", json!(4194305)),
    ("/message/command/payload/window/from_millis", json!(201)),
    ("/message/command/payload/selection/sha256", json!("mutable")),
    ("/message/command/payload/network_hosts", json!(["*"])),
  ] {
    let mut wire = original.clone();
    *wire.pointer_mut(pointer).unwrap() = value;
    assert!(decode(&wire).is_err(), "invalid scan field {pointer} must fail closed");
  }
}

fn page() -> Value {
  json!({"family":"source","message":{"protocol_version":1,"request_id":"scan-request-1",
  "outcome":{"status":"page","payload":{
    "scan_id":"a".repeat(64),"resource_id":"native-container","configuration_digest":"e".repeat(64),
    "selection":{"identity":"arbitrary-installed-selector","sha256":"b".repeat(64)},
    "next_cursor":"private-continuation", "candidates":[{
      "container_id":"native-container","item_id":"native-item","display_id":"TASK-7",
      "update_identity":"update-23","title":"Private task title","body":"Private task body\nacceptance",
      "fields":{"custom_acceptance":"Explicit acceptance"},"labels":["arbitrary-label"],"state":"provider-native-state"
    }]}}}})
}

#[test]
fn pages_are_private_bounded_observations_of_the_exact_installed_scan() {
  let request = decode_request(include_bytes!("../fixtures/scan-v1.json")).unwrap();
  let wire = page();
  let response = decode_response(&serde_json::to_vec(&wire).unwrap(), &request)
    .expect("normalized snapshots need no provider-specific state or identity enum");
  let debug = format!("{response:?}");
  for private in [
    "Private task title",
    "Private task body",
    "private-continuation",
    "Explicit acceptance",
  ] {
    assert!(!debug.contains(private), "private inputs must not enter diagnostics");
  }
  for (pointer, value) in [
    ("/message/outcome/payload/scan_id", json!("f".repeat(64))),
    ("/message/outcome/payload/configuration_digest", json!("f".repeat(64))),
    ("/message/outcome/payload/resource_id", json!("another-container")),
    ("/message/outcome/payload/selection/sha256", json!("f".repeat(64))),
    (
      "/message/outcome/payload/candidates/0/container_id",
      json!("another-container"),
    ),
    ("/message/outcome/payload/candidates/0/title", json!("x".repeat(4097))),
    ("/message/outcome/payload/candidates/0/body", json!("x".repeat(262145))),
    ("/message/outcome/payload/next_cursor", json!("x".repeat(4097))),
  ] {
    let mut replaced = wire.clone();
    *replaced.pointer_mut(pointer).unwrap() = value;
    assert!(
      decode_response(&serde_json::to_vec(&replaced).unwrap(), &request).is_err(),
      "{pointer}"
    );
  }
  for ceiling in ["max_snapshot_bytes", "max_page_bytes"] {
    let mut narrowed: Value = serde_json::from_slice(include_bytes!("../fixtures/scan-v1.json")).unwrap();
    narrowed["message"]["command"]["payload"][ceiling] = json!(100);
    let narrowed = decode_request(&serde_json::to_vec(&narrowed).unwrap()).unwrap();
    assert!(decode_response(&serde_json::to_vec(&wire).unwrap(), &narrowed).is_err());
  }
  let mut duplicate = wire;
  let candidates = duplicate["message"]["outcome"]["payload"]["candidates"]
    .as_array_mut()
    .unwrap();
  candidates.push(candidates[0].clone());
  assert!(decode_response(&serde_json::to_vec(&duplicate).unwrap(), &request).is_err());
}

#[test]
fn reporter_accepts_only_pinned_effects_with_its_own_write_authority() {
  let original: Value = serde_json::from_slice(include_bytes!("../fixtures/project-v1.json")).unwrap();
  let request = decode_request(&serde_json::to_vec(&original).unwrap()).expect("Reporter negotiates separately");
  assert!(!format!("{request:?}").contains("private-reporter-grant"));
  for class in ["work_source_read", "delivery_write", "vcs_read", "repository_job"] {
    let mut wire = original.clone();
    wire["message"]["command"]["payload"]["resource"]["credential"]["class"] = json!(class);
    assert!(decode_request(&serde_json::to_vec(&wire).unwrap()).is_err(), "{class}");
  }
  let receipt = json!({"projection_id":"c".repeat(64),"resource_id":"native-container","item_id":"native-item",
    "configuration_digest":"e".repeat(64),"mapping":{"identity":"arbitrary-installed-effects","sha256":"d".repeat(64)},
    "status":"needs-human-eyes", "matches":true});
  let response = json!({"family":"reporter","message":{"protocol_version":1,"request_id":"projection-request-1",
    "outcome":{"status":"applied","payload":receipt}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  for (pointer, value) in [
    ("/message/outcome/payload/projection_id", json!("f".repeat(64))),
    ("/message/outcome/payload/resource_id", json!("another-container")),
    ("/message/outcome/payload/item_id", json!("another-item")),
    ("/message/outcome/payload/configuration_digest", json!("f".repeat(64))),
    ("/message/outcome/payload/mapping/sha256", json!("f".repeat(64))),
    ("/message/outcome/payload/status", json!("inferred-status")),
    ("/message/outcome/payload/matches", json!(false)),
  ] {
    let mut wire = response.clone();
    *wire.pointer_mut(pointer).unwrap() = value;
    assert!(
      decode_response(&serde_json::to_vec(&wire).unwrap(), &request).is_err(),
      "{pointer}"
    );
  }
  let mut observation = original;
  observation["message"]["command"]["operation"] = json!("observe");
  let observation = decode_request(&serde_json::to_vec(&observation).unwrap()).unwrap();
  let mut observed = response;
  observed["message"]["outcome"]["status"] = json!("observed");
  observed["message"]["outcome"]["payload"]["matches"] = json!(false);
  assert!(decode_response(&serde_json::to_vec(&observed).unwrap(), &observation).is_ok());
  let cancellation =
    decode_request(&serde_json::to_vec(&cancellation("reporter", "target_projection_id")).unwrap()).unwrap();
  let ack = json!({"family":"reporter","message":{"protocol_version":1,"request_id":"request-1",
    "outcome":{"status":"acknowledged","payload":{"projection_id":"a".repeat(64)}}}});
  assert!(decode_response(&serde_json::to_vec(&ack).unwrap(), &cancellation).is_ok());
  assert!(decode_response(&serde_json::to_vec(&ack).unwrap(), &request).is_err());
}

#[test]
fn advertised_roles_negotiate_independently_and_cannot_borrow_credentials() {
  use octacity_work_source_protocol::{AdapterManifest, ProtocolRange};
  let original = json!({"adapter_id":"any-provider","executable_sha256":"a".repeat(64),
    "protocol":{"source":{"min":2,"max":3},"reporter":{"min":1,"max":1}},
    "capabilities":{
      "source":{"cancel":true,"credential_class":"work_source_read","configuration_digest":"e".repeat(64)},
      "reporter":{"observe":true,"project":true,"cancel":true,"idempotency":"observe_before_retry",
        "credential_class":"work_reporter_write","configuration_digest":"f".repeat(64)}}});
  let manifest: AdapterManifest = serde_json::from_value(original.clone()).unwrap();
  manifest.validate().unwrap();
  let local = ProtocolRange { min: 1, max: 1 };
  assert!(local.negotiate(manifest.protocol.source.unwrap()).is_err());
  assert_eq!(local.negotiate(manifest.protocol.reporter.unwrap()).unwrap(), 1);
  for pointer in [
    "/capabilities/source/credential_class",
    "/capabilities/reporter/credential_class",
  ] {
    let mut wrong = original.clone();
    *wrong.pointer_mut(pointer).unwrap() = json!("delivery_write");
    assert!(serde_json::from_value::<AdapterManifest>(wrong).is_err());
  }
  for (pointer, value) in [
    ("/capabilities/source/credential_class", json!("work_reporter_write")),
    ("/capabilities/reporter/observe", json!(false)),
    ("/protocol/source", Value::Null),
  ] {
    let mut wrong = original.clone();
    *wrong.pointer_mut(pointer).unwrap() = value;
    assert!(
      serde_json::from_value::<AdapterManifest>(wrong)
        .unwrap()
        .validate()
        .is_err()
    );
  }
  let mut reporter_only = original;
  reporter_only["protocol"].as_object_mut().unwrap().remove("source");
  reporter_only["capabilities"].as_object_mut().unwrap().remove("source");
  serde_json::from_value::<AdapterManifest>(reporter_only)
    .unwrap()
    .validate()
    .unwrap();
}

#[test]
fn failures_are_closed_safe_and_keep_outages_distinct_from_empty_pages() {
  for fixture in [
    include_bytes!("../fixtures/scan-v1.json").as_slice(),
    include_bytes!("../fixtures/project-v1.json").as_slice(),
  ] {
    let request = decode_request(fixture).unwrap();
    let wire: Value = serde_json::from_slice(fixture).unwrap();
    let response = json!({"family":wire["family"],"message":{"protocol_version":1,"request_id":wire["message"]["request_id"],
      "outcome":{"status":"failure","payload":{"class":"transient","retry_after_millis":1000}}}});
    assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
    let mut raw = response.clone();
    raw["message"]["outcome"]["payload"]["diagnostic"] = json!("private provider text");
    assert!(decode_response(&serde_json::to_vec(&raw).unwrap(), &request).is_err());
    let mut excessive = response;
    excessive["message"]["outcome"]["payload"]["retry_after_millis"] = json!(3600001);
    assert!(decode_response(&serde_json::to_vec(&excessive).unwrap(), &request).is_err());
  }
}

#[test]
fn wire_decoding_rejects_unknown_fields_versions_families_and_oversized_frames() {
  use octacity_work_source_protocol::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES};
  let original: Value = serde_json::from_slice(include_bytes!("../fixtures/scan-v1.json")).unwrap();
  for pointer in [
    "",
    "/message",
    "/message/command",
    "/message/command/payload",
    "/message/command/payload/resource",
    "/message/command/payload/resource/credential",
    "/message/command/payload/window",
    "/message/command/payload/selection",
  ] {
    let mut wire = original.clone();
    wire
      .pointer_mut(pointer)
      .unwrap()
      .as_object_mut()
      .unwrap()
      .insert("unknown".into(), json!(true));
    assert!(
      decode_request(&serde_json::to_vec(&wire).unwrap()).is_err(),
      "{pointer}"
    );
  }
  for (pointer, value) in [
    ("/family", json!("delivery")),
    ("/message/protocol_version", json!(2)),
    ("/message/command/operation", json!("admit_work")),
  ] {
    let mut wire = original.clone();
    *wire.pointer_mut(pointer).unwrap() = value;
    assert!(decode_request(&serde_json::to_vec(&wire).unwrap()).is_err());
  }
  assert!(decode_request(&vec![b' '; MAX_REQUEST_BYTES + 1]).is_err());
  let request = decode_request(&serde_json::to_vec(&original).unwrap()).unwrap();
  assert!(decode_response(&vec![b' '; MAX_RESPONSE_BYTES + 1], &request).is_err());
  let mut page = page();
  page["message"]["outcome"]["payload"]["candidates"][0]["authority"] = json!("run_factory");
  assert!(decode_response(&serde_json::to_vec(&page).unwrap(), &request).is_err());
}

#[test]
fn narrowed_page_byte_ceiling_applies_to_the_received_frame_including_whitespace() {
  let mut wire: Value = serde_json::from_slice(include_bytes!("../fixtures/scan-v1.json")).unwrap();
  wire["message"]["command"]["payload"]["max_page_bytes"] = json!(2000);
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).unwrap();
  let mut encoded = serde_json::to_vec(&page()).unwrap();
  assert!(encoded.len() < 2000);
  encoded.resize(2001, b' ');
  assert!(decode_response(&encoded, &request).is_err());
}

#[test]
fn delivery_keeps_its_independent_wire_and_cannot_install_work_authority() {
  let delivery = include_bytes!("../../octacity-delivery-protocol/fixtures/publish-v1.json");
  assert!(octacity_delivery_protocol::decode_request(delivery).is_ok());
  assert!(decode_request(delivery).is_err());
  for work in [
    include_bytes!("../fixtures/scan-v1.json").as_slice(),
    include_bytes!("../fixtures/project-v1.json").as_slice(),
  ] {
    assert!(decode_request(work).is_ok());
    assert!(octacity_delivery_protocol::decode_request(work).is_err());
  }
}

#[test]
fn exclusive_group_selectors_are_evaluated_against_one_snapshot_with_exact_matching_references() {
  let mut wire: Value = serde_json::from_slice(include_bytes!("../fixtures/scan-v1.json")).unwrap();
  let primary = wire["message"]["command"]["payload"]["selection"].clone();
  let alternative = json!({"identity":"another-input-selector","sha256":"c".repeat(64)});
  wire["message"]["command"]["payload"]["additional_selections"] = json!([alternative.clone()]);
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).expect("bounded union selection must be supported");
  let mut response = page();
  response["message"]["outcome"]["payload"]["matching_selections"] = json!([[primary.clone(), alternative.clone()]]);
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  for invalid in [
    json!([]),
    json!([[]]),
    json!([[primary.clone(), primary.clone()]]),
    json!([[{"identity":"foreign","sha256":"d".repeat(64)}]]),
  ] {
    response["message"]["outcome"]["payload"]["matching_selections"] = invalid;
    assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_err());
  }
  wire["message"]["command"]["payload"]["additional_selections"] = json!([primary]);
  assert!(decode_request(&serde_json::to_vec(&wire).unwrap()).is_err());
  wire["message"]["command"]["payload"]["additional_selections"] = json!(vec![alternative; 64]);
  assert!(decode_request(&serde_json::to_vec(&wire).unwrap()).is_err());
}

#[test]
fn source_access_checks_are_read_scoped_and_cannot_substitute_container_or_profile() {
  let mut request: serde_json::Value = serde_json::from_slice(include_bytes!("../fixtures/scan-v1.json")).unwrap();
  request["message"]["command"]["operation"] = serde_json::json!("authorize");
  request["message"]["command"]["payload"]
    .as_object_mut()
    .unwrap()
    .retain(|key, _| ["scan_id", "configuration_digest", "resource", "network_hosts"].contains(&key.as_str()));
  let decoded = octacity_work_source_protocol::decode_request(&serde_json::to_vec(&request).unwrap())
    .expect("Source must verify a current read grant independently of another discovery page");
  let intent = &request["message"]["command"]["payload"];
  let response = serde_json::json!({"family":"source","message":{"protocol_version":1,"request_id":request["message"]["request_id"],"outcome":{"status":"authorized","payload":{"scan_id":intent["scan_id"],"resource_id":intent["resource"]["resource_id"],"configuration_digest":intent["configuration_digest"]}}}});
  assert!(octacity_work_source_protocol::decode_response(&serde_json::to_vec(&response).unwrap(), &decoded).is_ok());
  for (field, value) in [
    ("resource_id", serde_json::json!("foreign-container")),
    ("configuration_digest", serde_json::json!("f".repeat(64))),
  ] {
    let mut forged = response.clone();
    forged["message"]["outcome"]["payload"][field] = value;
    assert!(octacity_work_source_protocol::decode_response(&serde_json::to_vec(&forged).unwrap(), &decoded).is_err());
  }
  request["message"]["command"]["payload"]["resource"]["credential"]["class"] =
    serde_json::json!("work_reporter_write");
  assert!(octacity_work_source_protocol::decode_request(&serde_json::to_vec(&request).unwrap()).is_err());
}

#[test]
fn ordered_reporter_negotiates_inspection_and_one_opaque_step_without_bulk_writes() {
  let mut inspect: Value = serde_json::from_slice(include_bytes!("../fixtures/project-v1.json")).unwrap();
  inspect["message"]["protocol_version"] = json!(2);
  inspect["message"]["command"]["operation"] = json!("inspect");
  let request = decode_request(&serde_json::to_vec(&inspect).unwrap()).expect("ordered inspection is supported");
  let receipt = json!({"projection_id":"c".repeat(64),"resource_id":"native-container","item_id":"native-item",
    "configuration_digest":"e".repeat(64),"mapping":{"identity":"arbitrary-installed-effects","sha256":"d".repeat(64)},
    "status":"needs-human-eyes", "matches":false});
  let plan = json!({"receipt":receipt,"fields":["a".repeat(64)],"steps":[{"fields":["a".repeat(64)],"token":"private-provider-step"}]});
  let response = json!({"family":"reporter","message":{"protocol_version":2,"request_id":"projection-request-1",
    "outcome":{"status":"planned","payload":plan}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  let mut apply = inspect.clone();
  apply["message"]["command"] =
    json!({"operation":"apply","payload":{"intent":inspect["message"]["command"]["payload"],"step":plan["steps"][0]}});
  let step = decode_request(&serde_json::to_vec(&apply).unwrap()).expect("one opaque step is supported");
  assert!(!format!("{step:?}").contains("private-provider-step"));
  let mut progressed = response.clone();
  progressed["message"]["outcome"]["status"] = json!("progressed");
  assert!(decode_response(&serde_json::to_vec(&progressed).unwrap(), &step).is_ok());
  assert!(decode_response(&serde_json::to_vec(&progressed).unwrap(), &request).is_err());
  for (pointer, value) in [
    ("/message/outcome/payload/fields", json!(["not-a-field-digest"])),
    ("/message/outcome/payload/steps/0/fields", json!(["b".repeat(64)])),
    ("/message/outcome/payload/steps/0/token", json!("x".repeat(8193))),
    ("/message/outcome/payload/receipt/matches", json!(true)),
  ] {
    let mut invalid = response.clone();
    *invalid.pointer_mut(pointer).unwrap() = value;
    assert!(
      decode_response(&serde_json::to_vec(&invalid).unwrap(), &request).is_err(),
      "{pointer}"
    );
  }
  inspect["message"]["command"]["operation"] = json!("project");
  assert!(
    decode_request(&serde_json::to_vec(&inspect).unwrap()).is_err(),
    "v2 cannot request an unguarded multi-write operation"
  );
  inspect["message"]["protocol_version"] = json!(1);
  inspect["message"]["command"]["operation"] = json!("inspect");
  assert!(
    decode_request(&serde_json::to_vec(&inspect).unwrap()).is_err(),
    "v1 remains closed"
  );
}
