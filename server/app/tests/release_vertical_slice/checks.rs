//! Assertions and read-side probes for the Linux Native release scenario.

use std::{path::Path, time::Duration};

use reqwest::Client;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio::{
  process::Child,
  time::{Instant, sleep},
};
use url::Url;

use super::{ARTIFACT_CONTENT, BuildRun, MEMORY_BYTES, get_json, string, wait_for_terminal_build};

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
    let events = get_json(
      client,
      format!(
        "{origin}/api/v1/jobs/{}/events?after=0&limit=256&wait_ms=0",
        string(job, "id")
      ),
    )
    .await;
    jobs.push(json!({"detail": detail, "events": events}));
  }
  BuildRun { build, attempt, jobs }
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

pub(super) async fn verify_artifacts(client: &Client, origin: &str, build: &Value) -> Value {
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
  assert_eq!(bytes.as_ref(), ARTIFACT_CONTENT);
  assert_eq!(download["artifact"]["sha256"], format!("{:x}", Sha256::digest(&bytes)));
  json!({"page": page, "downloaded_sha256": download["artifact"]["sha256"]})
}

pub(super) async fn verify_log_search(client: &Client, origin: &str, project_id: &str, build: &Value) -> Value {
  let full_text = wait_for_search(
    client,
    origin,
    project_id,
    &string(build, "id"),
    "release cache",
    "full_text",
  )
  .await;
  let literal = wait_for_search(
    client,
    origin,
    project_id,
    &string(build, "id"),
    "dist/message.txt",
    "literal",
  )
  .await;
  json!({"full_text": full_text, "literal": literal})
}

async fn wait_for_search(
  client: &Client,
  origin: &str,
  project_id: &str,
  build_id: &str,
  query: &str,
  mode: &str,
) -> Value {
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    let mut url = Url::parse(&format!("{origin}/api/v1/projects/{project_id}/build-logs/search")).unwrap();
    url
      .query_pairs_mut()
      .append_pair("query", query)
      .append_pair("mode", mode)
      .append_pair("build_id", build_id)
      .append_pair("limit", "10");
    let page = get_json(client, url.into()).await;
    if page["freshness"]["caught_up"] == true && !page["items"].as_array().unwrap().is_empty() {
      return page;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for {mode} log search: {page}"
    );
    sleep(Duration::from_millis(500)).await;
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
