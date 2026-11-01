#![cfg(unix)]
//! Clean Project authoring through real HTTP, PostgreSQL and an isolated Agent.
//! Only the model process is finite; no Factory handler or execution is replaced.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_release_harness::InstalledToolchain as Toolchain;
use octacity_server_factory::*;
use reqwest::Client;
use serde_json::{Value, json};
use std::{
  collections::BTreeMap,
  env,
  fs::{self, File},
  net::{SocketAddr, TcpListener},
  path::{Path, PathBuf},
  process::Stdio,
  time::Duration,
};
use tokio::{
  process::{Child, Command},
  time::{Instant, sleep, timeout},
};
use uuid::Uuid;

#[allow(dead_code)]
#[path = "release_vertical_slice/agent.rs"]
mod agent;
#[allow(dead_code)]
#[path = "release_vertical_slice/backend.rs"]
mod backend;
#[allow(dead_code)]
#[path = "release_vertical_slice/browser_proxy.rs"]
mod browser_proxy;
#[path = "factory_manual_authoring/code.rs"]
mod code;
#[path = "factory_manual_authoring/console.rs"]
mod console;
#[path = "release_vertical_slice/environment.rs"]
mod environment;
#[path = "factory_manual_authoring/installation.rs"]
mod installation;
#[path = "factory_manual_authoring/isolation.rs"]
mod isolation;
#[path = "factory_pilot/source.rs"]
mod local_mirror;
#[path = "factory_manual_authoring/recovery.rs"]
mod recovery;
#[allow(dead_code)]
#[path = "release_vertical_slice/server.rs"]
mod server;
#[allow(dead_code)]
#[path = "release_vertical_slice/support.rs"]
mod support;
#[path = "factory_manual_authoring/verification.rs"]
mod verification;
#[path = "factory_manual_authoring/workflow.rs"]
mod workflow;
use backend::ReleaseBackend;
use environment::*;
use server::*;
use support::{get_json, post_management, resource_id, string};

