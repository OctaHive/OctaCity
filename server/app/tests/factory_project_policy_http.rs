//! Policy management through the released HTTP composition and disposable PostgreSQL.
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};

#[allow(dead_code)]
#[path = "support/factory_rest_fixture.rs"]
mod installed_fixture;

#[allow(dead_code)]
#[path = "../../api/octacity-server-api-rest/tests/support/openapi.rs"]
mod openapi_contract;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres_support;
#[allow(dead_code)]
#[path = "support/runtime.rs"]
mod runtime_fixture;

struct Fixture {
  runtime: octacity_server::ServerRuntime,
  client: Client,
  project: String,
  database: postgres_support::TestDatabase,
  _directory: tempfile::TempDir,
}
impl Fixture {
  async fn start() -> Self {
    Self::with_installer(None).await
  }
  async fn with_installer(installer: Option<Arc<dyn octacity_server::FactoryRuntimeInstaller>>) -> Self {
    let database = postgres_support::TestDatabase::migrated().await;
    let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
    url.set_path(&database.name);
    let directory = tempfile::tempdir().unwrap();
    let extra = if installer.is_some() {
      "[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=5000\nbatch_size=4\nconcurrency=2"
    } else {
      ""
    };
    let config = octacity_server::ServerConfig::parse_toml(&runtime_fixture::runtime_config_source(
      directory.path(),
      url.as_str(),
      "http://127.0.0.1:19000",
      extra,
    ))
    .unwrap();
    let runtime = match installer {
      Some(installer) => octacity_server::ServerRuntime::start_with_factory(config, installer)
        .await
        .unwrap(),
      None => octacity_server::ServerRuntime::start(config).await.unwrap(),
    };
    let client = Client::new();
    let response = client
      .post(format!("http://{}/api/v1/projects", runtime.management_addr()))
      .header("idempotency-key", "policy-project")
      .json(&json!({"parent_id":null,"name":"Policy HTTP contract"}))
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let project = response.json::<Value>().await.unwrap()["resource"]["id"]
      .as_str()
      .unwrap()
      .to_owned();
    Self {
      runtime,
      client,
      project,
      database,
      _directory: directory,
    }
  }
  fn url(&self, suffix: &str) -> String {
    format!(
      "http://{}/api/v1/projects/{}/factory-policies{suffix}",
      self.runtime.management_addr(),
      self.project
    )
  }
  async fn post(&self, suffix: &str, key: &str, body: &Value, status: StatusCode) -> Value {
    let response = self
      .client
      .post(self.url(suffix))
      .header("idempotency-key", key)
      .json(body)
      .send()
      .await
      .unwrap();
    let actual = response.status();
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(actual, status, "{body}");
    body
  }
  async fn get(&self, suffix: &str, status: StatusCode) -> Value {
    let response = self.client.get(self.url(suffix)).send().await.unwrap();
    let actual = response.status();
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(actual, status, "{body}");
    body
  }
  async fn stop(self) {
    self.runtime.shutdown().await.unwrap();
    self.database.cleanup().await;
  }
  async fn restart(self) -> Self {
    self.restart_with_installer(None).await
  }
  async fn restart_with_installer(self, installer: Option<Arc<dyn octacity_server::FactoryRuntimeInstaller>>) -> Self {
    let Self {
      runtime,
      client,
      project,
      database,
      _directory,
    } = self;
    runtime.shutdown().await.unwrap();
    let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
    url.set_path(&database.name);
    let extra = if installer.is_some() {
      "[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=5000\nbatch_size=4\nconcurrency=2"
    } else {
      ""
    };
    let config = octacity_server::ServerConfig::parse_toml(&runtime_fixture::runtime_config_source(
      _directory.path(),
      url.as_str(),
      "http://127.0.0.1:19000",
      extra,
    ))
    .unwrap();
    let runtime = match installer {
      Some(installer) => octacity_server::ServerRuntime::start_with_factory(config, installer)
        .await
        .unwrap(),
      None => octacity_server::ServerRuntime::start(config).await.unwrap(),
    };
    Self {
      runtime,
      client,
      project,
      database,
      _directory,
    }
  }
}

