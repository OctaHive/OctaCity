//! Read and write Project grants are independently withdrawn through the same public protected API.
use super::*;

pub(super) async fn run(
  environment: &source_environment::SourceEnvironment,
  project: &str,
  connection: &str,
  configuration: &Value,
  repository: &Value,
) {
  let path = format!("/api/v1/integrations/{connection}/projects/{project}/access");
  let query = format!(
    "{}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1",
    environment.origin
  );
  for (previous, purposes) in [(1, vec!["task_read"]), (2, vec![])] {
    let resources = if purposes.is_empty() {
      vec![]
    } else {
      vec![json!({"reference":environment.installation.resource["reference"],"purposes":purposes})]
    };
    source_setup::publish(
      &environment.client,
      &environment.origin,
      &path,
      &format!("{connection}-revoke-{previous}"),
      "if-match",
      &format!("\"{previous}\""),
      json!({"intent":{"connection_id":connection,"project_id":project,
        "connection_version":1,"expected_current_version":previous,"resources":resources}}),
    )
    .await;
    let choices = get_json(&environment.client, query.clone()).await;
    let items = choices["items"].as_array().unwrap();
    if previous == 1 {
      assert_eq!(
        items.len(),
        1,
        "withdrawing the writer retains the independently granted reader"
      );
      assert_eq!(items[0]["purpose"], "task_read");
    } else {
      assert!(
        items.is_empty(),
        "withdrawing the reader leaves no Source intake authority"
      );
    }
  }
  let issue = post_management(
    &environment.client,
    &environment.origin,
    &format!("/api/v1/projects/{project}/issues"),
    &Uuid::new_v4().to_string(),
    json!({"title":"Manual task after Source revocation",
      "description":"Independent local intent","acceptance":null,"priority":null}),
  )
  .await;
  assert!(
    issue["resource"]["id"].is_string(),
    "manual drafting needs no Source or acceptance"
  );
  let issue_id = issue["resource"]["id"].as_str().unwrap();
  let mut content = issue["resource"]["content"].clone();
  content["acceptance"] = json!("Explicitly reviewed local acceptance.");
  let response = environment
    .client
    .patch(format!(
      "{}/api/v1/projects/{project}/issues/{issue_id}",
      environment.origin
    ))
    .header("idempotency-key", Uuid::new_v4().to_string())
    .header("if-match", "\"1\"")
    .json(&content)
    .send()
    .await
    .unwrap();
  assert!(response.status().is_success());
  let execution = source_setup::publish(
    &environment.client,
    &environment.origin,
    &format!("/api/v1/projects/{project}/issues/{issue_id}/executions"),
    &Uuid::new_v4().to_string(),
    "if-match",
    "\"2\"",
    json!({"classification":{"priority":40,"risk":"low","metadata":{}},
      "target":{"configuration_id":configuration["id"],"configuration_version":1,
        "repository_id":repository["id"],"repository_version":1,
        "source":{"kind":"exact_revision","value":"a".repeat(40)},"mapping":"title_description_acceptance_v1"}}),
  )
  .await;
  assert_eq!(
    execution["resource"]["state"], "admitted",
    "manual Factory admission remains independent of revoked Source grants"
  );
  assert_eq!(execution["resource"]["association"]["issue_id"], issue_id);
}
