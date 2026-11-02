use super::*;

struct ConfigurationAuthority(Arc<AtomicBool>);
#[async_trait::async_trait]
impl octacity_server_store::FactoryCapabilitySource for ConfigurationAuthority {
  async fn availability(
    &self,
    project: octacity_server_domain::ProjectId,
  ) -> Result<octacity_server_store::FactoryConfigurationAvailability, octacity_server_store::StoreError> {
    use octacity_server_factory::*;
    let octacity_server_store::FactoryConfigurationAvailability::Available(choices) =
      octacity_server_store::FactoryCapabilitySource::availability(&installed_fixture::Installation, project).await?
    else {
      unreachable!()
    };
    Ok(octacity_server_store::FactoryConfigurationAvailability::Available(
      choices
        .with_choice_metadata(
          FactoryChoiceKind::PermissionCeiling,
          "permissions".parse().unwrap(),
          FactoryChoiceMetadata {
            permission_declarations: Some(
              serde_json::from_value(host_permissions(if self.0.load(Ordering::SeqCst) {
                &["allowed.example"]
              } else {
                &[]
              }))
              .unwrap(),
            ),
            enforcement: PermissionCategory::ALL.to_vec(),
            ..Default::default()
          },
        )
        .unwrap(),
    ))
  }
}
#[async_trait::async_trait]
impl octacity_server::FactoryRuntimeInstaller for ConfigurationAuthority {
  async fn install(
    &self,
    resources: octacity_server::FactoryRuntimeResources,
  ) -> Result<octacity_server::FactoryRuntimeInstallation, octacity_server::FactoryRuntimeInstallationError> {
    let mut installation =
      octacity_server::FactoryRuntimeInstaller::install(&installed_fixture::Installation, resources).await?;
    installation.capabilities = Arc::new(Self(self.0.clone()));
    Ok(installation)
  }
}
fn host_permissions(hosts: &[&str]) -> Value {
  use octacity_server_factory::*;
  serde_json::to_value(
    FactoryPermissionSet::try_new(FactoryPermissionDraft {
      network_hosts: hosts.iter().map(|h| NetworkHost::new(*h).unwrap()).collect(),
      ..Default::default()
    })
    .unwrap(),
  )
  .unwrap()
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn configuration_constraints_explain_project_access_and_reject_revoked_authority() {
  let authority = Arc::new(AtomicBool::new(true));
  let f = Fixture::with_installer(Some(Arc::new(ConfigurationAuthority(authority.clone())))).await;
  let (launch, _) = policies(&f).await;
  let access = f
    .post(
      "",
      "network",
      &json!({"kind":"access","id":uuid::Uuid::new_v4().to_string(),
    "definition":{"name":"One host","permissions":host_permissions(&["allowed.example"])}}),
      StatusCode::OK,
    )
    .await;
  let candidate = json!({"id":uuid::Uuid::new_v4().to_string(),"project_id":f.project,"definition":{
    "admission_policy":{"project_policy":launch},"permission_ceiling":{"project_policy":access["policy"]["policy"]["reference"]},
    "hard_budget":installed_fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8},
    "enabled":true,"flow":flow_with_permissions(&f, host_permissions(&["allowed.example","excluded.example"])).await}});
  let base = format!("http://{}/api/v1/factory-configurations", f.runtime.management_addr());
  let response = f
    .client
    .post(format!("{base}/validate"))
    .json(&candidate)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let checked = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::OK, "{checked}");
  openapi_contract::assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "FactoryConfigurationValidation",
    &checked,
  );
  let effective = &checked["diagnostics"]["effective_permissions"][0];
  assert_eq!(effective["permissions"]["network_hosts"], json!(["allowed.example"]));
  assert_eq!(
    effective["requested"]["network_hosts"],
    json!(["allowed.example", "excluded.example"])
  );
  assert!(
    effective["sources"]
      .as_array()
      .unwrap()
      .contains(&json!("project_policy"))
  );
  let response = f
    .client
    .post(&base)
    .header("idempotency-key", "before-revocation")
    .json(&candidate)
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let saved = response.json::<Value>().await.unwrap();
  authority.store(false, Ordering::SeqCst);
  let response = f
    .client
    .post(format!("{base}/validate"))
    .json(&candidate)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let rejected = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
  assert_eq!(rejected["diagnostics"]["errors"][0]["field"], "/permission_ceiling");
  assert_eq!(
    rejected["diagnostics"]["errors"][0]["ceiling"],
    "installed_runtime_policy"
  );
  openapi_contract::assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "FactoryValidationDiagnostics",
    &rejected["diagnostics"],
  );
  assert!(!rejected.to_string().contains("allowed.example"));
  let report = readiness(&f, &saved["resource"]["configuration"]).await;
  assert!(
    report["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "project_policies"
        && c["state"] == "blocked"
        && c["reason"] == "permissions_unavailable"),
    "saved contents do not prove current authority: {report}"
  );
  f.stop().await;
}

