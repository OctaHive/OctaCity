//! Failure evidence belongs to the private scenario directory, never the public panic.
use super::*;
use std::{io::Write as _, os::unix::fs::OpenOptionsExt as _};

const PAGE_LIMIT: usize = 100;
// Match the manual journey's single-output ceiling when buffering HTTP response bodies.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

pub(super) async fn assert_no_failed_builds(client: &Client, origin: &str, project: &str, directory: &Path) {
  let builds = get_json(client, format!("{origin}/api/v1/projects/{project}/builds?limit=100")).await;
  for build in builds["items"].as_array().unwrap() {
    if build["state"] == "failed"
      && let Err(error) = retain(client, origin, build, directory).await
    {
      eprintln!(
        "failure diagnostic collection incomplete ({error}); private evidence: {}",
        directory.display()
      );
    }
    assert_ne!(
      build["state"],
      "failed",
      "ordinary Build failed: {build}; private evidence: {}",
      directory.display()
    );
  }
}

async fn retain(client: &Client, origin: &str, build: &Value, directory: &Path) -> Result<(), &'static str> {
  let build_id = identity(&build["id"])?;
  let attempt_id = identity(&build["current_attempt_id"])?;
  let directory = directory.join(format!("failed-build-{build_id}"));
  octacity_private_fs::create_private_directory(&directory).map_err(|_| "private_directory_failed")?;
  save(&directory, "build.json", &serde_json::to_vec_pretty(build).unwrap())?;
  let collected = collect(client, origin, build_id, attempt_id, &directory).await;
  if let Err(error) = collected {
    let _ = save(
      &directory,
      "collection-error.json",
      &serde_json::to_vec(&json!({"error":error})).unwrap(),
    );
  }
  collected
}

async fn collect(
  client: &Client,
  origin: &str,
  build: Uuid,
  attempt: Uuid,
  directory: &Path,
) -> Result<(), &'static str> {
  let mut errors = BTreeMap::new();
  if let Err(error) = attempt_events(client, origin, attempt, directory).await {
    errors.insert("job_events", error);
  }
  match read_json(client.get(format!("{origin}/api/v1/builds/{build}/artifacts?limit=100"))).await {
    Ok(outputs) => {
      save(
        directory,
        "artifacts.json",
        &serde_json::to_vec_pretty(&outputs).unwrap(),
      )?;
      for (name, file) in [
        ("codex-run-result", "codex-run-result.json"),
        ("codex-run-trace", "codex-run-trace.ndjson"),
      ] {
        let retained = async {
          let output = outputs["items"]
            .as_array()
            .ok_or("invalid_artifact_items")?
            .iter()
            .find(|output| output["name"] == name)
            .ok_or("artifact_not_published")?;
          let id = output["id"].as_str().ok_or("invalid_artifact_id")?;
          let download = read_json(client.post(format!("{origin}/api/v1/artifacts/{id}/download"))).await?;
          let url = download["get_url"].as_str().ok_or("invalid_artifact_download")?;
          let bytes = read_bytes(client.get(url)).await?;
          save(directory, file, &bytes)
        }
        .await;
        if let Err(error) = retained {
          errors.insert(name, error);
        }
      }
    }
    Err(error) => {
      errors.insert("artifacts", error);
    }
  }
  if !errors.is_empty() {
    save(
      directory,
      "collection-error.json",
      &serde_json::to_vec_pretty(&errors).unwrap(),
    )?;
  }
  Ok(())
}

async fn attempt_events(client: &Client, origin: &str, attempt: Uuid, directory: &Path) -> Result<(), &'static str> {
  let attempt = read_json(client.get(format!("{origin}/api/v1/attempts/{attempt}"))).await?;
  save(directory, "attempt.json", &serde_json::to_vec_pretty(&attempt).unwrap())?;
  for job in attempt["jobs"].as_array().ok_or("invalid_attempt_jobs")? {
    let id = identity(&job["id"])?;
    let mut after = 0;
    let mut events = Vec::new();
    loop {
      let page = read_json(client.get(format!(
        "{origin}/api/v1/jobs/{id}/events?after={after}&limit={PAGE_LIMIT}&wait_ms=0"
      )))
      .await?;
      let items = page["items"].as_array().ok_or("invalid_event_items")?;
      let cursor = page["cursor"].as_u64().ok_or("invalid_event_cursor")?;
      events.extend(items.iter().cloned());
      if items.len() < PAGE_LIMIT {
        save(
          directory,
          &format!("job-{id}-events.json"),
          &serde_json::to_vec_pretty(&json!({"items":events,"cursor":cursor})).unwrap(),
        )?;
        break;
      }
      if cursor <= after {
        return Err("nonadvancing_event_cursor");
      }
      after = cursor;
    }
  }
  Ok(())
}

fn identity(value: &Value) -> Result<Uuid, &'static str> {
  value
    .as_str()
    .and_then(|s| Uuid::parse_str(s).ok())
    .ok_or("invalid_evidence_identity")
}

fn save(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), &'static str> {
  fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .open(directory.join(name))
    .and_then(|mut file| file.write_all(bytes))
    .map_err(|_| "private_file_failed")
}

async fn read_json(request: reqwest::RequestBuilder) -> Result<Value, &'static str> {
  serde_json::from_slice(&read_bytes(request).await?).map_err(|_| "invalid_diagnostic_json")
}

async fn read_bytes(request: reqwest::RequestBuilder) -> Result<Vec<u8>, &'static str> {
  // Static errors must not retain signed download URLs or dump private response bodies into CI logs.
  let mut response = request
    .timeout(Duration::from_secs(5))
    .send()
    .await
    .map_err(|_| "diagnostic_request_failed")?
    .error_for_status()
    .map_err(|_| "diagnostic_http_failed")?;
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|_| "diagnostic_body_failed")? {
    if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
      return Err("diagnostic_response_limit");
    }
    bytes.extend_from_slice(&chunk);
  }
  Ok(bytes)
}