const SIGNING_SEED: [u8; 32] = [7; 32];
const RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS: u64 = 5;
const RELEASE_JOB_AUTHORITY_LIFETIME_MILLISECONDS: u64 = 600_000;
const RELEASE_OBJECT_OPERATION_TIMEOUT_MILLISECONDS: u64 = 5_000;
const RELEASE_READINESS_TIMEOUT_MILLISECONDS: u64 = 15_000;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires current server/Agent, pinned Octa, disposable PostgreSQL/S3 and qualified Microsandbox"]
async fn clean_project_authors_and_executes_two_isolated_agents_without_a_business_preset() {
  journey(Scenario::Headless).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Console/Server, disposable services and qualified Microsandbox"]
async fn packaged_console_authors_and_executes_two_isolated_agents_from_empty_project() {
  let requested = env::var("OCTACITY_RELEASE_AUTHORING_LOCALE").ok();
  if let Some(locale) = requested.as_deref() {
    assert!(["en", "ru"].contains(&locale), "unsupported qualification locale");
    journey(Scenario::Console(locale)).await;
  } else {
    for locale in ["en", "ru"] {
      journey(Scenario::Console(locale)).await;
    }
  }
}

#[derive(Clone, Copy)]
enum Scenario<'a> {
  Headless,
  Console(&'a str),
  RepositoryFree,
  Repositories,
  Recovery,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires current Server/Agent, disposable services and qualified Microsandbox"]
async fn unrelated_three_step_repository_free_workflow_retains_only_selected_inputs() {
  journey(Scenario::RepositoryFree).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires current Server/Agent, disposable services and qualified Microsandbox"]
async fn unrelated_three_step_workflow_isolates_repositories_a_b_and_no_code() {
  journey(Scenario::Repositories).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires current Server/Agent, disposable services and qualified Microsandbox"]
async fn manual_workflow_recovers_after_accepted_result_with_original_documents_once() {
  journey(Scenario::Recovery).await;
}

async fn journey(scenario: Scenario<'_>) {
  let locale = match scenario {
    Scenario::Console(locale) => Some(locale),
    _ => None,
  };
  let backend = ReleaseBackend::from_environment();
  let runtime = octacity_release_harness::validate_installed_agent_runtime(
    &octacity_release_harness::AgentRuntimeBundles {
      agent: required_path("OCTACITY_RELEASE_AGENT_ROOT", true),
      octa: required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT", true),
    },
    &octacity_release_harness::AgentRuntimePlatforms::new(
      format!("{}-{}", env::consts::OS, host_architecture()),
      if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
      } else {
        "linux-x86_64"
      },
    ),
  )
  .unwrap();
  let evidence_root = if env::var_os("OCTACITY_RELEASE_EVIDENCE_DIR").is_some() {
    required_path("OCTACITY_RELEASE_EVIDENCE_DIR", true)
  } else {
    required_path("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT", true)
      .parent()
      .unwrap()
      .to_path_buf()
  };
  let fixture = tempfile::Builder::new()
    .prefix("om-")
    .tempdir_in(&evidence_root)
    .unwrap();
  let directory = fixture.keep().canonicalize().unwrap();
  let admin = sqlx::PgPool::connect(&required_string("OCTACITY_POSTGRES_URL"))
    .await
    .unwrap();
  let database = format!("octacity_manual_{}", Uuid::new_v4().simple());
  sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{database}\"")))
    .execute(&admin)
    .await
    .unwrap();
  let mut postgres = url::Url::parse(&required_string("OCTACITY_POSTGRES_URL")).unwrap();
  postgres.set_path(&database);
  let (management_addr, agent_addr) = unused_loopback_addresses();
  let objects = required_string("OCTACITY_MINIO_ENDPOINT");
  let input = ServerConfigInput {
    postgres_url: postgres.as_str(),
    object_endpoint: &objects,
    cache_endpoint: "https://cache.example",
    toolchain: &runtime.toolchain,
    management_addr,
    agent_addr,
    cache_addr: None,
  };
  let config = write_server_config(&directory, &input);
  private_file(
    &directory,
    "object-access-key",
    &required_string("OCTACITY_MINIO_ACCESS_KEY"),
  );
  private_file(
    &directory,
    "object-secret-key",
    &required_string("OCTACITY_MINIO_SECRET_KEY"),
  );
  let client = Client::new();
  let origin = format!("http://{management_addr}");
  let log = directory.join("server.log");
  let binary = required_path("OCTACITY_MANUAL_SERVER_BINARY", true);
  let mut server = spawn_server(&binary, &config, &log, &directory.join("server.err"));
  wait_for_server_ready(&client, &origin, &mut server, &log).await;
  let resources = installation::create(&client, &origin, &backend).await;
  let profile = installation::profile(&directory, &resources, &backend, &runtime.octa_root);
  let observed_plugins = code::observe_git(&directory, &runtime.source_plugins);
  let code = if locale.is_some() || matches!(scenario, Scenario::Repositories) {
    Some(
      code::install(
        &client,
        &origin,
        &directory,
        &resources,
        &profile,
        &backend,
        &observed_plugins,
      )
      .await,
    )
  } else {
    None
  };
  let second_code = if matches!(scenario, Scenario::Repositories) {
    Some(
      code::install_named(
        &client,
        &origin,
        &directory,
        &resources,
        &profile,
        &backend,
        &code.as_ref().unwrap().source_plugins,
        "bulletin-b",
        "repository-b-canary: approved translation rules\n",
        &[code.as_ref().unwrap()],
      )
      .await,
    )
  } else {
    None
  };
  shutdown_server(&mut server, &log).await;
  let codes: Vec<_> = code
    .iter()
    .chain(second_code.iter())
    .map(|code| &code.profile)
    .collect();
  let profiles = installation::catalog(&directory, &resources, &profile, &codes);
  let connectors = private_file(&directory, "connectors.json", r#"{"version":1,"connectors":[]}"#);
  let config = write_factory_server_config(
    &directory,
    &input,
    &FactoryServerFiles {
      profiles: &profiles,
      connectors: &connectors,
    },
  );
  private_file(
    &directory,
    "object-access-key",
    &required_string("OCTACITY_MINIO_ACCESS_KEY"),
  );
  private_file(
    &directory,
    "object-secret-key",
    &required_string("OCTACITY_MINIO_SECRET_KEY"),
  );
  server = spawn_server(&binary, &config, &log, &directory.join("server.err"));
  wait_for_server_ready(&client, &origin, &mut server, &log).await;
  for collection in ["flow-definitions", "factory-configurations", "builds", "factory-runs"] {
    let page = get_json(
      &client,
      format!("{origin}/api/v1/projects/{}/{collection}?limit=100", resources.project),
    )
    .await;
    assert_eq!(
      page["items"],
      json!([]),
      "technical installation must not seed {collection}"
    );
  }
  let capabilities = get_json(
    &client,
    format!(
      "{origin}/api/v1/projects/{}/factory-authoring-capabilities",
      resources.project
    ),
  )
  .await;
  assert!(
    capabilities["build_resource_modes"]
      .as_array()
      .unwrap()
      .contains(&json!("none"))
  );
  let mut isolated = None;
  let run = if let Some(locale) = locale {
    console::author(
      &client,
      &origin,
      &resources.project,
      &directory,
      locale,
      code.as_ref().unwrap(),
    )
    .await
  } else {
    let published = installation::publish_role(&client, &origin, &resources, &profile, "manual-first-agent").await;
    let resolved = post_management(
      &client,
      &origin,
      &format!(
        "/api/v1/projects/{}/factory-execution-bindings/resolve",
        resources.project
      ),
      "resolve-first-step",
      json!({
        "profile":published["profiles"][0]["reference"], "capability":"manual-runtime", "result_outcome":"assessed"
      }),
    )
    .await;
    assert!(
      resolved["incompatibility"].is_null(),
      "first source-free step must resolve from installation alone: {resolved}"
    );
    assert!(resolved["build"].is_object());
    if matches!(
      scenario,
      Scenario::RepositoryFree | Scenario::Repositories | Scenario::Recovery
    ) {
      let authored = isolation::run(
        &client,
        &origin,
        &resources,
        &profile,
        &published,
        code.as_ref().zip(second_code.as_ref()),
        matches!(scenario, Scenario::Recovery),
      )
      .await;
      let run = authored.run.clone();
      isolated = Some(authored);
      run
    } else {
      workflow::run(&client, &origin, &resources, &profile, &published).await
    }
  };
  let enrollment=post_management(&client,&origin,"/api/v1/agent-enrollments","manual-enrollment",json!({"pool_id":resources.pool,"pool_version":1,"expected_platform":{"operating_system":backend.agent_platform().0,"architecture":backend.agent_platform().1}})).await;
  let agent_directory = directory.join("agent");
  fs::create_dir(&agent_directory).unwrap();
  set_mode(&agent_directory, 0o700);
  let credential = private_file(
    &agent_directory,
    "model.json",
    r#"{"OCTACITY_CODEX_AUTH":"manual-private-model-credential"}"#,
  );
  let credentials = BTreeMap::from([("manual-model".to_owned(), credential)]);
  let grants = octacity_server_store::factory_job_permissions(
    &code.as_ref().map_or(&profile, |code| &code.profile).project_permissions,
  )
  .unwrap();
  let digest = profile.toolchain.executable.digest().to_string();
  let executable = directory.join("finite-model");
  let platform = runtime.octa_capabilities.platform();
  let executables = [agent::AgentToolExecutable {
    product: "codex-cli",
    standalone: false,
    path: &executable,
    version: "0.161.0",
    platform,
    sha256: &digest,
  }];
  let origins = [objects.as_str()];
  let mut overrides = agent::AgentConfigOverrides::restricted_without_outputs(&origins);
  overrides.factory = Some(agent::AgentFactoryOverrides {
    permissions: &grants,
    credential_profiles: &credentials,
    capture: None,
    protected_reader: None,
  });
  overrides.tool_executables = &executables;
  overrides.output_limits = Some(octacity_protocol::OutputLimits {
    artifact_count: 16,
    artifact_bytes: 4 * 1024 * 1024,
    report_count: 16,
    report_bytes: 4 * 1024 * 1024,
    single_output_bytes: 4 * 1024 * 1024,
  });
  let agent_config = agent::write_agent_config(
    &agent_directory,
    &format!("http://{agent_addr}"),
    "manual-worker",
    &string(&enrollment, "credential"),
    agent::AgentReleasePaths {
      octa_root: &runtime.octa_root,
      source_plugins: second_code
        .as_ref()
        .or(code.as_ref())
        .map_or(&observed_plugins, |code| &code.source_plugins),
    },
    &backend,
    &overrides,
  );
  let mut worker = agent::spawn_agent(
    &required_path("OCTACITY_MANUAL_AGENT_BINARY", true),
    &agent_config,
    &agent_directory.join("stdout.log"),
    &agent_directory.join("stderr.log"),
  );
  if matches!(scenario, Scenario::Recovery) {
    let checkpoint = recovery::checkpoint(&client, &origin, &run).await;
    recovery::edit_documents(&client, &origin, &resources.project, isolated.as_ref().unwrap()).await;
    // The installed Agent has finite transport retries; restart its normal
    // process with the same durable state after the Server is available again.
    worker.kill().await.unwrap();
    worker.wait().await.unwrap();
    shutdown_server(&mut server, &log).await;
    server = spawn_server(&binary, &config, &log, &directory.join("server.err"));
    wait_for_server_ready(&client, &origin, &mut server, &log).await;
    worker = agent::spawn_agent(
      &required_path("OCTACITY_MANUAL_AGENT_BINARY", true),
      &agent_config,
      &agent_directory.join("restarted-stdout.log"),
      &agent_directory.join("restarted-stderr.log"),
    );
    recovery::resume(&client, &origin, &run, &checkpoint).await;
  }
  let state = timeout(Duration::from_secs(240), async {
    loop {
      let state = get_json(&client, format!("{origin}/api/v1/factory-runs/{run}")).await;
      if ["completed", "failed", "cancelled"].contains(&state["state"].as_str().unwrap_or("")) {
        break state;
      }
      if let Some(status) = worker.try_wait().unwrap() {
        panic!("Agent exited: {status}; private evidence: {}", directory.display());
      }
      let builds = get_json(
        &client,
        format!("{origin}/api/v1/projects/{}/builds?limit=100", resources.project),
      )
      .await;
      for build in builds["items"].as_array().unwrap() {
        assert_ne!(
          build["state"],
          "failed",
          "ordinary Build failed: {build}; private evidence: {}",
          directory.display()
        );
      }
      sleep(Duration::from_millis(100)).await;
    }
  })
  .await
  .expect("manual pipeline did not become terminal");
  assert_eq!(
    state["state"],
    "completed",
    "{state}; private evidence: {}",
    directory.display()
  );
  let retained = if let Some(authored) = &isolated {
    isolation::completed(&client, &origin, authored).await
  } else {
    verification::completed(&client, &origin, &run, locale.is_none(), code.as_ref()).await
  };
  match scenario {
    Scenario::RepositoryFree | Scenario::Recovery => assert!(
      !directory.join("git-invocations").exists(),
      "repository-free execution must not invoke Git"
    ),
    Scenario::Repositories => {
      let log = fs::read_to_string(directory.join("git-invocations")).unwrap();
      let directories: std::collections::BTreeSet<_> =
        log.lines().map(|line| line.split_once('|').unwrap().0).collect();
      let fetches: BTreeMap<_, _> = log
        .lines()
        .filter(|line| line.contains(" fetch "))
        .map(|line| (line.split_once('|').unwrap().0, line.split_whitespace().last().unwrap()))
        .collect();
      assert_eq!(
        directories,
        fetches.keys().copied().collect(),
        "Git may run only for the two selected source Jobs"
      );
      assert_eq!(fetches.len(), 2, "the no-code step must not materialize a checkout");
      assert_eq!(
        fetches.values().copied().collect::<std::collections::BTreeSet<_>>(),
        [
          code.as_ref().unwrap().revision.as_str(),
          second_code.as_ref().unwrap().revision.as_str()
        ]
        .into_iter()
        .collect()
      );
    }
    _ => (),
  }
  worker.kill().await.unwrap();
  worker.wait().await.unwrap();
  shutdown_server(&mut server, &log).await;
  server = spawn_server(&binary, &config, &log, &directory.join("server.err"));
  wait_for_server_ready(&client, &origin, &mut server, &log).await;
  assert_eq!(
    if let Some(authored) = &isolated {
      isolation::completed(&client, &origin, authored).await
    } else {
      verification::completed(&client, &origin, &run, locale.is_none(), code.as_ref()).await
    },
    retained
  );
  shutdown_server(&mut server, &log).await;
  if env::var_os("OCTACITY_RELEASE_EVIDENCE_DIR").is_some() {
    let agent_binary = required_path("OCTACITY_MANUAL_AGENT_BINARY", true);
    fs::write(
      evidence_root.join(match scenario {
        Scenario::Headless => "manual-authoring.json".to_owned(),
        Scenario::Console(locale) => format!("manual-console-{locale}.json"),
        Scenario::RepositoryFree => "manual-isolation-no-code.json".to_owned(),
        Scenario::Repositories => "manual-isolation-a-b.json".to_owned(),
        Scenario::Recovery => "manual-recovery.json".to_owned(),
      }),
      serde_json::to_vec_pretty(&json!({
        "locale":locale,
        "backend":required_string("OCTACITY_RELEASE_BACKEND"),
        "server_binary_sha256":FactoryDigest::content_sha256(&fs::read(&binary).unwrap()),
        "agent_binary_sha256":FactoryDigest::content_sha256(&fs::read(agent_binary).unwrap()),
        "after_restart":retained,
      }))
      .unwrap(),
    )
    .unwrap();
  }
  sqlx::query(sqlx::AssertSqlSafe(format!(
    "DROP DATABASE \"{database}\" WITH (FORCE)"
  )))
  .execute(&admin)
  .await
  .unwrap();
  admin.close().await;
  fs::remove_dir_all(directory).unwrap();
}
