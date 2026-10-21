use octacity_work_source_protocol::{decode_request, decode_response};
use serde_json::{Value, json};

fn request() -> Value {
  json!({"family":"review","message":{"protocol_version":1,"request_id":"review-read",
    "command":{"operation":"observe","payload":{
      "read_id":"a".repeat(64),"configuration_digest":"b".repeat(64),
      "resource":{"resource_id":"code-repository","resource_locator":"installed-code",
        "credential":{"class":"vcs_read","handle":"independent-code-reader"}},
      "item_id":"native-review","head":null,"network_hosts":["forge.example"],
      "cursor":null,"max_items":50,"max_page_bytes":65536}}}})
}

#[test]
fn review_observation_is_read_only_bounded_and_bound_to_independent_repository_authority() {
  let wire = request();
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).expect("installed review read contract");
  let mut response = json!({"family":"review","message":{"protocol_version":1,"request_id":"review-read",
    "outcome":{"status":"observed","payload":{
      "subject":{"read_id":"a".repeat(64),"configuration_digest":"b".repeat(64),
        "resource_id":"code-repository","item_id":"native-review"},
      "facts":{"title":"Reviewable change","number":"23","url":null,"author":null,
        "state":"open","draft":false,"head":"original-head","base":"original-base",
        "changed_files":null,"additions":0,"deletions":null,"review":null,"checks":null}}}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  response["message"]["outcome"]["payload"]["subject"]["resource_id"] = json!("ticket-container");
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_err());
  for class in ["work_source_read", "work_reporter_write", "delivery_write"] {
    let mut invalid = wire.clone();
    invalid["message"]["command"]["payload"]["resource"]["credential"]["class"] = json!(class);
    assert!(decode_request(&serde_json::to_vec(&invalid).unwrap()).is_err());
  }
  for operation in ["merge", "publish", "project", "transition"] {
    let mut invalid = wire.clone();
    invalid["message"]["command"]["operation"] = json!(operation);
    assert!(decode_request(&serde_json::to_vec(&invalid).unwrap()).is_err());
  }
  let mut oversized = wire;
  oversized["message"]["command"]["payload"]["max_items"] = json!(101);
  assert!(decode_request(&serde_json::to_vec(&oversized).unwrap()).is_err());
}

#[test]
fn checks_keep_the_original_head_and_partial_pages_do_not_imply_success() {
  let mut wire = request();
  wire["message"]["command"]["operation"] = json!("checks");
  wire["message"]["command"]["payload"]["head"] = json!("original-head");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).expect("bounded check read");
  let mut response = json!({"family":"review","message":{"protocol_version":1,"request_id":"review-read",
    "outcome":{"status":"checks","payload":{
      "subject":{"read_id":"a".repeat(64),"configuration_digest":"b".repeat(64),"resource_id":"code-repository","item_id":"native-review"},
      "head":"original-head","items":[{"id":"check-1","name":"unit tests","producer":null,"state":"completed","conclusion":null,"url":null}],
      "next_cursor":"opaque-next"}}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  response["message"]["outcome"]["payload"]["head"] = json!("new-head");
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_err());
  wire["message"]["command"]["payload"]["head"] = Value::Null;
  assert!(decode_request(&serde_json::to_vec(&wire).unwrap()).is_err());
}

#[test]
fn authorization_and_cancellation_are_correlated_without_observation_authority() {
  let mut wire = request();
  wire["message"]["command"]["operation"] = json!("authorize");
  let request = decode_request(&serde_json::to_vec(&wire).unwrap()).expect("independent grant check");
  let response = json!({"family":"review","message":{"protocol_version":1,"request_id":"review-read",
    "outcome":{"status":"authorized","payload":{"read_id":"a".repeat(64),"configuration_digest":"b".repeat(64),"resource_id":"code-repository","item_id":"native-review"}}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  let cancel = json!({"family":"review","message":{"protocol_version":1,"request_id":"cancel-review-read",
    "command":{"operation":"cancel","payload":{"target_read_id":"a".repeat(64)}}}});
  let request = decode_request(&serde_json::to_vec(&cancel).unwrap()).expect("same-read cancellation");
  let mut response = json!({"family":"review","message":{"protocol_version":1,"request_id":"cancel-review-read",
    "outcome":{"status":"acknowledged","payload":{"read_id":"a".repeat(64)}}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  response["message"]["outcome"]["payload"]["read_id"] = json!("c".repeat(64));
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_err());
}