struct PolicyAuthority(Arc<AtomicBool>);
#[async_trait::async_trait]
impl octacity_server_store_postgres::FactoryCapabilitySource for PolicyAuthority {
  async fn availability(
    &self,
    _: octacity_server_domain::ProjectId,
  ) -> Result<octacity_server_store::FactoryConfigurationAvailability, octacity_server_store::StoreError> {
    use octacity_server_factory::*;
    if !self.0.load(Ordering::SeqCst) {
      return Ok(octacity_server_store::FactoryConfigurationAvailability::Disabled);
    }
    let mut choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
      references: ["first", "second"]
        .into_iter()
        .enumerate()
        .map(|(i, alias)| FactoryReferenceChoice {
          kind: FactoryChoiceKind::PermissionCeiling,
          alias: alias.parse().unwrap(),
          reference: ImmutableReference::new(
            alias.parse().unwrap(),
            "v1".parse().unwrap(),
            FactoryDigest::from_bytes([i as u8 + 1; 32]),
          ),
        })
        .collect(),
      build_configurations: vec![],
    })
    .unwrap();
    for (alias, host) in [("first", "allowed.example"), ("second", "other.example")] {
      choices = choices
        .with_choice_metadata(
          FactoryChoiceKind::PermissionCeiling,
          alias.parse().unwrap(),
          FactoryChoiceMetadata {
            permission_declarations: Some(
              FactoryPermissionSet::try_new(FactoryPermissionDraft {
                network_hosts: vec![NetworkHost::new(host).unwrap()],
                plugins: (alias == "first").then(unavailable_plugin).into_iter().collect(),
                ..Default::default()
              })
              .unwrap(),
            ),
            enforcement: PermissionCategory::ALL.to_vec(),
            ..Default::default()
          },
        )
        .unwrap();
    }
    Ok(octacity_server_store::FactoryConfigurationAvailability::Available(
      choices,
    ))
  }
}

