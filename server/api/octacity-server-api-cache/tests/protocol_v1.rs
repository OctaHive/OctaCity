use std::{
  collections::{BTreeSet, HashMap},
  sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode, header},
};
use octacity_server_api_cache::{CacheDataPlaneApplication, cache_router};
use octacity_server_application::{
  ActionResultV1, BlobDescriptor, BlobEncoding, CacheDataPlaneError, CacheDataPlaneUseCases, CacheRequestAuthority,
  CacheWriteResult, Digest, FindMissingBlobsRequestV1, FindMissingBlobsResponseV1, REMOTE_CACHE_BLOB_CONTENT_TYPE,
  REMOTE_CACHE_JSON_CONTENT_TYPE, REMOTE_CACHE_PROTOCOL_HEADER, REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1,
  WriteActionRequestV1,
};
use tower::ServiceExt as _;

#[derive(Default)]
struct MemoryApplication {
  blobs: Mutex<HashMap<(String, BlobDescriptor), Vec<u8>>>,
  actions: Mutex<HashMap<(String, Digest), ActionResultV1>>,
}

impl MemoryApplication {
  fn scope(authority: &CacheRequestAuthority) -> Result<String, CacheDataPlaneError> {
    match authority.bearer_token.as_str() {
      "token-a" => Ok("namespace-a".to_owned()),
      "token-b" => Ok("namespace-b".to_owned()),
      _ => Err(CacheDataPlaneError::AuthorizationRejected),
    }
  }
}

#[async_trait]
impl CacheDataPlaneUseCases for MemoryApplication {
  async fn find_missing(
    &self,
    authority: CacheRequestAuthority,
    request: FindMissingBlobsRequestV1,
  ) -> Result<FindMissingBlobsResponseV1, CacheDataPlaneError> {
    let scope = Self::scope(&authority)?;
    let blobs = self.blobs.lock().unwrap();
    Ok(FindMissingBlobsResponseV1 {
      protocol_version: 1,
      missing: request
        .blobs
        .into_iter()
        .filter(|blob| !blobs.contains_key(&(scope.clone(), blob.clone())))
        .collect(),
    })
  }

  async fn read_blob(
    &self,
    authority: CacheRequestAuthority,
    blob: BlobDescriptor,
  ) -> Result<Option<Vec<u8>>, CacheDataPlaneError> {
    Ok(
      self
        .blobs
        .lock()
        .unwrap()
        .get(&(Self::scope(&authority)?, blob))
        .cloned(),
    )
  }

  async fn write_blob(
    &self,
    authority: CacheRequestAuthority,
    blob: BlobDescriptor,
    bytes: Vec<u8>,
  ) -> Result<CacheWriteResult, CacheDataPlaneError> {
    let key = (Self::scope(&authority)?, blob);
    let mut blobs = self.blobs.lock().unwrap();
    match blobs.get(&key) {
      Some(existing) if existing == &bytes => Ok(CacheWriteResult::AlreadyPresent),
      Some(_) => Err(CacheDataPlaneError::Conflict),
      None => {
        blobs.insert(key, bytes);
        Ok(CacheWriteResult::Written)
      }
    }
  }

  async fn read_action(
    &self,
    authority: CacheRequestAuthority,
    action: Digest,
  ) -> Result<Option<ActionResultV1>, CacheDataPlaneError> {
    let scope = Self::scope(&authority)?;
    if authority.namespace.as_deref() != Some(scope.as_str()) {
      return Err(CacheDataPlaneError::AuthorizationRejected);
    }
    Ok(self.actions.lock().unwrap().get(&(scope, action)).cloned())
  }

  async fn write_action(
    &self,
    authority: CacheRequestAuthority,
    action: Digest,
    request: WriteActionRequestV1,
  ) -> Result<CacheWriteResult, CacheDataPlaneError> {
    let scope = Self::scope(&authority)?;
    if authority.namespace.as_deref() != Some(scope.as_str()) || request.namespace != scope {
      return Err(CacheDataPlaneError::AuthorizationRejected);
    }
    if let Some(blob) = &request.result.output_bundle
      && !self.blobs.lock().unwrap().contains_key(&(scope.clone(), blob.clone()))
    {
      return Err(CacheDataPlaneError::Conflict);
    }
    let key = (scope, action);
    let mut actions = self.actions.lock().unwrap();
    match actions.get(&key) {
      Some(existing) if existing == &request.result => Ok(CacheWriteResult::AlreadyPresent),
      Some(_) => Err(CacheDataPlaneError::Conflict),
      None => {
        actions.insert(key, request.result);
        Ok(CacheWriteResult::Written)
      }
    }
  }
}

fn request(method: &str, uri: &str, token: &str, content_type: Option<&str>, body: Vec<u8>) -> Request<Body> {
  let mut builder = Request::builder()
    .method(method)
    .uri(uri)
    .header(REMOTE_CACHE_PROTOCOL_HEADER, REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1)
    .header(header::AUTHORIZATION, format!("Bearer {token}"));
  if let Some(content_type) = content_type {
    builder = builder
      .header(header::CONTENT_TYPE, content_type)
      .header(header::IF_NONE_MATCH, "*");
  }
  builder.body(Body::from(body)).unwrap()
}

