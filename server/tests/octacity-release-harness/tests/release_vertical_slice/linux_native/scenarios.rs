//! Cancellation and retry scenarios in the released Linux Native matrix.

use std::{collections::BTreeSet, ffi::OsString, fs, path::Path, time::Duration};

use reqwest::Client;
use serde_json::{Value, json};
use tokio::{
  process::Child,
  time::{Instant, sleep},
};

use super::{
  accept_manual_build, assert_lifecycle_order, assert_native_resource_controls, assert_resource_samples, get_json,
  post_management, string, wait_for_terminal_build,
};

pub(super) struct ManualBuildInput<'a> {
  pub(super) client: &'a Client,
  pub(super) origin: &'a str,
  pub(super) run: &'a str,
  pub(super) trigger_id: &'a str,
  pub(super) configuration: &'a str,
  pub(super) revision: &'a str,
  pub(super) stderr: &'a Path,
}

pub(super) struct CancellationInput<'a> {
  pub(super) build: ManualBuildInput<'a>,
  pub(super) native_cgroup: Option<NativeCgroupAssertion<'a>>,
  pub(super) ready_marker: Option<ReadyMarkerAssertion<'a>>,
  pub(super) maximum_disk_bytes: u64,
}

pub(super) struct NativeCgroupAssertion<'a> {
  pub(super) root: &'a Path,
  pub(super) baseline: &'a BTreeSet<OsString>,
}

pub(super) struct ReadyMarkerAssertion<'a> {
  pub(super) root: &'a Path,
  pub(super) file_name: &'a str,
}

pub(super) struct RetryInput<'a> {
  pub(super) build: ManualBuildInput<'a>,
}

pub(super) async fn run_and_cancel(input: CancellationInput<'_>, agent: &mut Child) -> Value {
  let build_input = input.build;
  let identity = format!("{}-cancel", build_input.run);
  let accepted = accept_manual_build(
    build_input.client,
    build_input.origin,
    &identity,
    build_input.trigger_id,
    build_input.configuration,
    build_input.revision,
  )
  .await;
  let build_id = string(&accepted, "build_id");
  let attempt_id = string(&accepted, "attempt_id");
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    if let Some(status) = agent.try_wait().unwrap() {
      panic!(
        "released Agent exited before cancellation with {status}: {}",
        fs::read_to_string(build_input.stderr).unwrap_or_default()
      );
    }
    let attempt = get_json(
      build_input.client,
      format!("{}/api/v1/attempts/{attempt_id}", build_input.origin),
    )
    .await;
    let job = &attempt["jobs"][0];
    let events = get_json(
      build_input.client,
      format!(
        "{}/api/v1/jobs/{}/events?after=0&limit=100&wait_ms=0",
        build_input.origin,
        string(job, "id")
      ),
    )
    .await;
    let started = events["items"].as_array().unwrap().iter().any(|event| {
      event["payload"]["source"] == "agent"
        && event["payload"]["event"]["type"] == "state_changed"
        && event["payload"]["event"]["state"] == "running"
    });
    let sampled = events["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|event| event["payload"]["source"] == "agent" && event["payload"]["event"]["type"] == "resource_usage");
    let fixture_ready = input
      .ready_marker
      .as_ref()
      .is_none_or(|marker| super::contains_file_named(marker.root, marker.file_name));
    if started && sampled && fixture_ready {
      if let Some(cgroup) = &input.native_cgroup {
        assert_native_resource_controls(cgroup.root, cgroup.baseline);
      }
      break;
    }
    assert!(
      Instant::now() < deadline,
      "cancelled Build never started with resource accounting and its required process marker"
    );
    sleep(Duration::from_millis(250)).await;
  }
  let cancellation = post_management(
    build_input.client,
    build_input.origin,
    &format!("/api/v1/builds/{build_id}/cancel"),
    &format!("{}-cancel-request", build_input.run),
    json!({}),
  )
  .await;
  let build = wait_for_terminal_build(
    build_input.client,
    build_input.origin,
    &build_id,
    agent,
    build_input.stderr,
  )
  .await;
  assert_eq!(build["state"], "cancelled");
  let attempt = get_json(
    build_input.client,
    format!("{}/api/v1/attempts/{attempt_id}", build_input.origin),
  )
  .await;
  let job = &attempt["jobs"][0];
  let events = get_json(
    build_input.client,
    format!(
      "{}/api/v1/jobs/{}/events?after=0&limit=256&wait_ms=0",
      build_input.origin,
      string(job, "id")
    ),
  )
  .await;
  assert_lifecycle_order(events["items"].as_array().unwrap());
  assert_resource_samples(events["items"].as_array().unwrap(), input.maximum_disk_bytes);
  json!({"accepted": accepted, "cancellation": cancellation, "build": build, "attempt": attempt, "events": events})
}

