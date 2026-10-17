use octacity_work_source_protocol::{decode_request, decode_response};
use serde_json::{Value, json};

fn request(family: &str) -> Value {
  json!({"family":family,"message":{"protocol_version":1,"request_id":"check-request",
    "command":{"operation":"check","payload":{"check_id":"a".repeat(64),
      "configuration_digest":"e".repeat(64),"endpoint":"https://tracker.invalid/api/",
      "resource":{"resource_id":"native-container","resource_locator":"installed-ticket-space",
        "credential":{"class":if family=="source" {"work_source_read"} else {"work_reporter_write"},"handle":"private-check-grant"}},
      "network_hosts":["tracker.invalid"]}}}})
}

#[test]
fn read_only_checks_bind_separate_roles_native_scope_and_safe_readiness_without_execution_inputs() {
  for family in ["source", "reporter"] {
    let wire = request(family);
    let request = decode_request(&serde_json::to_vec(&wire).unwrap()).expect("read-only connection check");
    assert!(!format!("{request:?}").contains("private-check-grant"));
    let response = json!({"family":family,"message":{"protocol_version":1,"request_id":"check-request",
      "outcome":{"status":"checked","payload":{"check_id":"a".repeat(64),"configuration_digest":"e".repeat(64),
        "resource_id":"native-container","configuration":"ready","authentication":"ready",
        "resource_access":"ready","capability":if family=="source" {"ready"} else {"unverified"},"retry_after_millis":null}}}});
    assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
    for (pointer, value) in [
      ("/message/outcome/payload/resource_id", json!("substituted-container")),
      ("/message/outcome/payload/configuration_digest", json!("f".repeat(64))),
      ("/message/outcome/payload/check_id", json!("b".repeat(64))),
      ("/message/outcome/payload/authentication", json!("denied")),
      ("/message/outcome/payload/retry_after_millis", json!(3_600_001)),
    ] {
      let mut invalid = response.clone();
      *invalid.pointer_mut(pointer).unwrap() = value;
      assert!(
        decode_response(&serde_json::to_vec(&invalid).unwrap(), &request).is_err(),
        "{pointer}"
      );
    }
    for field in ["cursor", "item_id", "mapping", "status", "work_id"] {
      let mut invalid = wire.clone();
      invalid["message"]["command"]["payload"][field] = json!("must-not-be-used");
      assert!(decode_request(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut wrong_class = wire.clone();
    wrong_class["message"]["command"]["payload"]["resource"]["credential"]["class"] = json!("delivery_write");
    assert!(decode_request(&serde_json::to_vec(&wrong_class).unwrap()).is_err());
    let mut oversized = serde_json::to_vec(&response).unwrap();
    oversized.extend(vec![b' '; 8193]);
    assert!(
      decode_response(&oversized, &request).is_err(),
      "checks have a small independent response ceiling"
    );
  }
}