async fn policies(f: &Fixture) -> (Value, Value) {
  let launch = f
    .post(
      "",
      "launch",
      &json!({"kind":"launch","id":uuid::Uuid::new_v4().to_string(),
    "definition":{"name":"Manual review","allow_manual":true,"allow_automatic_source":false}}),
      StatusCode::OK,
    )
    .await;
  let access = f
    .post(
      "",
      "access",
      &json!({"kind":"access","id":uuid::Uuid::new_v4().to_string(),
    "definition":{"name":"No grants","permissions":installed_fixture::permissions()}}),
      StatusCode::OK,
    )
    .await;
  (
    launch["policy"]["policy"]["reference"].clone(),
    access["policy"]["policy"]["reference"].clone(),
  )
}

async fn configuration(f: &Fixture, launch: Value, access: Value) -> Value {
  let response = f
    .client
    .post(format!(
      "http://{}/api/v1/factory-configurations",
      f.runtime.management_addr()
    ))
    .header("idempotency-key", "review-configuration")
    .json(&json!({"id":uuid::Uuid::new_v4().to_string(),
      "project_id":f.project,"definition":{"admission_policy":launch,"permission_ceiling":access,
        "hard_budget":installed_fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8},
        "enabled":true,"flow":flow(f).await}}))
    .send()
    .await
    .unwrap();
  let status = response.status();
  let publication = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::CREATED, "{publication}");
  publication["resource"]["configuration"].clone()
}

