//! Raw performance measurements for the released vertical slice.

use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use octacity_protocol::AgentCredentialToken;
use reqwest::Client;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::{task::JoinSet, time::Instant};

use super::{BuildRun, ReleaseBackend, get_json, issue_enrollment, string};

const SOAK_AGENT_COUNT: usize = 100;
const SOAK_POLL_ROUNDS: usize = 5;

#[derive(Serialize)]
struct Measurement<'a> {
  name: &'a str,
  unit: &'a str,
  samples: &'a [f64],
}

pub(super) struct Recorder {
  backend: String,
  samples: BTreeMap<&'static str, (&'static str, Vec<f64>)>,
}

impl Recorder {
  pub(super) fn new(backend: &str) -> Self {
    Self {
      backend: backend.to_owned(),
      samples: BTreeMap::new(),
    }
  }

  pub(super) fn duration(&mut self, name: &'static str, duration: Duration) {
    self.sample(name, "milliseconds", duration.as_secs_f64() * 1_000.0);
  }

  pub(super) fn sample(&mut self, name: &'static str, unit: &'static str, value: f64) {
    let entry = self.samples.entry(name).or_insert_with(|| (unit, Vec::new()));
    assert_eq!(entry.0, unit, "performance metric {name} changed unit");
    assert!(value.is_finite() && value >= 0.0, "invalid sample for {name}");
    entry.1.push(value);
  }

  pub(super) fn write(&self, evidence: &Path) {
    let measurements = self
      .samples
      .iter()
      .map(|(name, (unit, samples))| Measurement { name, unit, samples })
      .collect::<Vec<_>>();
    fs::write(
      evidence.join("performance-measurements.json"),
      serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "producer": format!("released-{}-vertical-slice", self.backend),
        "measurements": measurements,
      }))
      .unwrap(),
    )
    .unwrap();
  }
}

pub(super) fn enabled() -> bool {
  std::env::var("OCTACITY_PERFORMANCE_GATE").is_ok_and(|value| value == "true")
}

