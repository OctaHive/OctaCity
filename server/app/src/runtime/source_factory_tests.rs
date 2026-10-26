//! Enabled Factory intake through the same normal installation and protected API.
use serde_json::{Value, json};

pub(super) async fn assert_factory_intake(
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  project: &str,
  published: &Value,
  provider: &std::path::Path,
) {
  let base = format!("http://localhost/api/v1/projects/{project}");
  let tasks_before: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let canonical = tasks_before["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|i| i["title"] == "External task draft")
    .unwrap()["id"]
    .clone();
  let mode_path = provider.join("mode.json");
  let original = std::fs::read(&mode_path).unwrap();
  let mut mode: Value = serde_json::from_slice(&original).unwrap();
  mode["candidates"][0]["body"] = json!("Reviewed external acceptance criteria");
  let mut no_card = mode["candidates"][0].clone();
  no_card["item_id"] = json!("execution-without-card");
  no_card["display_id"] = json!("EXECUTION-ONLY-1");
  no_card["title"] = json!("Execution without an Issue");
  mode["candidates"].as_array_mut().unwrap().push(no_card);
  std::fs::write(&mode_path, serde_json::to_vec(&mode).unwrap()).unwrap();
  let mut works = std::collections::BTreeSet::new();
  let mut canonical_works = Vec::new();
  let mut no_card_works = Vec::new();
  let mut native = std::collections::BTreeSet::new();
  let mut diagnostics_path = String::new();
  let mut retained = std::collections::BTreeSet::new();
  for index in 0..2 {
    let id = uuid::Uuid::new_v4().to_string();
    retained.insert((id.clone(), 1));
    retained.insert((id.clone(), 2));
    let mut intent = published["intent"].clone();
    intent["inputs"][0]["id"] = json!(id);
    intent["inputs"][0]["definition"]["enabled"] = json!(true);
    let path = format!("{base}/factory-source-inputs/{id}/versions");
    diagnostics_path = format!("{path}/1/diagnostics");
    let response = client
      .post(&path)
      .header("idempotency-key", format!("normal-factory-intake.{index}"))
      .header("if-none-match", "*")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      reqwest::StatusCode::OK,
      "{}",
      response.text().await.unwrap()
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    let diagnostics = loop {
      let diagnostics: Value = client
        .get(format!("{path}/1/diagnostics"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
      if diagnostics["admissions"].as_array().unwrap().len() == 2 {
        break diagnostics;
      }
      assert!(
        tokio::time::Instant::now() < deadline,
        "enabled Factory input must admit both native candidates through normal startup: {diagnostics}"
      );
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    };
    let admissions = diagnostics["admissions"].as_array().unwrap();
    assert_paged_diagnostics(client, &diagnostics_path, admissions).await;
    assert_eq!(
      admissions
        .iter()
        .filter(|a| a["issue"]["issue_id"] == canonical)
        .count(),
      1,
      "each distinct input resolves the original imported Issue: {diagnostics}"
    );
    assert_eq!(
      admissions.iter().filter(|a| a.get("issue").is_none()).count(),
      1,
      "execution-only native items require no invented Issue: {diagnostics}"
    );
    for admission in admissions {
      assert!(admission["association"]["run_id"].is_string());
      let work_response = client
        .get(format!(
          "http://localhost/api/v1/factory-runs/{}/work",
          admission["association"]["run_id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
      assert_eq!(work_response.status(), reqwest::StatusCode::OK);
      let work: Value = work_response.json().await.unwrap();
      assert_eq!(work["items"].as_array().unwrap().len(), 1);
      let work = &work["items"][0]["record"];
      assert_eq!(work["id"], admission["association"]["work_id"]);
      assert_eq!(
        work["configuration"],
        published["intent"]["inputs"][0]["definition"]["target"]
      );
      assert_eq!(
        work["subject"]["repository_id"], published["intent"]["inputs"][0]["definition"]["repository"]["id"],
        "execution uses the separately reviewed code Repository, not the native task container"
      );
      assert_eq!(work["subject"]["base_revision"], "a".repeat(40));
      assert_eq!(admission["reporting"], json!([]));
      assert!(
        works.insert(admission["association"]["work_id"].as_str().unwrap().to_owned()),
        "separate input identities require separate Work envelopes"
      );
      let owned = if admission.get("issue").is_some() {
        &mut canonical_works
      } else {
        &mut no_card_works
      };
      owned.push(admission["association"]["work_id"].as_str().unwrap().to_owned());
      native.insert(
        admission["association"]["native_correlation"]
          .as_str()
          .unwrap()
          .to_owned(),
      );
    }
    intent["inputs"][0]["expected_version"] = json!(1);
    intent["inputs"][0]["definition"]["enabled"] = json!(false);
    let response = client
      .post(&path)
      .header("idempotency-key", format!("stop-normal-factory-intake.{index}"))
      .header("if-match", "\"1\"")
      .json(&json!({"intent":intent}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let replacement: Value = client
      .get(format!("{path}/2/diagnostics"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    assert_eq!(
      replacement["admissions"],
      json!([]),
      "a disabled replacement with no admission cannot display the previous version's Work/Run as its own"
    );
  }
  assert_eq!(works.len(), 4);
  retained.insert((published["input"].as_str().unwrap().to_owned(), 1));
  assert_retained_dependencies(client, project, published, &retained).await;
  // Only this journey's disposable DB: a missing authoritative task-link read
  // must be unavailable, not a successful empty association after admission.
  sqlx::query("ALTER TABLE issue_source_links RENAME TO interrupted_issue_source_links")
    .execute(pool)
    .await
    .unwrap();
  let unavailable = client.get(&diagnostics_path).send().await;
  sqlx::query("ALTER TABLE interrupted_issue_source_links RENAME TO issue_source_links")
    .execute(pool)
    .await
    .unwrap();
  assert_eq!(
    unavailable.unwrap().status(),
    reqwest::StatusCode::SERVICE_UNAVAILABLE,
    "Source diagnostics cannot hide an authoritative Issue association outage"
  );
  let recovered: Value = client
    .get(&diagnostics_path)
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    recovered["admissions"]
      .as_array()
      .unwrap()
      .iter()
      .filter(|row| row.get("issue").is_some())
      .count(),
    1
  );
  assert_eq!(
    native.len(),
    2,
    "native identity is independent of per-input Work identity"
  );
  assert!(
    !provider.join("reporter-requests.jsonl").exists(),
    "read-only intake requires no writer"
  );
  let tasks_after: Value = client
    .get(format!("{base}/issues"))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(
    tasks_after, tasks_before,
    "Factory intake cannot implicitly import or edit an Issue"
  );
  wait_associations(client, &base, canonical.as_str().unwrap(), &canonical_works).await;
  assert_late_import(client, &base, published, &no_card_works).await;
  assert_retired_run_does_not_consume_a_diagnostic_page(client, pool, &diagnostics_path).await;
  std::fs::write(mode_path, original).unwrap();
}

async fn assert_retired_run_does_not_consume_a_diagnostic_page(
  client: &reqwest::Client,
  pool: &sqlx::PgPool,
  path: &str,
) {
  let before: Value = client.get(path).send().await.unwrap().json().await.unwrap();
  let admissions = before["admissions"].as_array().unwrap();
  assert_eq!(admissions.len(), 2);
  let retired: uuid::Uuid = admissions[0]["association"]["run_id"]
    .as_str()
    .unwrap()
    .parse()
    .unwrap();
  let surviving = &admissions[1]["association"]["run_id"];
  // Simulate the retained-hidden phase already qualified by retention contracts;
  // this slice qualifies the public Source read, not the retention worker itself.
  sqlx::query("UPDATE factory_runs SET visible=false,hidden_at=now() WHERE id=$1")
    .bind(retired)
    .execute(pool)
    .await
    .unwrap();
  let page: Value = client
    .get(history_page_url(path, None))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(page["admissions"].as_array().unwrap().len(), 1);
  assert_eq!(
    &page["admissions"][0]["association"]["run_id"], surviving,
    "retained-hidden Runs cannot appear in Source diagnostics or consume their bounded page"
  );
  assert!(page["next_cursor"].is_null());
}

async fn assert_paged_diagnostics(client: &reqwest::Client, path: &str, admissions: &[Value]) {
  let first: Value = client
    .get(history_page_url(path, None))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(first["admissions"].as_array().unwrap().len(), 1);
  let cursor = first["next_cursor"].as_str().unwrap();
  let second: Value = client
    .get(history_page_url(path, Some(cursor)))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(second["admissions"].as_array().unwrap().len(), 1);
  assert!(second["next_cursor"].is_null());
  let expected: std::collections::BTreeSet<_> = admissions
    .iter()
    .map(|row| row["association"]["id"].as_str().unwrap())
    .collect();
  assert_eq!(
    std::collections::BTreeSet::from([
      first["admissions"][0]["association"]["id"].as_str().unwrap(),
      second["admissions"][0]["association"]["id"].as_str().unwrap(),
    ]),
    expected
  );
  assert!(!first.to_string().contains("Reviewed external acceptance criteria"));
  let other_version = path.replace("/versions/1/", "/versions/2/");
  assert_eq!(
    client
      .get(history_page_url(&other_version, Some(cursor)))
      .send()
      .await
      .unwrap()
      .status(),
    reqwest::StatusCode::BAD_REQUEST,
    "diagnostic cursors cannot cross immutable input versions"
  );
}

async fn assert_retained_dependencies(
  client: &reqwest::Client,
  project: &str,
  published: &Value,
  required: &std::collections::BTreeSet<(String, u64)>,
) {
  let connection = published["intent"]["inputs"][0]["definition"]["connection"]["id"]
    .as_str()
    .unwrap();
  let path = format!("http://localhost/api/v1/integrations/{connection}/projects/{project}/dependencies");
  let mut after = None;
  let mut seen = std::collections::BTreeSet::new();
  for _ in 0..16 {
    let response = client
      .get(history_page_url(&path, after.as_deref()))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let page: Value = response.json().await.unwrap();
    assert!(page["items"].as_array().unwrap().len() <= 1);
    for record in page["items"].as_array().unwrap() {
      assert_eq!(record["kind"], "input");
      let input = &record["definition"];
      assert_eq!(input["definition"]["connection"]["id"], connection);
      assert!(seen.insert((
        input["id"].as_str().unwrap().to_owned(),
        input["version"].as_u64().unwrap()
      )));
    }
    after = page["next_cursor"].as_str().map(str::to_owned);
    if after.is_none() {
      break;
    }
  }
  assert!(after.is_none(), "retained dependency fixture must drain bounded pages");
  assert!(
    required.is_subset(&seen),
    "disabled and previous immutable inputs stay visible: {seen:?}"
  );
}

fn history_page_url(path: &str, after: Option<&str>) -> url::Url {
  let mut url = url::Url::parse(path).unwrap();
  url.query_pairs_mut().append_pair("limit", "1");
  if let Some(after) = after {
    url.query_pairs_mut().append_pair("after", after);
  }
  url
}

pub(super) async fn assert_disabled_profile_is_ignored(
  config: &crate::ServerConfig,
  pool: sqlx::PgPool,
  published: &Value,
) {
  use super::work_source::FactorySourceProfileProvider as _;
  let installed = super::integration_management::install(
    config,
    std::sync::Arc::new(octacity_server_store_postgres::PostgresStore::new(pool)),
    tokio_util::sync::CancellationToken::new(),
    &mut super::tests::healthy_checks(),
  )
  .await
  .unwrap()
  .unwrap();
  let sources = installed.source_management.unwrap();
  let store = sources.store.clone();
  let provider = super::managed_factory_source::ManagedFactorySource::new(
    sources,
    &config.integration_management().unwrap().principal,
  );
  let mut enabled = published["publication"]["inputs"][0]["input"].clone();
  enabled["definition"]["enabled"] = json!(true);
  let mut disabled = enabled.clone();
  disabled["id"] = json!(uuid::Uuid::new_v4().to_string());
  disabled["definition"]["enabled"] = json!(false);
  disabled["definition"]["connection"] = Value::Null;
  disabled["definition"]["source"]["configuration"]["digest"] = json!("c".repeat(64));
  let profiles = provider
    .profiles(&[
      serde_json::from_value(disabled).unwrap(),
      serde_json::from_value(enabled.clone()).unwrap(),
    ])
    .await
    .expect("a disabled unavailable member cannot block the enabled installed profile");
  assert_eq!(profiles.len(), 1);
  assert_eq!(
    serde_json::to_value(&profiles[0].read.binding).unwrap(),
    enabled["definition"]["source"]
  );
  assert_legacy_target_is_not_routed(&provider, store.as_ref(), serde_json::from_value(enabled).unwrap()).await;
}

async fn assert_legacy_target_is_not_routed(
  provider: &super::managed_factory_source::ManagedFactorySource,
  store: &octacity_server_store_postgres::PostgresStore,
  template: octacity_server_factory::WorkSourceInput,
) {
  use super::work_source::FactorySourceProfileProvider as _;
  use octacity_server_factory::*;
  use octacity_server_store::*;
  let mut definition = template.definition().clone();
  definition.connection = None;
  let selected = store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: template.project_id(),
      configuration_id: definition.target.id(),
      configuration_version: definition.target.version(),
      repository_id: definition.repository.id,
      repository_version: definition.repository.version,
    })
    .await
    .unwrap();
  let capability = WorkSourceCapability {
    project_id: template.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: definition.source.clone(),
  };
  let repository = definition.repository.clone();
  let context = WorkSourceInputContext {
    target: &selected.configuration.configuration,
    repository: &repository,
    source: Some(&capability),
    reporter: None,
  };
  let legacy = WorkSourceInput::publish(
    WorkSourceInputId::generate(),
    template.project_id(),
    definition.clone(),
    context,
  )
  .unwrap();
  store
    .publish_work_sources(PublishWorkSources {
      project_id: template.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: legacy.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  let targets = provider.targets(64).await.unwrap();
  assert!(
    targets.iter().all(|(_, t)| *t
      != WorkSourceScanTarget::Input {
        id: legacy.id(),
        version: legacy.version()
      }),
    "managed discovery must leave legacy unpinned targets to their explicit owner before claims"
  );
  definition.enabled = false;
  let disabled = legacy
    .replace(
      WorkSourceInputVersion::new(2).unwrap(),
      definition,
      WorkSourceInputContext {
        target: &selected.configuration.configuration,
        repository: &repository,
        source: Some(&capability),
        reporter: None,
      },
    )
    .unwrap();
  store
    .publish_work_sources(PublishWorkSources {
      project_id: template.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: Some(legacy.version()),
        input: disabled,
      }],
      group: None,
    })
    .await
    .unwrap();
}

async fn wait_associations(client: &reqwest::Client, base: &str, issue: &str, works: &[String]) {
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
  loop {
    let response = client
      .get(format!("{base}/issues/{issue}/legacy-work"))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let history: Value = response.json().await.unwrap();
    let items = history["items"].as_array().unwrap();
    if works
      .iter()
      .all(|w| items.iter().any(|a| a["work_id"].as_str() == Some(w.as_str())))
    {
      return;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "explicit native task must converge to its original per-input Work associations: {history}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  }
}

async fn assert_late_import(client: &reqwest::Client, base: &str, published: &Value, works: &[String]) {
  let connection = &published["intent"]["inputs"][0]["definition"]["connection"];
  let choices: Value = client
    .get(format!(
      "{base}/issue-source-choices?connection_id={}&connection_version={}",
      connection["id"].as_str().unwrap(),
      connection["version"].as_u64().unwrap()
    ))
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  let id = uuid::Uuid::new_v4().to_string();
  let mut intent = json!({"id":id,"project_id":published["intent"]["project_id"],"expected_current_version":null,
    "definition":{"source":choices["items"][0]["binding"],"connection":connection,"enabled":true}});
  let path = format!("{base}/issue-source-imports/{id}/versions");
  let response = client
    .post(&path)
    .header("idempotency-key", "late-import-after-execution")
    .header("if-none-match", "*")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
  let issue = loop {
    let tasks: Value = client
      .get(format!("{base}/issues"))
      .send()
      .await
      .unwrap()
      .json()
      .await
      .unwrap();
    if let Some(issue) = tasks["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|t| t["title"] == "Execution without an Issue")
    {
      break issue["id"].as_str().unwrap().to_owned();
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "later explicit import must create the task draft"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
  };
  wait_associations(client, base, &issue, works).await;
  intent["expected_current_version"] = json!(1);
  intent["definition"]["enabled"] = json!(false);
  let response = client
    .post(&path)
    .header("idempotency-key", "stop-late-import")
    .header("if-match", "\"1\"")
    .json(&json!({"intent":intent}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
}
