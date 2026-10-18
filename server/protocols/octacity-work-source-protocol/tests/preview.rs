use octacity_work_source_protocol::{decode_request, decode_response};
use serde_json::{Value, json};
#[test]
fn preview_returns_bounded_filtered_summaries_and_reasons_without_private_snapshots() {
  let mut request: Value = serde_json::from_str(include_str!("../fixtures/scan-v1.json")).unwrap();
  request["message"]["command"]["operation"] = json!("preview");
  request["message"]["command"]["payload"]["max_items"] = json!(2);
  let request = decode_request(&serde_json::to_vec(&request).unwrap()).expect("separate read-only preview");
  let selection = json!({"identity":"arbitrary-installed-selector","sha256":"b".repeat(64)});
  let response = json!({"family":"source","message":{"protocol_version":1,"request_id":"scan-request-1",
    "outcome":{"status":"previewed","payload":{"scan_id":"a".repeat(64),"resource_id":"native-container",
      "configuration_digest":"e".repeat(64),"candidates":[
        {"item_id":"native-1","display_id":"ONE-1","title":"Summary","state":"open","labels":[],"selections":[{"selection":selection,"reasons":[]}]},
        {"item_id":"native-2","display_id":"ONE-2","title":"Filtered","state":"closed","labels":["on-hold"],"selections":[{"selection":selection,"reasons":["state_mismatch","excluded_label_present"]}]}
      ],"next_cursor":null}}}});
  assert!(decode_response(&serde_json::to_vec(&response).unwrap(), &request).is_ok());
  for (pointer, value) in [
    ("/message/outcome/payload/resource_id", json!("another-container")),
    ("/message/outcome/payload/candidates/0/title", json!("a".repeat(513))),
    ("/message/outcome/payload/candidates/1/item_id", json!("native-1")),
    (
      "/message/outcome/payload/candidates/1/selections/0/selection/identity",
      json!("unselected"),
    ),
    (
      "/message/outcome/payload/candidates/1/selections/0/reasons",
      json!(["private-provider-error"]),
    ),
  ] {
    let mut invalid = response.clone();
    *invalid.pointer_mut(pointer).unwrap() = value;
    assert!(
      decode_response(&serde_json::to_vec(&invalid).unwrap(), &request).is_err(),
      "{pointer}"
    );
  }
  for field in ["body", "fields", "credential", "issue_id", "work_id"] {
    let mut invalid = response.clone();
    invalid["message"]["outcome"]["payload"]["candidates"][0][field] = json!("private-sentinel");
    assert!(
      decode_response(&serde_json::to_vec(&invalid).unwrap(), &request).is_err(),
      "{field}"
    );
  }
}
