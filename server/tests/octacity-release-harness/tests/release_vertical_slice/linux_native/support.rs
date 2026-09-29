use super::*;

pub(super) async fn create_manual_definition(
  client: &Client,
  origin: &str,
  run: &str,
  name: &str,
  configuration: &str,
) -> String {
  let response = post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/manual",
    &format!("{run}-{name}-manual-definition"),
    json!({"configuration_id": configuration, "configuration_version": 1, "enabled": true, "definition": {}}),
  )
  .await;
  resource_id(&response)
}

pub(super) async fn accept_manual_build(
  client: &Client,
  origin: &str,
  identity: &str,
  trigger_id: &str,
  configuration_id: &str,
  revision: &str,
) -> Value {
  post_management(
    client,
    origin,
    "/api/v1/triggers/manual",
    identity,
    json!({
      "trigger_id": trigger_id,
      "trigger_version": 1,
      "configuration_id": configuration_id,
      "configuration_version": 1,
      "deduplication_identity": identity,
      "source": {"kind": "exact_revision", "value": revision},
      "parameters": {},
      "priority": 50
    }),
  )
  .await
}

pub(super) async fn create_schedule(
  client: &Client,
  origin: &str,
  run: &str,
  configuration: &str,
  revision: &str,
) -> Value {
  let occurrence = Utc::now() + chrono::Duration::seconds(15);
  let expression = format!(
    "{} {} {} {} {} * *",
    occurrence.second(),
    occurrence.minute(),
    occurrence.hour(),
    occurrence.day(),
    occurrence.month()
  );
  post_management(
    client,
    origin,
    "/api/v1/trigger-definitions/scheduled",
    &format!("{run}-schedule"),
    json!({
      "configuration_id": configuration,
      "configuration_version": 1,
      "enabled": true,
      "schedule": {"expression": expression, "timezone": "UTC", "missed_run_policy": {"kind": "run_once"}},
      "build": {
        "source": {"kind": "exact_revision", "value": revision},
        "parameters": {},
        "priority": 45
      }
    }),
  )
  .await
}

pub(super) async fn start_matrix_agent(input: AgentStartInput<'_>) -> MatrixAgent {
  let enrollment = issue_enrollment(
    input.client,
    input.management_origin,
    &format!("{}-{}", input.run, input.evidence_name),
    input.pool_id,
    input.backend,
  )
  .await;
  let directory = input.temporary.join(input.evidence_name);
  fs::create_dir(&directory).unwrap();
  let upload_origins = [input.object_endpoint];
  let config = write_agent_config(
    &directory,
    input.agent_origin,
    input.agent_name,
    &enrollment,
    AgentReleasePaths::from(input.release),
    input.backend,
    &AgentConfigOverrides {
      cache_read: true,
      cache_write: true,
      remote_cache_origin: Some(&input.cache_proxy.origin),
      cache_ca_certificate: Some(&input.cache_proxy.ca_certificate),
      unrestricted_network: true,
      upload_origins: &upload_origins,
      output_limit_bytes: Some(OUTPUT_BYTES),
    },
  );
  let child = spawn_agent(
    &input.release.agent_binary,
    &config,
    &input.evidence.join(format!("{}.stdout.log", input.evidence_name)),
    &input.evidence.join(format!("{}.stderr.log", input.evidence_name)),
  );
  MatrixAgent { child }
}

pub(super) async fn stop_agent(
  client: &Client,
  origin: &str,
  name: &str,
  run: &str,
  agent: &mut MatrixAgent,
  stderr: &Path,
) {
  drain_agent(client, origin, name, &format!("{run}-{name}")).await;
  let status = timeout(Duration::from_secs(45), agent.child.wait())
    .await
    .expect("released Agent did not stop after drain")
    .unwrap();
  assert!(
    status.success(),
    "released Agent exited unsuccessfully: {}",
    fs::read_to_string(stderr).unwrap_or_default()
  );
}

pub(super) async fn wait_for_trigger_build(pool: &PgPool, trigger_id: &str) -> String {
  let trigger_id = Uuid::parse_str(trigger_id).unwrap();
  let deadline = Instant::now() + Duration::from_secs(120);
  loop {
    let build_id = sqlx::query_scalar::<_, Uuid>(
      "SELECT build_id FROM trigger_occurrences WHERE trigger_id = $1 AND build_id IS NOT NULL ORDER BY created_at LIMIT 1",
    )
    .bind(trigger_id)
    .fetch_optional(pool)
    .await
    .unwrap();
    if let Some(build_id) = build_id {
      return build_id.to_string();
    }
    assert!(Instant::now() < deadline, "timed out waiting for Trigger {trigger_id}");
    sleep(Duration::from_millis(500)).await;
  }
}

