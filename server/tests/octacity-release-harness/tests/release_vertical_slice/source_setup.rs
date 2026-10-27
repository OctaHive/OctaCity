//! Supported current multi-step enrollment over the packaged protected gateway.
//! Integration-setup qualification can reuse the environment and replace this journey.
use super::*;

pub(super) async fn enroll_writer(
  client: &Client,
  origin: &str,
  installed: &source_installation::Installation,
) -> String {
  enroll(client, origin, installed, true).await.0
}

/// Force the Source-only gate to cover a connection beyond the first public ID-ordered page.
/// These earlier metadata-only connections have no credentials, grants or consumers.
pub(super) async fn enroll_reader_with_history(
  client: &Client,
  origin: &str,
  installed: &source_installation::Installation,
) -> String {
  let (project, connection) = enroll(client, origin, installed, false).await;
  let selected = Uuid::parse_str(&connection).unwrap().as_u128();
  // The Console requests 25 connections per page; older IDs keep this target past it.
  for offset in 1..=26 {
    let id = Uuid::from_u128(selected - offset);
    publish(
      client,
      origin,
      &format!("/api/v1/integrations/{id}/versions"),
      &format!("{id}-metadata"),
      "if-none-match",
      "*",
      json!({"intent":{"id":id,"expected_current_version":null,"draft":{
        "name":format!("Earlier metadata {id}"),"installation":installed.installation,
        "endpoint":installed.endpoint,"authentication":"token",
        "resources":[{"reference":installed.resource["reference"],"purposes":["task_read"]}]
      }}}),
    )
    .await;
  }
  project
}

async fn enroll(
  client: &Client,
  origin: &str,
  installed: &source_installation::Installation,
  writer: bool,
) -> (String, String) {
  let connection = Uuid::new_v4().to_string();
  let key = |operation: &str| format!("{connection}-{operation}");
  let project = post_management(
    client,
    origin,
    "/api/v1/projects",
    &key("source-project"),
    json!({"name":format!("Packaged Source-only {connection}")}),
  )
  .await;
  let project_id = resource_id(&project);
  let connection = enroll_resource(client, origin, installed, &project_id, &installed.resource, writer).await;
  (project_id, connection)
}

/// Enroll a later Project/resource against the already running installed inventory.
pub(super) async fn enroll_resource(
  client: &Client,
  origin: &str,
  installed: &source_installation::Installation,
  project_id: &str,
  resource: &Value,
  writer: bool,
) -> String {
  let purposes = if writer {
    vec!["task_read", "task_update"]
  } else {
    vec!["task_read"]
  };
  let connection = Uuid::new_v4().to_string();
  let key = |operation: &str| format!("{connection}-{operation}");
  let path = format!("/api/v1/integrations/{connection}");
  publish(
    client,
    origin,
    &format!("{path}/versions"),
    &key("source-connection"),
    "if-none-match",
    "*",
    json!({"intent":{
      "id":connection,"expected_current_version":null,"draft":{"name":format!("Packaged tasks {project_id}"),
        "installation":installed.installation,"endpoint":installed.endpoint,"authentication":"token",
        "resources":[{"reference":resource["reference"],"purposes":purposes}]}
    }}),
  )
  .await;
  publish(client,origin,&format!("{path}/credentials"),&key("source-credential"),"if-none-match","*",json!({"intent":{
    "id":connection,"expected_metadata_version":1,"expected_revision":0,
    "action":{"operation":"credential","reference":resource["capabilities"][0]["credential"],"kind":"provision","activate_connection":false}},
    "value":source_evidence::READER
  })).await;
  if writer {
    publish(client, origin, &format!("{path}/credentials"), &key("writer-credential"), "if-match", "\"1\"", json!({"intent":{
      "id":connection,"expected_metadata_version":1,"expected_revision":1,
      "action":{"operation":"credential","reference":resource["capabilities"][1]["credential"],"kind":"provision","activate_connection":false}},
      "value":source_evidence::WRITER
    })).await;
  }
  let revision = if writer { 2 } else { 1 };
  post_management(
    client,
    origin,
    &format!("{path}/checks"),
    &key("source-read-check"),
    json!({"version":1,
    "resource":resource["reference"],"purpose":"task_read"}),
  )
  .await;
  publish(
    client,
    origin,
    &format!("{path}/state"),
    &key("source-activate"),
    "if-match",
    &format!("\"{revision}\""),
    json!({"intent":{
      "id":connection,"expected_metadata_version":1,"expected_revision":revision,"action":{"operation":"activate"}
    }}),
  )
  .await;
  publish(
    client,
    origin,
    &format!("{path}/projects/{project_id}/access"),
    &key("source-grant"),
    "if-none-match",
    "*",
    json!({"intent":{
      "connection_id":connection,"project_id":project_id,"connection_version":1,"expected_current_version":null,
      "resources":[{"reference":resource["reference"],"purposes":purposes}]
    }}),
  )
  .await;
  connection
}

pub(super) async fn publish(
  client: &Client,
  origin: &str,
  path: &str,
  key: &str,
  header: &str,
  version: &str,
  body: Value,
) -> Value {
  let response = client
    .post(format!("{origin}{path}"))
    .header("idempotency-key", key)
    .header(header, version)
    .json(&body)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let body: Value = response.json().await.unwrap();
  assert!(status.is_success(), "{path}: {status} {body}");
  body
}