async fn readiness(f: &Fixture, configuration: &Value) -> Value {
  let base = format!("http://{}/api/v1", f.runtime.management_addr());
  let response = f
    .client
    .post(format!("{base}/repositories"))
    .header("idempotency-key", "repository")
    .json(&json!({"project_id":f.project,"name":"Code","definition":{
      "vcs_integration_id":uuid::Uuid::new_v4().to_string(),"repository_locator":"https://example.test/source.git",
      "selection":{"allowed_references":[],"default_reference":null,"allow_exact_revision":true}}}))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let repository = response.json::<Value>().await.unwrap();
  let reference = &configuration["reference"];
  let response = f.client.post(format!("{base}/factory-configurations/{}/versions/{}/readiness",
    reference["id"].as_str().unwrap(), reference["version"].as_u64().unwrap()))
    .json(&json!({"repository":{"project_id":f.project,"id":repository["resource"]["id"],"version":1,
      "definition_digest":octacity_server_factory::FactoryDigest::content_sha256(
        &serde_json::to_vec(&serde_json::from_value::<octacity_server_store::RepositoryDefinition>(repository["resource"]["definition"].clone()).unwrap()).unwrap())}}))
    .send().await.unwrap();
  let status = response.status();
  let report = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::OK, "{report}");
  report
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn readiness_observes_exact_project_policy_contents_without_installation_aliases() {
  let f = Fixture::with_installer(Some(Arc::new(installed_fixture::Installation))).await;
  let (launch, access) = policies(&f).await;
  let configuration = configuration(&f, json!({"project_policy":launch}), json!({"project_policy":access})).await;
  let report = readiness(&f, &configuration).await;
  assert!(
    report["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "project_policies" && c["state"] == "ready" && c["reason"] == "retained"),
    "{report}"
  );
  assert!(
    report["checks"]
      .as_array()
      .unwrap()
      .iter()
      .any(|c| c["aspect"] == "installation" && c["state"] == "ready"),
    "Project pins must not be looked up as installed aliases: {report}"
  );
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn configuration_choices_expose_exact_project_defaults_without_publishing_or_repinning() {
  let f = Fixture::with_installer(Some(Arc::new(installed_fixture::Installation))).await;
  let (launch, access) = policies(&f).await;
  let defaults = f
    .post(
      "/defaults",
      "defaults",
      &json!({"expected_version":null,
    "launch_policy":launch,"access_profile":access}),
      StatusCode::OK,
    )
    .await;
  let response = f
    .client
    .get(format!(
      "http://{}/api/v1/projects/{}/factory-configuration-choices",
      f.runtime.management_addr(),
      f.project
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let choices = response.json::<Value>().await.unwrap();
  assert_eq!(
    choices["project_policy_defaults"], defaults["defaults"],
    "new drafts must receive the Project's exact selected versions"
  );
  let read = f.get("/defaults", StatusCode::OK).await;
  assert_eq!(
    read["defaults"], defaults["defaults"],
    "choice reads must not publish defaults"
  );
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn legacy_configuration_publication_requires_exact_mapping_and_pins_its_content_without_default_fallback() {
  use octacity_server_store::{
    FactoryCapabilitySource, FactoryProjectPolicyStore, FactoryProjectProvisionIntent, ProvisionFactoryProjectPolicies,
  };
  let authority = Arc::new(ConfigurationAuthority(Arc::new(AtomicBool::new(false))));
  let f = Fixture::with_installer(Some(authority.clone())).await;
  let candidate = json!({"id":uuid::Uuid::new_v4().to_string(),"project_id":f.project,"definition":{
    "admission_policy":"admission","permission_ceiling":"permissions","enabled":true,
    "flow":flow(&f).await,"hard_budget":installed_fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8}}});
  let url = format!("http://{}/api/v1/factory-configurations", f.runtime.management_addr());
  let response = f
    .client
    .post(&url)
    .header("idempotency-key", "legacy-configuration")
    .json(&candidate)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let rejected = response.json::<Value>().await.unwrap();
  assert_eq!(
    status,
    StatusCode::BAD_REQUEST,
    "unmapped aliases cannot fall back to Standard: {rejected}"
  );
  assert_eq!(rejected["diagnostics"]["errors"][0]["code"], "unmapped_legacy_policy");
  openapi_contract::assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "FactoryValidationDiagnostics",
    &rejected["diagnostics"],
  );
  let project = f.project.parse().unwrap();
  let octacity_server_store::FactoryConfigurationAvailability::Available(choices) =
    authority.availability(project).await.unwrap()
  else {
    unreachable!()
  };
  let legacy_references = choices
    .entries()
    .references
    .into_iter()
    .filter_map(|c| match c.kind {
      octacity_server_factory::FactoryChoiceKind::AdmissionPolicy => {
        Some(octacity_server_store::FactoryLegacyPolicyReference::Launch(c.reference))
      }
      octacity_server_factory::FactoryChoiceKind::PermissionCeiling => {
        Some(octacity_server_store::FactoryLegacyPolicyReference::Access(c.reference))
      }
      _ => None,
    })
    .collect();
  let store =
    octacity_server_store_postgres::PostgresStore::with_factory_capabilities(f.database.pool.clone(), authority);
  let provisioned = store
    .provision_factory_project_policies(octacity_server_store::testing::management_mutation(
      ProvisionFactoryProjectPolicies {
        intent: FactoryProjectProvisionIntent {
          project_id: project,
          designated_access: None,
          legacy_references,
        },
        idempotency_key: "provision".parse().unwrap(),
        provisioned_at: octacity_server_domain::Timestamp::from_unix_millis(10).unwrap(),
      },
    ))
    .await
    .unwrap();
  assert!(provisioned.unresolved.is_empty());
  let response = f
    .client
    .post(&url)
    .header("idempotency-key", "legacy-configuration")
    .json(&candidate)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let saved = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::CREATED, "{saved}");
  for mapping in provisioned.mappings {
    let (field, expected) = match mapping.target {
      octacity_server_store::FactoryProjectPolicyRef::Launch(r) => {
        ("admission_policy", serde_json::to_value(r).unwrap())
      }
      octacity_server_store::FactoryProjectPolicyRef::Access(r) => {
        ("permission_ceiling", serde_json::to_value(r).unwrap())
      }
    };
    assert_eq!(saved["resource"]["configuration"][field]["project_policy"], expected);
  }
  f.stop().await;
}

async fn flow(f: &Fixture) -> Value {
  flow_with_permissions(f, installed_fixture::permissions()).await
}

async fn flow_with_permissions(f: &Fixture, permissions: Value) -> Value {
  let (boolean, _) = installed_fixture::contracts();
  let mut node = installed_fixture::node(
    "review",
    "trusted_action",
    &boolean["reference"],
    &boolean["reference"],
    "done",
  );
  node["action"] = json!({"plugin":{"identity":"fixture.observer","version":"v1","digest":"03".repeat(32)},"action":"observe","parameters":{}});
  node["permissions"] = permissions.clone();
  let mut draft = installed_fixture::draft(
    "review",
    vec![node],
    json!([installed_fixture::transition("review", "done", "terminal", "finished")]),
    "finished",
    &boolean["reference"],
    vec![boolean.clone()],
  );
  draft["root"]["execution"]["permissions"] = permissions.clone();
  draft["limits"]["permissions"] = permissions;
  let response = f
    .client
    .post(format!(
      "http://{}/api/v1/flow-definitions",
      f.runtime.management_addr()
    ))
    .header("idempotency-key", "policy-flow")
    .json(&json!({"id":uuid::Uuid::new_v4().to_string(),"project_id":f.project,"definition":draft}))
    .send()
    .await
    .unwrap();
  let status = response.status();
  let flow = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::CREATED, "{flow}");
  flow["resource"]["flow"].clone()
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn persisted_configuration_and_admission_context_keep_policy_overrides_after_default_replacement_and_restart() {
  use octacity_server_store::{FactoryAdmissionStore, ReadFactoryAdmissionContext};
  let f = Fixture::with_installer(Some(Arc::new(installed_fixture::Installation))).await;
  let (launch, access) = policies(&f).await;
  f.post(
    "/defaults",
    "defaults",
    &json!({"expected_version":null,
    "launch_policy":launch,"access_profile":access}),
    StatusCode::OK,
  )
  .await;
  let alternative = f
    .post(
      "",
      "alternative",
      &json!({"kind":"launch","id":uuid::Uuid::new_v4().to_string(),
    "definition":{"name":"Automatic only","allow_manual":false,"allow_automatic_source":true}}),
      StatusCode::OK,
    )
    .await;
  let alternative = &alternative["policy"]["policy"]["reference"];
  let candidate = json!({"id":uuid::Uuid::new_v4().to_string(),"project_id":f.project,"definition":{
    "admission_policy":{"project_policy":alternative},"permission_ceiling":{"project_policy":access},
    "hard_budget":installed_fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8},
    "enabled":true,"flow":flow(&f).await
  }});
  let response = f
    .client
    .post(format!(
      "http://{}/api/v1/factory-configurations",
      f.runtime.management_addr()
    ))
    .header("idempotency-key", "policy-configuration")
    .json(&candidate)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let publication = response.json::<Value>().await.unwrap();
  assert_eq!(status, StatusCode::CREATED, "{publication}");
  openapi_contract::assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "FactoryConfigurationMutationResponse",
    &publication,
  );
  let configuration = &publication["resource"]["configuration"];
  assert_eq!(
    configuration["admission_policy"]["project_policy"], *alternative,
    "explicit override beats defaults"
  );
  let replacement = f
    .post(
      "",
      "launch-v2",
      &json!({"kind":"launch","id":alternative["id"],"expected_current":alternative,
    "definition":{"name":"Stopped","allow_manual":false,"allow_automatic_source":false}}),
      StatusCode::OK,
    )
    .await;
  f.post(
    "/defaults",
    "defaults-v2",
    &json!({"expected_version":1,
    "launch_policy":replacement["policy"]["policy"]["reference"],"access_profile":access}),
    StatusCode::OK,
  )
  .await;
  let f = f.restart().await;
  let id = configuration["reference"]["id"].as_str().unwrap();
  let response = f
    .client
    .get(format!(
      "http://{}/api/v1/factory-configurations/{id}/versions/1",
      f.runtime.management_addr()
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.json::<Value>().await.unwrap()["configuration"], *configuration);
  let store = octacity_server_store_postgres::PostgresStore::new(f.database.pool.clone());
  let context = store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: f.project.parse().unwrap(),
      configuration_id: id.parse().unwrap(),
      configuration_version: octacity_server_factory::FactoryConfigurationVersion::INITIAL,
      repository_id: None,
      repository_version: None,
    })
    .await
    .unwrap();
  let policies = context
    .project_policies
    .expect("admission must resolve the exact saved versions");
  assert_eq!(
    serde_json::to_value(policies.launch_policy.reference()).unwrap(),
    *alternative
  );
  assert_eq!(
    serde_json::to_value(policies.access_profile.reference()).unwrap(),
    access
  );
  assert!(!policies.launch_policy.definition().allow_manual);
  assert!(
    policies.launch_policy.definition().allow_automatic_source,
    "new default must not stop the saved policy"
  );
  f.stop().await;
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn configuration_publication_requires_existing_exact_owned_project_policies() {
  let f = Fixture::with_installer(Some(Arc::new(installed_fixture::Installation))).await;
  let flow = flow(&f).await;
  let base = format!("http://{}/api/v1", f.runtime.management_addr());
  let reference =
    |id: uuid::Uuid| json!({"project_id":f.project,"id":id.to_string(),"version":1,"digest":"ab".repeat(32)});
  let candidate = json!({"id":uuid::Uuid::new_v4().to_string(),"project_id":f.project,"definition":{
    "admission_policy":{"project_policy":reference(uuid::Uuid::new_v4())},
    "permission_ceiling":{"project_policy":reference(uuid::Uuid::new_v4())},
    "hard_budget":installed_fixture::budget(),"wip_limits":{"max_active_runs":8,"max_active_nodes":8},
    "enabled":true,"flow":flow
  }});
  let response = f
    .client
    .post(format!("{base}/factory-configurations"))
    .header("idempotency-key", "unknown-project-policies")
    .json(&candidate)
    .send()
    .await
    .unwrap();
  let status = response.status();
  let error = response.json::<Value>().await.unwrap();
  assert_eq!(
    status,
    StatusCode::BAD_REQUEST,
    "a fabricated pin cannot be published: {error}"
  );
  assert_eq!(error["diagnostics"]["errors"][0]["field"], "/admission_policy");
  assert_eq!(error["diagnostics"]["errors"][0]["code"], "unavailable_reference");
  let (launch, access) = policies(&f).await;
  let mut known = candidate.clone();
  known["definition"]["admission_policy"]["project_policy"] = launch;
  known["definition"]["permission_ceiling"]["project_policy"] = access;
  for (field, property, invalid) in [
    (
      "admission_policy",
      "project_id",
      json!(uuid::Uuid::new_v4().to_string()),
    ),
    ("permission_ceiling", "digest", json!("ff".repeat(32))),
    ("permission_ceiling", "version", json!(999)),
  ] {
    let mut unavailable = known.clone();
    unavailable["definition"][field]["project_policy"][property] = invalid;
    let response = f
      .client
      .post(format!("{base}/factory-configurations"))
      .header("idempotency-key", "unknown-project-policies")
      .json(&unavailable)
      .send()
      .await
      .unwrap();
    let status = response.status();
    let error = response.json::<Value>().await.unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["diagnostics"]["errors"][0]["field"], format!("/{field}"));
    assert_eq!(error["diagnostics"]["errors"][0]["code"], "unavailable_reference");
  }
  let response = f
    .client
    .post(format!("{base}/factory-configurations"))
    .header("idempotency-key", "unknown-project-policies")
    .json(&known)
    .send()
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::CREATED,
    "failed references must not reserve the publication key"
  );
  f.stop().await;
}