pub(super) async fn wait_for_agent_registration(client: &Client, origin: &str, name: &str) {
  let deadline = Instant::now() + Duration::from_secs(30);
  loop {
    let page = get_json(client, format!("{origin}/api/v1/agents?limit=100")).await;
    if page["items"]
      .as_array()
      .unwrap()
      .iter()
      .any(|agent| agent["name"] == name)
    {
      return;
    }
    assert!(Instant::now() < deadline, "Agent {name} did not register");
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
}

pub(super) fn record_run(recorder: &mut Recorder, run: &BuildRun) {
  let created_at = run.build["created_at_unix_ms"].as_i64().unwrap();
  if let Some(preparing_at) = run
    .jobs
    .iter()
    .flat_map(events)
    .find(|event| {
      event["payload"]["source"] == "agent"
        && event["payload"]["event"]["type"] == "state_changed"
        && event["payload"]["event"]["state"] == "preparing"
    })
    .and_then(|event| event["occurred_at_unix_ms"].as_i64())
  {
    recorder.sample(
      "queue_acquisition_latency_ms",
      "milliseconds",
      preparing_at.saturating_sub(created_at) as f64,
    );
  }
  for job in &run.jobs {
    if let (Some(preparing), Some(running)) = (
      lifecycle_timestamp(job, "preparing"),
      lifecycle_timestamp(job, "running"),
    ) {
      recorder.sample(
        "backend_startup_ms",
        "milliseconds",
        running.saturating_sub(preparing) as f64,
      );
    }
    if let (Some(cleaning), Some(completing)) = (
      lifecycle_timestamp(job, "cleaning"),
      lifecycle_timestamp(job, "completing"),
    ) {
      recorder.sample(
        "backend_teardown_ms",
        "milliseconds",
        completing.saturating_sub(cleaning) as f64,
      );
    }
    let occurred = events(job)
      .filter_map(|event| event["occurred_at_unix_ms"].as_i64())
      .collect::<Vec<_>>();
    if let (Some(first), Some(last)) = (occurred.first(), occurred.last()) {
      let elapsed_ms = last.saturating_sub(*first).max(1) as f64;
      recorder.sample(
        "event_throughput_per_second",
        "events_per_second",
        occurred.len() as f64 * 1_000.0 / elapsed_ms,
      );
    }
  }
}

fn lifecycle_timestamp(job: &Value, state: &str) -> Option<i64> {
  events(job)
    .find(|event| {
      event["payload"]["source"] == "agent"
        && event["payload"]["event"]["type"] == "state_changed"
        && event["payload"]["event"]["state"] == state
    })
    .and_then(|event| event["occurred_at_unix_ms"].as_i64())
}

pub(super) fn cache_restore_duration(run: &BuildRun) -> Duration {
  let cache_events = run
    .jobs
    .iter()
    .flat_map(events)
    .filter(|event| event["payload"]["source"] == "runner")
    .collect::<Vec<_>>();
  let started = cache_events
    .iter()
    .find(|event| event["payload"]["event"]["data"]["type"] == "cache_lookup_started")
    .and_then(|event| event["occurred_at_unix_ms"].as_i64())
    .expect("scheduled cache restore did not start");
  let completed = cache_events
    .iter()
    .find(|event| event["payload"]["event"]["data"]["type"] == "cache_hit")
    .and_then(|event| event["occurred_at_unix_ms"].as_i64())
    .expect("scheduled cache restore did not hit remote cache");
  Duration::from_millis(u64::try_from(completed.saturating_sub(started)).unwrap())
}

fn events(job: &Value) -> impl Iterator<Item = &Value> {
  job["events"]["items"].as_array().unwrap().iter()
}

pub(super) async fn rest_latency(client: &Client, origin: &str, build_id: &str) -> Vec<Duration> {
  let mut samples = Vec::with_capacity(20);
  for _ in 0..20 {
    let started = Instant::now();
    let build = get_json(client, format!("{origin}/api/v1/builds/{build_id}")).await;
    assert_eq!(string(&build, "id"), build_id);
    samples.push(started.elapsed());
  }
  samples
}

pub(super) async fn database_contention(pool: &PgPool, build_id: &str) -> Vec<Duration> {
  let build_id = uuid::Uuid::parse_str(build_id).unwrap();
  let mut queries = JoinSet::new();
  for _ in 0..32 {
    let pool = pool.clone();
    queries.spawn(async move {
      let started = Instant::now();
      let state: String = sqlx::query_scalar("SELECT state FROM builds WHERE id = $1")
        .bind(build_id)
        .fetch_one(&pool)
        .await
        .unwrap();
      assert_eq!(state, "succeeded");
      started.elapsed()
    });
  }
  let mut samples = Vec::with_capacity(32);
  while let Some(result) = queries.join_next().await {
    samples.push(result.unwrap());
  }
  samples
}

pub(super) async fn agent_soak(
  client: &Client,
  management_origin: &str,
  agent_origin: &str,
  run: &str,
  pool_id: &str,
  backend: &ReleaseBackend,
  database: &PgPool,
) -> (Vec<Duration>, Vec<Duration>, usize, usize) {
  let mut enrollments = Vec::with_capacity(SOAK_AGENT_COUNT);
  for index in 0..SOAK_AGENT_COUNT {
    let name = format!("{run}-soak-{index:03}");
    let credential = issue_enrollment(client, management_origin, &name, pool_id, backend).await;
    enrollments.push((name, credential));
  }

  let (operating_system, architecture) = backend.agent_platform();
  let mut agents = JoinSet::new();
  for (name, credential) in enrollments {
    let client = client.clone();
    let agent_origin = agent_origin.to_owned();
    agents.spawn(async move {
      let request_id = format!("register-{name}");
      let registration_started = Instant::now();
      let response = client
        .post(format!("{agent_origin}/api/v1/agents/register"))
        .bearer_auth(&credential)
        .header("idempotency-key", &request_id)
        .json(&registration_request(
          &name,
          &request_id,
          operating_system,
          architecture,
        ))
        .send()
        .await
        .map_err(|error| format!("registration transport failed: {error}"))?;
      let status = response.status();
      let body = response
        .bytes()
        .await
        .map_err(|error| format!("registration response failed: {error}"))?;
      if !status.is_success() {
        return Err(format!(
          "registration returned {status}: {}",
          String::from_utf8_lossy(&body)
        ));
      }
      let registration: Value = serde_json::from_slice(&body).unwrap();
      let registration_elapsed = registration_started.elapsed();
      let registration_id = string(&registration, "registration_id");
      let promoted = AgentCredentialToken::parse(&credential)
        .unwrap()
        .promote(&registration_id)
        .unwrap()
        .encode();
      let mut polls = Vec::with_capacity(SOAK_POLL_ROUNDS);
      for round in 0..SOAK_POLL_ROUNDS {
        let poll_request_id = format!("poll-{name}-{round}");
        let poll_started = Instant::now();
        let response = client
          .post(format!("{agent_origin}/api/v1/agents/{name}/leases:acquire"))
          .bearer_auth(promoted.as_str())
          .header("idempotency-key", &poll_request_id)
          .json(&json!({
            "protocol_version": 1,
            "request_id": poll_request_id,
            "registration_id": registration_id,
            "wait_seconds": 1,
            "accept_jobs": false,
            "snapshot": {
              "available_cpu_millis": 1000,
              "available_memory_bytes": 1073741824_u64,
              "work_disk_free_bytes": 1073741824_u64,
              "state_disk_free_bytes": 1073741824_u64,
              "active_job": null,
              "backends": []
            }
          }))
          .send()
          .await
          .map_err(|error| format!("lease poll transport failed: {error}"))?;
        let status = response.status();
        let body = response
          .bytes()
          .await
          .map_err(|error| format!("lease poll response failed: {error}"))?;
        if !status.is_success() {
          return Err(format!(
            "lease poll returned {status}: {}",
            String::from_utf8_lossy(&body)
          ));
        }
        polls.push(poll_started.elapsed());
      }
      Ok((registration_elapsed, polls))
    });
  }

  let mut registrations = Vec::with_capacity(SOAK_AGENT_COUNT);
  let mut polls = Vec::with_capacity(SOAK_AGENT_COUNT);
  let mut errors = 0;
  while let Some(result) = agents.join_next().await {
    match result.unwrap() {
      Ok((registration, agent_polls)) => {
        registrations.push(registration);
        polls.extend(agent_polls);
      }
      Err(error) => {
        eprintln!("100-Agent soak request failed: {error}");
        errors += 1;
      }
    }
  }
  let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE name LIKE $1")
    .bind(format!("{run}-soak-%"))
    .fetch_one(database)
    .await
    .unwrap();
  assert_eq!(stored, SOAK_AGENT_COUNT as i64 - errors as i64);
  (registrations, polls, errors, usize::try_from(stored).unwrap())
}

fn registration_request(name: &str, request_id: &str, operating_system: &str, architecture: &str) -> Value {
  json!({
    "protocol_version": 1,
    "request_id": request_id,
    "inventory": {
      "agent_id": name,
      "agent_version": "performance-contract",
      "coordinator_protocols": [1],
      "labels": {"contract": "100-agent-soak"},
      "host_platform": {"os": operating_system, "architecture": architecture},
      "host_capacity": {
        "logical_cpu_count": 1,
        "total_memory_bytes": 1073741824_u64,
        "work_disk_total_bytes": 1073741824_u64,
        "state_disk_total_bytes": 1073741824_u64,
        "virtualization_available": false
      },
      "runtimes": [],
      "octa": {
        "version": "performance-contract",
        "runner_sha256": "11".repeat(32),
        "build_commit": null,
        "runner_protocols": [1],
        "event_schemas": [1],
        "plugin_protocols": [1],
        "octafile_versions": [1],
        "features": [],
        "plugins": []
      },
      "source_plugins": []
    }
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn registration_fixture_is_a_valid_protocol_document() {
    let request: octacity_protocol::RegisterAgentRequest =
      serde_json::from_value(registration_request("agent-1", "request-1", "linux", "amd64")).unwrap();
    request.validate().unwrap();
  }
}