fn blob() -> (BlobDescriptor, Vec<u8>, String) {
  let bytes = b"verified canonical bundle".to_vec();
  let digest = Digest::blake3(&bytes);
  let descriptor = BlobDescriptor {
    digest,
    encoding: BlobEncoding::Identity,
    encoded_size_bytes: bytes.len() as u64,
    expanded_size_bytes: bytes.len() as u64,
    entry_count: 1,
  };
  let path = format!(
    "/v1/blobs/blake3/{}/{}/identity/{}/1",
    digest.hex(),
    bytes.len(),
    bytes.len()
  );
  (descriptor, bytes, path)
}

#[test]
fn published_octa_metadata_fixtures_remain_accepted() {
  let missing_request: FindMissingBlobsRequestV1 = serde_json::from_str(include_str!(
    "../../../../../octa/crates/octa-cache-protocol/fixtures/find-missing-blobs-request-v1.json"
  ))
  .unwrap();
  missing_request.validate().unwrap();
  let missing_response: FindMissingBlobsResponseV1 = serde_json::from_str(include_str!(
    "../../../../../octa/crates/octa-cache-protocol/fixtures/find-missing-blobs-response-v1.json"
  ))
  .unwrap();
  missing_response.validate().unwrap();
  let action: WriteActionRequestV1 = serde_json::from_str(include_str!(
    "../../../../../octa/crates/octa-cache-protocol/fixtures/write-action-request-v1.json"
  ))
  .unwrap();
  action.validate().unwrap();
}

#[tokio::test]
async fn complete_protocol_workflow_is_atomic_and_namespace_isolated() {
  let application = Arc::new(MemoryApplication::default());
  let router = cache_router(CacheDataPlaneApplication::new(application), 1024 * 1024);
  let (blob, bytes, blob_path) = blob();
  let missing = FindMissingBlobsRequestV1 {
    protocol_version: 1,
    blobs: vec![blob.clone()],
  };

  let response = router
    .clone()
    .oneshot(request(
      "POST",
      "/v1/blobs/missing",
      "token-a",
      Some(REMOTE_CACHE_JSON_CONTENT_TYPE),
      serde_json::to_vec(&missing).unwrap(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response.headers().get(REMOTE_CACHE_PROTOCOL_HEADER).unwrap(),
    REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1
  );
  let found: FindMissingBlobsResponseV1 =
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap();
  assert_eq!(found.missing, vec![blob.clone()]);

  let response = router
    .clone()
    .oneshot(request(
      "PUT",
      &blob_path,
      "token-a",
      Some(REMOTE_CACHE_BLOB_CONTENT_TYPE),
      bytes.clone(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);

  let action = Digest::blake3(b"opaque action descriptor");
  let result = ActionResultV1 {
    result_version: 1,
    action,
    output_bundle: Some(blob.clone()),
    stdout: Some("ok".to_owned()),
    task_outputs: Default::default(),
    artifacts: Vec::new(),
    reports: Vec::new(),
  };
  let action_path = format!(
    "/v1/actions/blake3/{}/{}?namespace=namespace-a",
    action.hex(),
    action.size_bytes()
  );
  let publication = WriteActionRequestV1 {
    protocol_version: 1,
    namespace: "namespace-a".to_owned(),
    result: result.clone(),
  };
  let response = router
    .clone()
    .oneshot(request(
      "PUT",
      &action_path,
      "token-a",
      Some(REMOTE_CACHE_JSON_CONTENT_TYPE),
      serde_json::to_vec(&publication).unwrap(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);

  let response = router
    .clone()
    .oneshot(request("GET", &action_path, "token-a", None, Vec::new()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let restored: ActionResultV1 =
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap();
  assert_eq!(restored, result);

  let other_action_path = action_path.replace("namespace-a", "namespace-b");
  let response = router
    .clone()
    .oneshot(request("GET", &other_action_path, "token-b", None, Vec::new()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  let response = router
    .oneshot(request("GET", &blob_path, "token-b", None, Vec::new()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_protocol_and_unknown_bearers_disclose_no_cache_state() {
  let router = cache_router(
    CacheDataPlaneApplication::new(Arc::new(MemoryApplication::default())),
    1024,
  );
  let (_, _, blob_path) = blob();
  let response = router
    .clone()
    .oneshot(Request::builder().uri(&blob_path).body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(
    response.headers().get(REMOTE_CACHE_PROTOCOL_HEADER).unwrap(),
    REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1
  );
  let response = router
    .oneshot(request("GET", &blob_path, "unknown", None, Vec::new()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  assert!(to_bytes(response.into_body(), 1024).await.unwrap().is_empty());
}

#[test]
fn fixture_missing_sets_are_well_formed() {
  let response: FindMissingBlobsResponseV1 = serde_json::from_str(include_str!(
    "../../../../../octa/crates/octa-cache-protocol/fixtures/find-missing-blobs-response-v1.json"
  ))
  .unwrap();
  assert_eq!(
    response.missing.iter().collect::<BTreeSet<_>>().len(),
    response.missing.len()
  );
}