pub(super) async fn run_and_retry(input: RetryInput<'_>, agent: &mut Child) -> Value {
  let build_input = input.build;
  let identity = format!("{}-retry", build_input.run);
  let accepted = accept_manual_build(
    build_input.client,
    build_input.origin,
    &identity,
    build_input.trigger_id,
    build_input.configuration,
    build_input.revision,
  )
  .await;
  let build_id = string(&accepted, "build_id");
  let source_attempt_id = string(&accepted, "attempt_id");
  let first_failure = wait_for_terminal_build(
    build_input.client,
    build_input.origin,
    &build_id,
    agent,
    build_input.stderr,
  )
  .await;
  assert_eq!(first_failure["state"], "failed");
  let source_attempt = get_json(
    build_input.client,
    format!("{}/api/v1/attempts/{source_attempt_id}", build_input.origin),
  )
  .await;
  assert_failed_attempt(&source_attempt, 1, None);

  let retry_key = format!("{}-retry-request", build_input.run);
  let retry = post_management(
    build_input.client,
    build_input.origin,
    &format!("/api/v1/builds/{build_id}/retry"),
    &retry_key,
    json!({}),
  )
  .await;
  let replay = post_management(
    build_input.client,
    build_input.origin,
    &format!("/api/v1/builds/{build_id}/retry"),
    &retry_key,
    json!({}),
  )
  .await;
  assert_eq!(retry["disposition"], "applied");
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(retry["attempt_id"], replay["attempt_id"]);
  assert_eq!(retry["source_attempt_id"], source_attempt_id);
  assert_eq!(retry["attempt_number"], 2);

  let retried_attempt_id = string(&retry, "attempt_id");
  let second_failure = wait_for_terminal_build(
    build_input.client,
    build_input.origin,
    &build_id,
    agent,
    build_input.stderr,
  )
  .await;
  assert_eq!(second_failure["state"], "failed");
  let retried_attempt = get_json(
    build_input.client,
    format!("{}/api/v1/attempts/{retried_attempt_id}", build_input.origin),
  )
  .await;
  assert_failed_attempt(&retried_attempt, 2, Some(&source_attempt_id));
  let retained_source = get_json(
    build_input.client,
    format!("{}/api/v1/attempts/{source_attempt_id}", build_input.origin),
  )
  .await;
  assert_eq!(retained_source, source_attempt);

  json!({
    "accepted": accepted,
    "first_failure": first_failure,
    "source_attempt": source_attempt,
    "retry": retry,
    "retry_replay": replay,
    "second_failure": second_failure,
    "retried_attempt": retried_attempt,
  })
}

fn assert_failed_attempt(attempt: &Value, number: u64, retry_of_attempt_id: Option<&str>) {
  assert_eq!(attempt["attempt"]["number"], number);
  assert_eq!(attempt["attempt"]["retry_of_attempt_id"].as_str(), retry_of_attempt_id);
  let jobs = attempt["jobs"].as_array().expect("Attempt jobs must be an array");
  assert_eq!(jobs.len(), 1);
  assert_eq!(jobs[0]["state"], "failed");
  assert_eq!(jobs[0]["terminal"]["failure_classification"], "execution");
}
