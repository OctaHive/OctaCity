//! Assertions and read-side probes for the Linux Native release scenario.

use std::{path::Path, time::Duration};

use chrono::Utc;
use reqwest::Client;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio::{
  process::Child,
  time::{Instant, sleep},
};
use url::Url;

use super::{ARTIFACT_BYTES, BuildRun, MEMORY_BYTES, get_json, string, wait_for_terminal_build};

pub(super) async fn wait_for_successful_run(
  client: &Client,
  origin: &str,
  build_id: &str,
  agent: &mut Child,
  stderr: &Path,
) -> BuildRun {
  let build = wait_for_terminal_build(client, origin, build_id, agent, stderr).await;
  assert_eq!(build["state"], "succeeded", "Build did not succeed: {build}");
  let attempt = get_json(
    client,
    format!(
      "{origin}/api/v1/attempts/{}",
      build["current_attempt"]["id"].as_str().unwrap()
    ),
  )
  .await;
  let mut jobs = Vec::new();
  for job in attempt["jobs"].as_array().unwrap() {
    let detail = get_json(client, format!("{origin}/api/v1/jobs/{}", string(job, "id"))).await;
    let events = complete_job_events(client, origin, &string(job, "id")).await;
    jobs.push(json!({"detail": detail, "events": events}));
  }
  BuildRun { build, attempt, jobs }
}

async fn complete_job_events(client: &Client, origin: &str, job_id: &str) -> Value {
  const PAGE_LIMIT: usize = 100;
  let mut after = 0_u64;
  let mut items = Vec::new();
  loop {
    let page = get_json(
      client,
      format!("{origin}/api/v1/jobs/{job_id}/events?after={after}&limit={PAGE_LIMIT}&wait_ms=0"),
    )
    .await;
    let page_items = page["items"].as_array().expect("Job event items must be an array");
    let cursor = page["cursor"].as_u64().expect("Job event cursor must be an integer");
    assert!(cursor >= after, "Job event cursor moved backwards");
    items.extend(page_items.iter().cloned());
    if page_items.len() < PAGE_LIMIT {
      return json!({"items": items, "cursor": cursor});
    }
    assert!(cursor > after, "full Job event page did not advance its cursor");
    after = cursor;
  }
}

pub(super) fn assert_dag_and_events(run: &BuildRun, maximum_disk_bytes: Option<u64>) {
  assert_eq!(run.attempt["jobs"].as_array().unwrap().len(), 2);
  assert_eq!(run.attempt["edges"].as_array().unwrap().len(), 1);
  assert_successful_job_events(run);
  if let Some(maximum_disk_bytes) = maximum_disk_bytes {
    let events = run
      .jobs
      .iter()
      .flat_map(|job| job["events"]["items"].as_array().unwrap())
      .cloned()
      .collect::<Vec<_>>();
    assert_resource_samples(&events, maximum_disk_bytes);
  }
}

pub(super) fn assert_resource_samples(events: &[Value], maximum_disk_bytes: u64) {
  let samples = events
    .iter()
    .filter(|event| event["payload"]["source"] == "agent" && event["payload"]["event"]["type"] == "resource_usage");
  let mut count = 0;
  for sample in samples {
    count += 1;
    let usage = &sample["payload"]["event"]["usage"];
    assert!(usage["memory_peak_bytes"].as_u64().unwrap() <= MEMORY_BYTES);
    assert!(usage["disk_peak_bytes"].as_u64().unwrap() <= maximum_disk_bytes);
  }
  assert!(count > 0, "Native execution must publish resource accounting");
}

pub(super) fn assert_metrics_exercised(metrics: &str, names: &[&str]) {
  for name in names {
    let counter = format!("{name}_total");
    let recorded = metrics.lines().filter(|line| !line.starts_with('#')).any(|line| {
      let mut fields = line.split_whitespace();
      let Some(series) = fields.next() else { return false };
      let metric = series.split_once('{').map_or(series, |(metric, _)| metric);
      (metric == *name || metric == counter)
        && fields
          .next()
          .and_then(|value| value.parse::<f64>().ok())
          .is_some_and(|value| value > 0.0)
    });
    assert!(recorded, "metric {name} did not record an exercised operation");
  }
}