pub(super) async fn occurrence_count(pool: &PgPool, trigger_id: &str) -> i64 {
  sqlx::query_scalar("SELECT COUNT(*) FROM trigger_occurrences WHERE trigger_id = $1")
    .bind(Uuid::parse_str(trigger_id).unwrap())
    .fetch_one(pool)
    .await
    .unwrap()
}

pub(super) fn assert_native_resource_controls(cgroup_root: &Path, baseline: &BTreeSet<OsString>) {
  let current = directory_entries(cgroup_root);
  let active = current.difference(baseline).collect::<Vec<_>>();
  assert_eq!(active.len(), 1, "one Native execution cgroup must be active");
  let cgroup = cgroup_root.join(active[0]);
  assert_eq!(
    fs::read_to_string(cgroup.join("memory.max")).unwrap().trim(),
    MEMORY_BYTES.to_string()
  );
  assert_eq!(fs::read_to_string(cgroup.join("memory.swap.max")).unwrap().trim(), "0");
  assert_eq!(fs::read_to_string(cgroup.join("pids.max")).unwrap().trim(), "4096");
  let cpu = fs::read_to_string(cgroup.join("cpu.max")).unwrap();
  let values = cpu
    .split_whitespace()
    .map(|value| value.parse::<u64>().unwrap())
    .collect::<Vec<_>>();
  assert_eq!(values.len(), 2);
  assert_eq!(values[0], values[1], "1000 CPU millis must enforce one complete CPU");
}

pub(super) fn backend_work_root(backend: &ReleaseBackend) -> &Path {
  match backend {
    ReleaseBackend::Native { work_root, .. }
    | ReleaseBackend::Microsandbox { work_root, .. }
    | ReleaseBackend::Containerd { work_root, .. }
    | ReleaseBackend::AppleVf { work_root, .. } => work_root,
  }
}

pub(super) fn cache_proxy_advertised_host(backend: &ReleaseBackend) -> &'static str {
  match backend {
    ReleaseBackend::Microsandbox { .. } => MICROSANDBOX_HOST_ALIAS,
    ReleaseBackend::Native { .. } | ReleaseBackend::Containerd { .. } | ReleaseBackend::AppleVf { .. } => "127.0.0.1",
  }
}

pub(super) fn write_cache_diagnostics(evidence: &Path, phase: &str, run: &BuildRun) -> Value {
  let diagnostics = json!({"events": cache_events(run)});
  fs::write(
    evidence.join(format!("cache-{phase}.json")),
    serde_json::to_vec_pretty(&diagnostics).unwrap(),
  )
  .unwrap();
  diagnostics
}

pub(super) fn native_roots(backend: &ReleaseBackend) -> Option<(&Path, &Path)> {
  match backend {
    ReleaseBackend::Native {
      work_root, cgroup_root, ..
    } => Some((work_root, cgroup_root)),
    ReleaseBackend::Microsandbox { .. } | ReleaseBackend::Containerd { .. } | ReleaseBackend::AppleVf { .. } => None,
  }
}

pub(super) fn native_cache_root(backend: &ReleaseBackend) -> Option<&Path> {
  match backend {
    ReleaseBackend::Native { cache_root, .. } => Some(cache_root),
    ReleaseBackend::Microsandbox { .. } | ReleaseBackend::Containerd { .. } | ReleaseBackend::AppleVf { .. } => None,
  }
}

pub(super) fn available_disk_bytes(path: &Path) -> u64 {
  let statistics = rustix::fs::statvfs(path).unwrap();
  statistics.f_bavail.saturating_mul(statistics.f_frsize)
}

pub(super) fn directory_entries(path: &Path) -> BTreeSet<OsString> {
  fs::read_dir(path)
    .unwrap()
    .map(|entry| entry.unwrap().file_name())
    .collect()
}

pub(super) fn remove_agent_cache_entries(cache_root: &Path, baseline: &BTreeSet<OsString>) {
  for entry in fs::read_dir(cache_root).unwrap() {
    let entry = entry.unwrap();
    if baseline.contains(&entry.file_name()) {
      continue;
    }
    let path = entry.path();
    let file_type = entry.file_type().unwrap();
    if file_type.is_dir() {
      fs::remove_dir_all(path).unwrap();
    } else {
      fs::remove_file(path).unwrap();
    }
  }
  assert_eq!(
    directory_entries(cache_root),
    *baseline,
    "Native L1 cache cleanup failed"
  );
}

pub(super) fn unused_loopback_address() -> SocketAddr {
  TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap()
}
