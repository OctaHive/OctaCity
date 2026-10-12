//! Headless transport client: only HTTP, operation IDs and the published JSON contract.
use reqwest::{Client, StatusCode, header::HeaderMap};
use serde_json::Value;

pub struct OpenApiClient {
  origin: String,
  http: Client,
  pub document: Value,
}
pub struct Reply {
  pub body: Value,
  pub headers: HeaderMap,
  pub status: StatusCode,
}
impl OpenApiClient {
  pub async fn wait_for_node(&self, run: &str, node: &str) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
      loop {
        let reply = self
          .ok(
            "listFactoryDiagnostics",
            &[("factory_run_id", run), ("collection", "node-attempts")],
            None,
            None,
            None,
          )
          .await;
        if let Some(row) = reply.body["items"]
          .as_array()
          .unwrap()
          .iter()
          .find(|row| row["record"]["node_key"] == node)
        {
          return row["record"].clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
      }
    })
    .await
    .expect("configured node becomes observable over HTTP")
  }
  pub async fn connect(origin: String) -> Self {
    let http = Client::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
      loop {
        if http
          .get(format!("{origin}/health/ready"))
          .send()
          .await
          .unwrap()
          .status()
          .is_success()
        {
          break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
      }
    })
    .await
    .expect("released HTTP server becomes ready");
    let document = http
      .get(format!("{origin}/api/v1/openapi.json"))
      .send()
      .await
      .unwrap()
      .error_for_status()
      .unwrap()
      .json()
      .await
      .unwrap();
    Self { origin, http, document }
  }
  pub async fn call(
    &self,
    operation: &str,
    identities: &[(&str, &str)],
    body: Option<Value>,
    key: Option<&str>,
    precondition: Option<&str>,
  ) -> Reply {
    let (path, method, contract) = self.document["paths"]
      .as_object()
      .unwrap()
      .iter()
      .flat_map(|(path, methods)| {
        methods
          .as_object()
          .unwrap()
          .iter()
          .map(move |(method, contract)| (path, method, contract))
      })
      .find(|(_, _, contract)| contract["operationId"] == operation)
      .expect("published operation ID");
    let mut path = path.clone();
    for (name, value) in identities {
      path = path.replace(&format!("{{{name}}}"), value);
    }
    assert!(!path.contains('{'), "every published path parameter must be supplied");
    let mut request = self.http.request(
      method.to_ascii_uppercase().parse().unwrap(),
      format!("{}{path}", self.origin),
    );
    if let Some(key) = key {
      request = request.header("Idempotency-Key", key);
    }
    if let Some(precondition) = precondition {
      request = request.header("If-Match", precondition);
    }
    if let Some(body) = body {
      let reference = contract["requestBody"]["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap();
      crate::openapi_contract::assert_json_matches_component(
        &self.document,
        reference.rsplit('/').next().unwrap(),
        &body,
      );
      request = request.json(&body);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.bytes().await.unwrap();
    let body: Value = if bytes.is_empty() {
      Value::Null
    } else {
      serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("HTTP {status}: invalid JSON response: {error}"))
    };
    assert!(headers.contains_key("x-request-id"));
    if status.is_success() {
      let reference = contract["responses"][status.as_str()]["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap();
      crate::openapi_contract::assert_json_matches_component(
        &self.document,
        reference.rsplit('/').next().unwrap(),
        &body,
      );
    }
    Reply { body, headers, status }
  }
  pub async fn ok(
    &self,
    operation: &str,
    identities: &[(&str, &str)],
    body: Option<Value>,
    key: Option<&str>,
    precondition: Option<&str>,
  ) -> Reply {
    let reply = self.call(operation, identities, body, key, precondition).await;
    assert!(
      reply.status.is_success(),
      "{operation}: {}: {}",
      reply.status,
      reply.body
    );
    reply
  }
}