pub(super) fn assert_successful_job_events(run: &BuildRun) {
  for job in &run.jobs {
    assert_eq!(job["detail"]["terminal"]["state"], "succeeded");
    assert_lifecycle_order(job["events"]["items"].as_array().unwrap());
  }
}

pub(super) fn assert_lifecycle_order(events: &[Value]) {
  for (index, event) in events.iter().enumerate() {
    assert_eq!(
      event["sequence"].as_u64(),
      Some(index as u64 + 1),
      "event sequence has a gap"
    );
  }
  let states = events
    .iter()
    .filter_map(|event| {
      (event["payload"]["source"] == "agent" && event["payload"]["event"]["type"] == "state_changed")
        .then(|| event["payload"]["event"]["state"].as_str())
        .flatten()
    })
    .collect::<Vec<_>>();
  let cleaning = states
    .iter()
    .position(|state| *state == "cleaning")
    .expect("missing cleaning event");
  let completing = states
    .iter()
    .position(|state| *state == "completing")
    .expect("missing completing event");
  assert!(cleaning < completing, "cleanup must precede completion");
}

pub(super) fn has_cache_hit(job: &Value) -> bool {
  job["events"]["items"]
    .as_array()
    .unwrap()
    .iter()
    .any(|event| event["payload"]["source"] == "runner" && event["payload"]["event"]["data"]["type"] == "cache_hit")
}

pub(super) fn cache_events(run: &BuildRun) -> Vec<Value> {
  run
    .jobs
    .iter()
    .flat_map(|job| job["events"]["items"].as_array().unwrap())
    .filter(|event| {
      event["payload"]["source"] == "runner"
        && event["payload"]["event"]["data"]["type"]
          .as_str()
          .is_some_and(|event_type| event_type.starts_with("cache_"))
    })
    .cloned()
    .collect()
}

pub(super) fn assert_internal_causality(manual: &Value, upstream: &Value, downstream: &Value) {
  assert_eq!(downstream["immutable_revision"], upstream["immutable_revision"]);
  assert_eq!(downstream["trigger"]["kind"], "internal");
  assert_eq!(downstream["trigger"]["cause"]["source_build_id"], upstream["id"]);
  assert_eq!(
    downstream["trigger"]["causality"]["root_occurrence_id"],
    manual["trigger_occurrence_id"]
  );
  assert_eq!(
    downstream["trigger"]["causality"]["parent_occurrence_id"],
    manual["trigger_occurrence_id"]
  );
  assert_eq!(downstream["trigger"]["causality"]["depth"], 1);
}

