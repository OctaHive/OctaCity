//! Separate durable static-service target for the released deployment pilot.
use axum::{
  Json, Router,
  extract::{DefaultBodyLimit, Path as RoutePath, State},
  http::StatusCode,
  routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_server_factory::{CandidateSubject, FactoryDigest, FactoryKey, ImmutableReference};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
  fs,
  io::Write as _,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
  time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DeploymentRequest {
  operation_id: FactoryDigest,
  candidate: CandidateSubject,
  content_sha256: FactoryDigest,
  content: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DeploymentRecord {
  deployment: ImmutableReference,
  request: DeploymentRequest,
}
impl DeploymentRecord {
  fn receipt(&self) -> Value {
    json!({"deployment":self.deployment,"candidate":self.request.candidate,"content_sha256":self.request.content_sha256})
  }
}
type TargetState = Arc<Mutex<PathBuf>>;

pub struct DeploymentTarget {
  endpoint: String,
  stop: Option<oneshot::Sender<()>>,
  task: Option<JoinHandle<()>>,
}
impl DeploymentTarget {
  pub async fn start(directory: &Path) -> Self {
    let state = Arc::new(Mutex::new(directory.canonicalize().unwrap().join("active.json")));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/deployment", listener.local_addr().unwrap());
    let router = Router::new()
      .route("/deployment", post(publish))
      .route("/deployment/{identity}", get(observe))
      .route("/live", get(live))
      .layer(DefaultBodyLimit::max(96 * 1024))
      .with_state(state);
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
      axum::serve(listener, router)
        .with_graceful_shutdown(async {
          let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    Self {
      endpoint,
      stop: Some(stop),
      task: Some(task),
    }
  }
  pub fn endpoint(&self) -> &str {
    &self.endpoint
  }
  pub fn live_endpoint(&self) -> String {
    self.endpoint.replace("/deployment", "/live")
  }
  pub async fn stop(mut self) {
    let _ = self.stop.take().unwrap().send(());
    let mut task = self.task.take().unwrap();
    match tokio::time::timeout(Duration::from_secs(5), &mut task).await {
      Ok(result) => result.unwrap(),
      Err(_) => {
        task.abort();
        let _ = task.await;
        panic!("owned deployment target exceeded its shutdown bound");
      }
    }
  }
}
impl Drop for DeploymentTarget {
  fn drop(&mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
    if let Some(task) = self.task.take() {
      task.abort();
    }
  }
}

fn read_record(file: &Path) -> Result<DeploymentRecord, StatusCode> {
  let bytes = fs::read(file).map_err(|error| {
    if error.kind() == std::io::ErrorKind::NotFound {
      StatusCode::NOT_FOUND
    } else {
      StatusCode::INTERNAL_SERVER_ERROR
    }
  })?;
  serde_json::from_slice(&bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
async fn publish(
  State(state): State<TargetState>,
  Json(request): Json<DeploymentRequest>,
) -> Result<Json<Value>, StatusCode> {
  let content = STANDARD.decode(&request.content).map_err(|_| StatusCode::BAD_REQUEST)?;
  if content.len() > 65536 || FactoryDigest::content_sha256(&content) != request.content_sha256 {
    return Err(StatusCode::BAD_REQUEST);
  }
  let file = state.lock().unwrap();
  match read_record(&file) {
    Ok(record) => {
      if record.request != request {
        return Err(StatusCode::CONFLICT);
      }
      Ok(Json(record.receipt()))
    }
    Err(StatusCode::NOT_FOUND) => {
      let bytes = serde_json::to_vec(&request).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      let record = DeploymentRecord {
        deployment: ImmutableReference::new(
          FactoryKey::new(request.operation_id.to_string()).unwrap(),
          FactoryKey::new("v1").unwrap(),
          FactoryDigest::content_sha256(&bytes),
        ),
        request,
      };
      let parent = file.parent().unwrap();
      let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      serde_json::to_writer(&mut temporary, &record).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      temporary.flush().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      temporary
        .as_file()
        .sync_all()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      temporary
        .persist(&*file)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
      Ok(Json(record.receipt()))
    }
    Err(error) => Err(error),
  }
}
async fn observe(
  State(state): State<TargetState>,
  RoutePath(identity): RoutePath<String>,
) -> Result<Json<Value>, StatusCode> {
  let record = read_record(&state.lock().unwrap())?;
  if record.deployment.identity().as_str() != identity {
    return Err(StatusCode::NOT_FOUND);
  }
  let mut receipt = record.receipt();
  receipt["content"] = json!(record.request.content);
  Ok(Json(receipt))
}
async fn live(State(state): State<TargetState>) -> Result<Vec<u8>, StatusCode> {
  let record = read_record(&state.lock().unwrap())?;
  STANDARD
    .decode(record.request.content)
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
