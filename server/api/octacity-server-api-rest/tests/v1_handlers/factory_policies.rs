//! Every policy route participates in the common authorization-before-dispatch contract.
use super::*;
pub(super) fn management_requests() -> Vec<Request<Body>> {
  let project = "11111111-1111-4111-8111-111111111111";
  let policy = "22222222-2222-4222-8222-222222222222";
  let base = format!("/api/v1/projects/{project}/factory-policies");
  let draft = serde_json::json!({"kind":"launch","id":policy,"expected_current":null,"definition":{
    "name":"Manual","allow_manual":true,"allow_automatic_source":false
  }})
  .to_string();
  let legacy =
    serde_json::json!({"kind":"launch","reference":{"identity":"old-policy","version":"v1","digest":"11".repeat(32)}})
      .to_string();
  vec![
    json_request("POST", &base, "publish-policy", None, &draft),
    empty_request(
      "GET",
      &format!("{base}/launch/{policy}/versions/1?digest={}", "11".repeat(32)),
      None,
    ),
    json_request("POST", &format!("{base}/validate"), "unused", None, &draft),
    empty_request("GET", &format!("{base}?limit=1"), None),
    empty_request("GET", &format!("{base}/defaults"), None),
    json_request(
      "POST",
      &format!("{base}/defaults"),
      "select-defaults",
      None,
      r#"{"expected_version":null,"launch_policy":null,"access_profile":null}"#,
    ),
    json_request("POST", &format!("{base}/legacy/resolve"), "unused", None, &legacy),
  ]
}