fn unavailable_plugin() -> octacity_server_factory::ImmutableReference {
  use octacity_server_factory::*;
  ImmutableReference::new(
    "missing-plugin".parse().unwrap(),
    "v1".parse().unwrap(),
    FactoryDigest::from_bytes([3; 32]),
  )
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn access_policy_http_rejects_unselectable_dependencies_even_inside_a_declared_ceiling() {
  use octacity_server_factory::*;
  let f = Fixture::with_installer(Some(Arc::new(PolicyAuthority(Arc::new(AtomicBool::new(true)))))).await;
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![unavailable_plugin()],
    ..Default::default()
  })
  .unwrap();
  let body = json!({"kind":"access","id":uuid::Uuid::new_v4().to_string(),"expected_current":null,"definition":{
    "name":"Unavailable tool","permissions":permissions
  }});
  for suffix in ["/validate", ""] {
    let error = f.post(suffix, "missing-tool", &body, StatusCode::BAD_REQUEST).await;
    assert_eq!(error["diagnostics"]["errors"][0]["field"], "/definition/permissions");
    assert!(!error.to_string().contains("missing-plugin"));
  }
  assert_eq!(f.get("", StatusCode::OK).await["items"], json!([]));
  f.stop().await;
}
#[async_trait::async_trait]
impl octacity_server::FactoryRuntimeInstaller for PolicyAuthority {
  async fn install(
    &self,
    resources: octacity_server::FactoryRuntimeResources,
  ) -> Result<octacity_server::FactoryRuntimeInstallation, octacity_server::FactoryRuntimeInstallationError> {
    let mut installed =
      octacity_server::FactoryRuntimeInstaller::install(&installed_fixture::Installation, resources).await?;
    installed.capabilities = Arc::new(PolicyAuthority(self.0.clone()));
    Ok(installed)
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn access_policy_http_qualification_never_unions_ceilings_and_replay_survives_revocation() {
  use octacity_server_factory::*;
  let authority = Arc::new(AtomicBool::new(true));
  let f = Fixture::with_installer(Some(Arc::new(PolicyAuthority(authority.clone())))).await;
  let permissions = |hosts: &[&str]| {
    serde_json::to_value(
      FactoryPermissionSet::try_new(FactoryPermissionDraft {
        network_hosts: hosts.iter().map(|host| NetworkHost::new(*host).unwrap()).collect(),
        ..Default::default()
      })
      .unwrap(),
    )
    .unwrap()
  };
  let mut body = json!({"kind":"access","id":uuid::Uuid::new_v4().to_string(),"expected_current":null,"definition":{
    "name":"Read service","permissions":permissions(&["allowed.example"])
  }});
  let checked = f.post("/validate", "unused", &body, StatusCode::OK).await;
  assert_eq!(checked["installed_ceiling"]["alias"], "first");
  assert_eq!(checked["installed_ceiling"]["reference"]["version"], "v1");
  assert!(checked["installed_ceiling"].get("permissions").is_none());
  assert!(!checked.to_string().contains("missing-plugin"));
  let mut visible = body.clone();
  visible["definition"]["permissions"] = permissions(&["other.example"]);
  let visible = f.post("/validate", "unused", &visible, StatusCode::OK).await;
  assert_eq!(visible["installed_ceiling"]["alias"], "second");
  assert_eq!(
    visible["installed_ceiling"]["permissions"]["network_hosts"],
    json!(["other.example"])
  );
  let saved = f.post("", "access", &body, StatusCode::OK).await;
  assert_eq!(saved["policy"], checked["policy"]);
  let r = &saved["policy"]["policy"]["reference"];
  assert_eq!(
    f.get(
      &format!(
        "/access/{}/versions/1?digest={}",
        r["id"].as_str().unwrap(),
        r["digest"].as_str().unwrap()
      ),
      StatusCode::OK
    )
    .await["policy"],
    saved["policy"]
  );
  let first_body = body.clone();
  body["id"] = json!(uuid::Uuid::new_v4().to_string());
  body["definition"]["permissions"] = permissions(&["allowed.example", "other.example"]);
  let rejected = f.post("/validate", "unused", &body, StatusCode::BAD_REQUEST).await;
  assert_eq!(rejected["diagnostics"]["errors"][0]["field"], "/definition/permissions");
  assert_eq!(rejected["diagnostics"]["errors"][0]["ceiling"], "installation");
  assert!(!rejected.to_string().contains("allowed.example"));
  f.post("", "retry-after-fix", &body, StatusCode::BAD_REQUEST).await;
  body["definition"]["permissions"] = permissions(&["allowed.example"]);
  f.post("", "retry-after-fix", &body, StatusCode::OK).await;
  authority.store(false, Ordering::SeqCst);
  let replay = f.post("", "access", &first_body, StatusCode::OK).await;
  assert_eq!(replay["policy"], saved["policy"]);
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["published_at"], saved["published_at"]);
  body["id"] = json!(uuid::Uuid::new_v4().to_string());
  f.post("", "revoked", &body, StatusCode::BAD_REQUEST).await;
  body["definition"]["permissions"] = permissions(&[]);
  f.post("", "deny-all", &body, StatusCode::OK).await;
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn policy_http_legacy_resolution_is_explicit_and_cannot_be_confused_with_project_references() {
  let f = Fixture::with_installer(Some(Arc::new(PolicyAuthority(Arc::new(AtomicBool::new(true)))))).await;
  let legacy = json!({"kind":"access","reference":{"identity":"first","version":"v1","digest":"01".repeat(32)}});
  assert_eq!(
    f.post("/legacy/resolve", "unused", &legacy, StatusCode::OK).await,
    json!({"mapping":null})
  );
  assert_eq!(f.get("/defaults", StatusCode::OK).await, json!({"defaults":null}));
  let intent = json!({"designated_access":legacy["reference"],"legacy_references":[legacy]});
  let document = octacity_server_api_rest::v1::openapi_document();
  openapi_contract::assert_json_matches_component(&document, "FactoryProjectPolicyProvisionDraft", &intent);
  f.post(
    "/provision",
    "duplicates",
    &json!({"legacy_references":[legacy, legacy]}),
    StatusCode::BAD_REQUEST,
  )
  .await;
  assert_eq!(f.get("/defaults", StatusCode::OK).await, json!({"defaults":null}));
  let saved = f.post("/provision", "initialize", &intent, StatusCode::OK).await;
  openapi_contract::assert_json_matches_component(&document, "FactoryProjectPolicyProvisionOutcome", &saved);
  assert_eq!(saved["provisioning"]["access_origin"], legacy["reference"]);
  assert_eq!(
    saved["defaults"]["access_profile"],
    saved["provisioning"]["standard_access"]
  );
  assert_eq!(f.get("/defaults", StatusCode::OK).await["defaults"], saved["defaults"]);
  let f = f.restart().await;
  let replay = f.post("/provision", "initialize", &intent, StatusCode::OK).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["defaults"], saved["defaults"]);
  assert_eq!(replay["mappings"], saved["mappings"]);
  f.post(
    "/provision",
    "initialize",
    &json!({"legacy_references":[]}),
    StatusCode::CONFLICT,
  )
  .await;
  assert_eq!(saved["mappings"][0]["legacy"], legacy);
  let mapped = f.post("/legacy/resolve", "unused", &legacy, StatusCode::OK).await;
  assert_eq!(mapped["mapping"], saved["mappings"][0]);
  assert_eq!(mapped["mapping"]["target"]["kind"], "access");
  assert_eq!(mapped["mapping"]["target"]["reference"]["project_id"], f.project);
  // Project pins and legacy installation references are explicitly different wire variants.
  f.post(
    "/legacy/resolve",
    "unused",
    &mapped["mapping"]["target"],
    StatusCode::BAD_REQUEST,
  )
  .await;
  let mut unknown = legacy.clone();
  unknown["reference"]["digest"] = json!("ff".repeat(32));
  assert_eq!(
    f.post("/legacy/resolve", "unused", &unknown, StatusCode::OK).await,
    json!({"mapping":null})
  );
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn policy_http_wire_contract_is_typed_and_preserves_project_isolation() {
  let mut f = Fixture::start().await;
  let body = json!({"kind":"launch","id":uuid::Uuid::new_v4().to_string(),"expected_current":null,"definition":{
    "name":"Private policy name","allow_manual":true,"allow_automatic_source":false
  }});
  let document = octacity_server_api_rest::v1::openapi_document();
  for name in [
    "FactoryProjectPolicyDraft",
    "FactoryProjectPolicyDetails",
    "FactoryProjectPolicyPublication",
    "FactoryProjectPolicyPage",
    "FactoryProjectPolicyDefaultsDraft",
    "FactoryProjectPolicyDefaultsRead",
    "FactoryProjectPolicyDefaultsPublication",
    "FactoryLegacyPolicyReference",
    "FactoryLegacyPolicyMappingRead",
  ] {
    openapi_contract::assert_component_exists(&document, name);
  }
  openapi_contract::assert_json_matches_component(&document, "FactoryProjectPolicyDraft", &body);
  let saved = f.post("", "original", &body, StatusCode::OK).await;
  openapi_contract::assert_json_matches_component(&document, "FactoryProjectPolicyPublication", &saved);
  let page = f.get("", StatusCode::OK).await;
  openapi_contract::assert_json_matches_component(&document, "FactoryProjectPolicyPage", &page);
  let r = &saved["policy"]["policy"]["reference"];
  let exact = format!(
    "/launch/{}/versions/1?digest={}",
    r["id"].as_str().unwrap(),
    r["digest"].as_str().unwrap()
  );
  let detail = f.get(&exact, StatusCode::OK).await;
  openapi_contract::assert_json_matches_component(&document, "FactoryProjectPolicyDetails", &detail);
  let owner = f.project.clone();
  let other = f
    .client
    .post(format!("http://{}/api/v1/projects", f.runtime.management_addr()))
    .header("idempotency-key", "other-project")
    .json(&json!({"parent_id":null,"name":"Other Project"}))
    .send()
    .await
    .unwrap()
    .json::<Value>()
    .await
    .unwrap();
  f.project = other["resource"]["id"].as_str().unwrap().to_owned();
  assert_eq!(f.get("", StatusCode::OK).await["items"], json!([]));
  let denied = f.get(&exact, StatusCode::NOT_FOUND).await;
  assert!(!denied.to_string().contains("Private policy name"));
  f.post(
    "/defaults",
    "foreign-default",
    &json!({"expected_version":null,"launch_policy":r,"access_profile":null}),
    StatusCode::BAD_REQUEST,
  )
  .await;
  let mut changed = body.clone();
  changed["expected_current"] = r.clone();
  f.post("", "foreign-replacement", &changed, StatusCode::BAD_REQUEST)
    .await;
  let mut ambiguous = body.clone();
  ambiguous["admission_policy"] = json!({"identity":"old","version":"v1","digest":"11".repeat(32)});
  f.post("/validate", "unused", &ambiguous, StatusCode::BAD_REQUEST).await;
  f.project = owner;
  assert_eq!(f.get(&exact, StatusCode::OK).await["policy"], saved["policy"]);
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn launch_policy_http_replacement_checks_exact_heads_and_replays_original_receipts() {
  let f = Fixture::start().await;
  let mut body = json!({"kind":"launch","id":uuid::Uuid::new_v4().to_string(),"expected_current":null,"definition":{
    "name":"Operator policy","allow_manual":true,"allow_automatic_source":false
  }});
  let original = f.post("", "first", &body, StatusCode::OK).await;
  let first_body = body.clone();
  body["expected_current"] = original["policy"]["policy"]["reference"].clone();
  body["definition"]["allow_manual"] = json!(false);
  let replacement = f.post("", "second", &body, StatusCode::OK).await;
  assert_eq!(replacement["policy"]["policy"]["reference"]["version"], 2);
  assert_eq!(replacement["policy"]["policy"]["definition"]["allow_manual"], false);
  f.post("", "stale", &body, StatusCode::PRECONDITION_FAILED).await;
  f.post("", "first", &body, StatusCode::CONFLICT).await;
  let f = f.restart().await;
  let replay = f.post("", "first", &first_body, StatusCode::OK).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["policy"], original["policy"]);
  assert_eq!(replay["published_at"], original["published_at"]);
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn policy_validation_is_read_only_and_checks_the_current_publication_precondition() {
  let f = Fixture::start().await;
  let body = json!({"kind":"launch","id":uuid::Uuid::new_v4().to_string(),"expected_current":null,"definition":{
    "name":"Paused","allow_manual":false,"allow_automatic_source":false
  }});
  let validated = f.post("/validate", "not-a-reservation", &body, StatusCode::OK).await;
  assert_eq!(validated["policy"]["policy"]["definition"], body["definition"]);
  let reference = &validated["policy"]["policy"]["reference"];
  let missing = f
    .client
    .get(f.url(&format!(
      "/launch/{}/versions/1?digest={}",
      body["id"].as_str().unwrap(),
      reference["digest"].as_str().unwrap()
    )))
    .send()
    .await
    .unwrap();
  assert_eq!(missing.status(), StatusCode::NOT_FOUND);
  let saved = f.post("", "not-a-reservation", &body, StatusCode::OK).await;
  assert_eq!(saved["policy"], validated["policy"]);
  f.post("/validate", "unused", &body, StatusCode::PRECONDITION_FAILED)
    .await;
  let mut bad = body.clone();
  bad["definition"]["credential_value"] = json!("private-canary-value");
  let error = f.post("/validate", "unused", &bad, StatusCode::BAD_REQUEST).await;
  assert!(!error.to_string().contains("private-canary-value"));
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn policy_http_catalog_and_defaults_preserve_exact_explicit_selections() {
  let f = Fixture::start().await;
  assert_eq!(f.get("/defaults", StatusCode::OK).await, json!({"defaults":null}));
  let mut references = Vec::new();
  for (index, id) in [
    "11111111-1111-4111-8111-111111111111",
    "22222222-2222-4222-8222-222222222222",
  ]
  .into_iter()
  .enumerate()
  {
    let body = json!({"kind":"launch","id":id,"expected_current":null,"definition":{
      "name":format!("Policy {index}"),"allow_manual":true,"allow_automatic_source":false
    }});
    let saved = f.post("", &format!("policy-{index}"), &body, StatusCode::OK).await;
    references.push(saved["policy"]["policy"]["reference"].clone());
  }
  let first = f.get("?limit=1", StatusCode::OK).await;
  assert_eq!(first["items"].as_array().unwrap().len(), 1);
  assert_eq!(first["items"][0]["name"], "Policy 0");
  assert!(first["items"][0].get("definition").is_none());
  let mut url = url::Url::parse(&f.url("")).unwrap();
  url
    .query_pairs_mut()
    .append_pair("limit", "1")
    .append_pair("after", &first["next_cursor"].to_string());
  let second = f.client.get(url).send().await.unwrap().json::<Value>().await.unwrap();
  assert_eq!(second["items"][0]["name"], "Policy 1");
  assert!(second["next_cursor"].is_null());
  f.get("?limit=0", StatusCode::BAD_REQUEST).await;
  let selection = json!({"expected_version":null,"launch_policy":references[1],"access_profile":null});
  let selected = f.post("/defaults", "defaults", &selection, StatusCode::OK).await;
  assert_eq!(selected["defaults"]["version"], 1);
  assert_eq!(
    f.get("/defaults", StatusCode::OK).await["defaults"],
    selected["defaults"]
  );
  let cleared = f
    .post(
      "/defaults",
      "clear",
      &json!({"expected_version":1,"launch_policy":null,"access_profile":null}),
      StatusCode::OK,
    )
    .await;
  assert_eq!(cleared["defaults"]["version"], 2);
  assert!(cleared["defaults"]["launch_policy"].is_null());
  f.post(
    "/defaults",
    "stale-defaults",
    &selection,
    StatusCode::PRECONDITION_FAILED,
  )
  .await;
  let replay = f.post("/defaults", "defaults", &selection, StatusCode::OK).await;
  assert_eq!(replay["defaults"], selected["defaults"]);
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(
    f.get("/defaults", StatusCode::OK).await["defaults"],
    cleared["defaults"]
  );
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn launch_policy_http_publication_is_readable_without_installation_or_execution() {
  let f = Fixture::start().await;
  let id = uuid::Uuid::new_v4();
  let body = json!({"kind":"launch","id":id.to_string(),"expected_current":null,"definition":{
    "name":"Manual review","allow_manual":true,"allow_automatic_source":false
  }});
  let response = f
    .client
    .post(f.url(""))
    .header("idempotency-key", "launch-create")
    .json(&body)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let saved = response.json::<Value>().await.unwrap();
  assert_eq!(saved["disposition"], "applied");
  assert_eq!(saved["policy"]["kind"], "launch");
  assert_eq!(saved["policy"]["policy"]["definition"], body["definition"]);
  let reference = &saved["policy"]["policy"]["reference"];
  assert_eq!(reference["project_id"], f.project);
  assert_eq!(reference["id"], id.to_string());
  assert_eq!(reference["version"], 1);
  let detail = f
    .client
    .get(f.url(&format!(
      "/launch/{id}/versions/1?digest={}",
      reference["digest"].as_str().unwrap()
    )))
    .send()
    .await
    .unwrap();
  assert_eq!(detail.status(), StatusCode::OK);
  assert_eq!(detail.json::<Value>().await.unwrap()["policy"], saved["policy"]);
  f.stop().await;
}

#[path = "factory_project_policy_http/configuration.rs"]
mod configuration;

#[path = "factory_project_policy_http/admission.rs"]
mod admission;