pub(super) async fn verify_artifacts(client: &Client, origin: &str, build: &Value) -> (Value, f64) {
  let page = get_json(
    client,
    format!("{origin}/api/v1/builds/{}/artifacts?limit=10", string(build, "id")),
  )
  .await;
  let items = page["items"].as_array().unwrap();
  assert_eq!(items.len(), 2, "Build Result must expose one artifact and one report");
  assert!(items.iter().any(|item| item["output_type"]["kind"] == "artifact"));
  assert!(
    items
      .iter()
      .any(|item| item["output_type"]["kind"] == "report" && item["output_type"]["format"] == "junit")
  );
  let artifact = items.iter().find(|item| item["name"] == "release-result").unwrap();
  let download = client
    .post(format!("{origin}/api/v1/artifacts/{}/download", string(artifact, "id")))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json::<Value>()
    .await
    .unwrap();
  let download_started = Instant::now();
  let bytes = client
    .get(string(&download, "get_url"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .bytes()
    .await
    .unwrap();
  let elapsed = download_started.elapsed().as_secs_f64().max(f64::EPSILON);
  assert_release_artifact_payload(&bytes);
  assert_eq!(download["artifact"]["sha256"], hex::encode(Sha256::digest(&bytes)));
  (
    json!({"page": page, "downloaded_sha256": download["artifact"]["sha256"]}),
    bytes.len() as f64 / elapsed,
  )
}

pub(super) async fn verify_codex_outputs(client: &Client, origin: &str, build: &Value) -> Value {
  let page = get_json(
    client,
    format!("{origin}/api/v1/builds/{}/artifacts?limit=10", string(build, "id")),
  )
  .await;
  let items = page["items"].as_array().unwrap();
  assert_eq!(items.len(), 3, "Codex must publish two artifacts and one report");
  let trace = items.iter().find(|item| item["name"] == "codex-run-trace").unwrap();
  assert_eq!(trace["output_type"]["kind"], "artifact");
  assert_eq!(trace["media_type"], "application/x-ndjson");
  let provenance = items
    .iter()
    .find(|item| item["name"] == "codex-run-provenance")
    .unwrap();
  assert_eq!(provenance["output_type"]["kind"], "artifact");
  let result = items.iter().find(|item| item["name"] == "codex-run-result").unwrap();
  assert_eq!(result["output_type"]["kind"], "report");
  assert_eq!(result["output_type"]["format"], "octa.codex.result.v1");

  let trace_bytes = download_output(client, origin, trace).await;
  let trace_text = std::str::from_utf8(&trace_bytes).unwrap();
  assert!(
    !trace_text.contains("OCTACITY_CODEX_PROMPT_MUST_NOT_APPEAR_IN_TRACE"),
    "sanitized trace retained prompt content"
  );
  assert!(
    !trace_text.contains("release-secret-canary-must-not-appear"),
    "sanitized trace retained an explicitly mapped credential"
  );
  assert!(
    trace_text.contains("*****"),
    "sanitized trace did not retain a redaction marker"
  );
  for line in trace_text.lines() {
    let record: Value = serde_json::from_str(line).unwrap();
    assert_eq!(record["format_version"], 1);
  }

  let result_bytes = download_output(client, origin, result).await;
  let result_document: Value = serde_json::from_slice(&result_bytes).unwrap();
  assert_eq!(result_document["format_version"], 1);
  assert_eq!(result_document["outcome"], "completed");
  assert_eq!(
    result_document["structured_result"],
    json!({"outcome": "completed", "files": 1})
  );
  assert!(
    !serde_json::to_string(&result_document)
      .unwrap()
      .contains("release-secret-canary-must-not-appear")
  );
  let provenance_bytes = download_output(client, origin, provenance).await;
  let provenance_document: Value = serde_json::from_slice(&provenance_bytes).unwrap();
  assert_eq!(provenance_document["format_version"], 1);
  let retained = format!(
    "{trace_text}\n{}\n{}",
    serde_json::to_string(&result_document).unwrap(),
    serde_json::to_string(&provenance_document).unwrap()
  );
  assert_no_codex_sensitive_text(&retained, "Codex published output");
  json!({"page": page, "trace": trace_text, "result": result_document, "provenance": provenance_document})
}

pub(super) async fn verify_no_outputs(client: &Client, origin: &str, build: &Value) -> Value {
  let page = get_json(
    client,
    format!("{origin}/api/v1/builds/{}/artifacts?limit=10", string(build, "id")),
  )
  .await;
  assert_eq!(
    page["items"].as_array().map(Vec::len),
    Some(0),
    "failed or cancelled Codex execution published partial output"
  );
  page
}

pub(super) async fn verify_failed_codex_events(
  client: &Client,
  origin: &str,
  accepted: &Value,
  build: &Value,
) -> Value {
  assert_eq!(build["state"], "failed");
  let attempt = get_json(
    client,
    format!("{origin}/api/v1/attempts/{}", string(accepted, "attempt_id")),
  )
  .await;
  let jobs = attempt["jobs"].as_array().expect("Attempt jobs must be an array");
  assert_eq!(jobs.len(), 1);
  assert_eq!(jobs[0]["state"], "failed");
  let events = complete_job_events(client, origin, &string(&jobs[0], "id")).await;
  let evidence = json!({"attempt": attempt, "events": events});
  assert_no_codex_sensitive_text(&evidence.to_string(), "failed Codex events");
  evidence
}

pub(super) fn assert_run_has_no_codex_sensitive_text(run: &BuildRun) {
  assert_no_codex_sensitive_text(
    &json!({"build": run.build, "attempt": run.attempt, "jobs": run.jobs}).to_string(),
    "successful Codex REST evidence",
  );
}

fn assert_no_codex_sensitive_text(value: &str, description: &str) {
  for forbidden in [
    "release-secret-canary-must-not-appear",
    "OCTACITY_CODEX_PROMPT_MUST_NOT_APPEAR_IN_TRACE",
  ] {
    assert!(!value.contains(forbidden), "{description} retained sensitive input");
  }
}

async fn download_output(client: &Client, origin: &str, output: &Value) -> Vec<u8> {
  let download = client
    .post(format!("{origin}/api/v1/artifacts/{}/download", string(output, "id")))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .json::<Value>()
    .await
    .unwrap();
  client
    .get(string(&download, "get_url"))
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap()
    .bytes()
    .await
    .unwrap()
    .to_vec()
}

pub(super) fn assert_release_artifact_payload(bytes: &[u8]) {
  assert_eq!(bytes.len(), ARTIFACT_BYTES);
  assert!(
    bytes.iter().any(|byte| *byte != 0),
    "release artifact must contain the generated performance payload"
  );
}

pub(super) async fn verify_log_search(
  client: &Client,
  origin: &str,
  project_id: &str,
  run: &BuildRun,
) -> (Value, Vec<Duration>, Vec<Duration>) {
  let source_timestamp = run
    .jobs
    .iter()
    .flat_map(|job| job["events"]["items"].as_array().unwrap())
    .filter_map(|event| event["occurred_at_unix_ms"].as_i64())
    .max()
    .expect("successful Build must retain timestamped events");
  let (full_text, full_text_lag, full_text_query) = wait_for_search(
    client,
    origin,
    project_id,
    &string(&run.build, "id"),
    "release cache",
    "full_text",
    source_timestamp,
  )
  .await;
  let (literal, literal_lag, literal_query) = wait_for_search(
    client,
    origin,
    project_id,
    &string(&run.build, "id"),
    "dist/message.txt",
    "literal",
    source_timestamp,
  )
  .await;
  (
    json!({"full_text": full_text, "literal": literal}),
    vec![full_text_lag, literal_lag],
    vec![full_text_query, literal_query],
  )
}

async fn wait_for_search(
  client: &Client,
  origin: &str,
  project_id: &str,
  build_id: &str,
  query: &str,
  mode: &str,
  source_timestamp: i64,
) -> (Value, Duration, Duration) {
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    let mut url = Url::parse(&format!("{origin}/api/v1/projects/{project_id}/build-logs/search")).unwrap();
    url
      .query_pairs_mut()
      .append_pair("query", query)
      .append_pair("mode", mode)
      .append_pair("build_id", build_id)
      .append_pair("limit", "10");
    let query_started = Instant::now();
    let page = get_json(client, url.into()).await;
    let query_elapsed = query_started.elapsed();
    if page["freshness"]["caught_up"] == true && !page["items"].as_array().unwrap().is_empty() {
      let lag_milliseconds = Utc::now().timestamp_millis().saturating_sub(source_timestamp);
      return (
        page,
        Duration::from_millis(u64::try_from(lag_milliseconds).unwrap()),
        query_elapsed,
      );
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for {mode} log search: {page}"
    );
    sleep(Duration::from_millis(500)).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cache_diagnostics_keep_only_runner_cache_events() {
    let run = BuildRun {
      build: Value::Null,
      attempt: Value::Null,
      jobs: vec![json!({
        "events": {"items": [
          {"payload": {"source": "runner", "event": {"data": {"type": "cache_error"}}}},
          {"payload": {"source": "runner", "event": {"data": {"type": "task_finished"}}}},
          {"payload": {"source": "agent", "event": {"data": {"type": "cache_hit"}}}}
        ]}
      })],
    };

    let events = cache_events(&run);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["payload"]["event"]["data"]["type"], "cache_error");
  }
}

#[test]
fn metric_assertion_requires_a_positive_exact_series() {
  let metrics = "# HELP octacity_server_http_requests request count\n\
                 octacity_server_http_requests_total{route=\"ready\"} 0\n\
                 octacity_server_http_requests_total_extra 7\n";
  let rejected = std::panic::catch_unwind(|| {
    assert_metrics_exercised(metrics, &["octacity_server_http_requests"]);
  });
  assert!(rejected.is_err());

  assert_metrics_exercised(
    "octacity_server_http_requests_total{route=\"ready\"} 2\n",
    &["octacity_server_http_requests"],
  );
}
