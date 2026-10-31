//! Explicit Project code availability and independent step selections.
use super::*;

#[test]
fn common_flow_can_declare_an_empty_code_inventory_and_explicit_no_code_steps() {
  run_ready(async {
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::json!({});
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    let draft = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire);
    assert!(
      draft.is_ok(),
      "a reviewed no-code Flow must decode explicitly: {draft:?}"
    );
    let flow = authoring_queries()
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: ProjectId::generate(),
          draft: draft.unwrap(),
        },
      )
      .await
      .unwrap();
    let definition = flow.closure.definition(flow.closure.root()).unwrap();
    let retained = serde_json::to_value(definition).unwrap();
    assert_eq!(retained["code_sources"], serde_json::json!({}));
    for node in retained["nodes"].as_array().unwrap() {
      assert_eq!(node["code"], serde_json::json!({"kind":"none"}));
      assert!(node["code"].get("repository_id").is_none());
    }
  });
}

async fn code_inventory(
  project: ProjectId,
) -> (
  Arc<octacity_server_store::testing::InMemoryConfigurationStore>,
  serde_json::Map<String, serde_json::Value>,
) {
  code_inventory_with_exact_policy(project, true).await
}

async fn code_inventory_with_exact_policy(
  project: ProjectId,
  allow_exact_revision: bool,
) -> (
  Arc<octacity_server_store::testing::InMemoryConfigurationStore>,
  serde_json::Map<String, serde_json::Value>,
) {
  use octacity_server_store::{
    ConfigurationStore as _, CreateRepository, RepositoryDefinition, RepositorySelectionPolicy,
  };
  let repositories = Arc::new(octacity_server_store::testing::InMemoryConfigurationStore::new());
  repositories.seed_project(project).unwrap();
  let mut inventory = serde_json::Map::new();
  for alias in ["first", "second"] {
    let published = repositories
      .create_repository(octacity_server_store::testing::management_mutation_with_request(
        CreateRepository {
          id: octacity_server_domain::RepositoryId::generate(),
          project_id: project,
          name: alias.parse().unwrap(),
          definition: RepositoryDefinition {
            vcs_integration_id: octacity_server_domain::IntegrationId::generate(),
            repository_locator: format!("https://example.test/{alias}.git").parse().unwrap(),
            selection: RepositorySelectionPolicy {
              allowed_references: if allow_exact_revision {
                Default::default()
              } else {
                ["refs/heads/main".parse().unwrap()].into()
              },
              default_reference: (!allow_exact_revision).then(|| "refs/heads/main".parse().unwrap()),
              allow_exact_revision,
            },
          },
          idempotency_key: alias.parse().unwrap(),
          published_at: time(1),
        },
        format!("code-source-{alias}"),
      ))
      .await
      .unwrap()
      .repository;
    inventory.insert(
      alias.to_owned(),
      serde_json::json!({
        "repository": FactoryRepositoryRef {
          project_id: project, id: published.id, version: published.version,
          definition_digest: FactoryDigest::content_sha256(&serde_json::to_vec(&published.definition).unwrap()),
        },
        "selection": if allow_exact_revision { serde_json::json!({"kind":"exact_revision", "value":format!("base-{alias}")}) } else { serde_json::json!({"kind":"default_reference"}) },
        "access":"read_write", "scope":{"kind":"repository"},
      }),
    );
  }
  (repositories, inventory)
}

fn code_queries(
  repositories: Arc<octacity_server_store::testing::InMemoryConfigurationStore>,
) -> AuthorizedQueryHandler<crate::FactoryFlowDefinitionHandlers<super::profile_selection::PublicationChoices>> {
  let store = Arc::new(super::profile_selection::PublicationChoices(std::sync::Mutex::new(
    FactoryConfigurationChoices::try_new(Default::default()).unwrap(),
  )));
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(crate::FactoryFlowDefinitionHandlers::new(store, policy).with_repositories(repositories)),
  )
}

#[test]
fn declared_project_repositories_are_exact_available_choices_without_granting_step_access() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory(project).await;
    let queries = code_queries(repositories);
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory.clone());
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    let draft = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire.clone());
    assert!(
      draft.is_ok(),
      "Project source declarations must use a closed typed contract: {draft:?}"
    );
    let flow = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: draft.unwrap(),
        },
      )
      .await
      .unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    assert_eq!(
      serde_json::to_value(root).unwrap()["code_sources"],
      serde_json::Value::Object(inventory)
    );
    for node in root.nodes() {
      assert_eq!(node.permissions(), &FactoryPermissionSet::deny_all());
      assert!(node.context_binding().is_none());
      assert_eq!(
        serde_json::to_value(node).unwrap()["code"],
        serde_json::json!({"kind":"none"})
      );
    }
    wire["root"]["code_sources"]["first"]["repository"]["definition_digest"] = serde_json::json!(digest(255));
    let spoofed = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire).unwrap();
    assert!(
      queries
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            id: FlowDefinitionId::generate(),
            project_id: project,
            draft: spoofed,
          }
        )
        .await
        .is_err(),
      "a caller cannot substitute an immutable Repository definition"
    );
  });
}

#[test]
fn independent_step_code_bindings_use_declared_sources_and_reject_dangling_or_wider_access() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory(project).await;
    let queries = code_queries(repositories);
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory);
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    wire["root"]["nodes"][0]["code"] = serde_json::json!({
      "kind":"source", "source":"first", "access":"read_only", "scope":{"kind":"repository"}, "revision":{"kind":"admitted_base"},
    });
    wire["root"]["nodes"][1]["code"] = serde_json::json!({
      "kind":"source", "source":"second", "access":"read_only", "scope":{"kind":"repository"}, "revision":{"kind":"admitted_base"},
    });
    let draft = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire.clone());
    assert!(draft.is_ok(), "independent step source choices must decode: {draft:?}");
    let flow = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: draft.unwrap(),
        },
      )
      .await
      .unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    assert_eq!(
      serde_json::to_value(root.node(&key("measure_layout")).unwrap()).unwrap()["code"]["source"],
      "first"
    );
    assert_eq!(
      serde_json::to_value(root.node(&key("proof_caption")).unwrap()).unwrap()["code"]["source"],
      "second"
    );
    assert_eq!(
      serde_json::to_value(root.node(&key("package_caption")).unwrap()).unwrap()["code"],
      serde_json::json!({"kind":"none"})
    );
    let mut detached = wire.clone();
    detached["root"]["nodes"][0]["code"] = serde_json::json!({"kind":"none"});
    let replacement = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: serde_json::from_value(detached).unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      replacement
        .closure
        .definition(replacement.closure.root())
        .unwrap()
        .node(&key("proof_caption"))
        .unwrap()
        .code(),
      root.node(&key("proof_caption")).unwrap().code()
    );
    for problem in ["undeclared", "removed", "wider_access"] {
      let mut invalid = wire.clone();
      let field = if problem == "wider_access" {
        invalid["root"]["code_sources"]["first"]["access"] = serde_json::json!("read_only");
        invalid["root"]["nodes"][0]["code"]["access"] = serde_json::json!("read_write");
        "/code/access"
      } else {
        if problem == "removed" {
          invalid["root"]["code_sources"].as_object_mut().unwrap().remove("first");
        } else {
          invalid["root"]["nodes"][0]["code"]["source"] = serde_json::json!("undeclared");
        }
        "/code/source"
      };
      let error = queries
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            id: FlowDefinitionId::generate(),
            project_id: project,
            draft: serde_json::from_value(invalid).unwrap(),
          },
        )
        .await
        .unwrap_err();
      let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
        panic!("{error:?}");
      };
      assert_eq!(report.errors[0].field, field, "{problem}");
      assert_eq!(report.errors[0].node, Some(key("measure_layout")), "{problem}");
    }
  });
}

#[test]
fn code_paths_and_line_ranges_can_narrow_but_never_widen_a_declared_source() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory(project).await;
    let queries = code_queries(repositories);
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory);
    wire["root"]["code_sources"]["first"]["access"] = serde_json::json!("read_only");
    wire["root"]["code_sources"]["first"]["scope"] = serde_json::json!({"kind":"selected", "entries":[
      {"kind":"path", "path":"src"},
      {"kind":"range", "path":"docs/design.md", "lines":{"first_line":10,"last_line":30}},
    ]});
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    let narrow = serde_json::json!({"kind":"selected", "entries":[
      {"kind":"path", "path":"src/engine"},
      {"kind":"range", "path":"docs/design.md", "lines":{"first_line":20,"last_line":22}},
    ]});
    wire["root"]["nodes"][0]["code"] = serde_json::json!({
      "kind":"source", "source":"first", "access":"read_only", "scope":narrow.clone(), "revision":{"kind":"admitted_base"},
    });
    let draft = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire.clone());
    assert!(draft.is_ok(), "bounded source paths/ranges must decode: {draft:?}");
    let flow = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: draft.unwrap(),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(
        flow
          .closure
          .definition(flow.closure.root())
          .unwrap()
          .node(&key("measure_layout"))
          .unwrap()
      )
      .unwrap()["code"]["scope"],
      narrow
    );
    for (label, scope) in [
      (
        "neighbor prefix",
        serde_json::json!({"kind":"selected","entries":[{"kind":"path","path":"src-other"}]}),
      ),
      (
        "wider range",
        serde_json::json!({"kind":"selected","entries":[{"kind":"range","path":"docs/design.md","lines":{"first_line":20,"last_line":31}}]}),
      ),
      (
        "whole ranged file",
        serde_json::json!({"kind":"selected","entries":[{"kind":"path","path":"docs/design.md"}]}),
      ),
      ("whole repository", serde_json::json!({"kind":"repository"})),
    ] {
      let mut invalid = wire.clone();
      invalid["root"]["nodes"][0]["code"]["scope"] = scope;
      let error = queries
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            id: FlowDefinitionId::generate(),
            project_id: project,
            draft: serde_json::from_value(invalid).unwrap(),
          },
        )
        .await
        .unwrap_err();
      let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
        panic!("{error:?}");
      };
      assert_eq!(report.errors[0].field, "/code/scope", "{label}");
      assert_eq!(report.errors[0].node, Some(key("measure_layout")), "{label}");
    }
    let mut writable = wire.clone();
    writable["root"]["code_sources"]["first"]["access"] = serde_json::json!("read_write");
    writable["root"]["code_sources"]["first"]["scope"] = serde_json::json!({"kind":"repository"});
    writable["root"]["nodes"][0]["code"]["access"] = serde_json::json!("read_write");
    let error = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: serde_json::from_value(writable).unwrap(),
        },
      )
      .await
      .unwrap_err();
    let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
      panic!("{error:?}");
    };
    assert_eq!(
      report.errors[0].field, "/code/access",
      "line views cannot grant writes to hidden file content"
    );
    for bad_scope in [
      serde_json::json!({"kind":"selected","entries":[]}),
      serde_json::json!({"kind":"selected","entries":[{"kind":"path","path":"../private"}]}),
      serde_json::json!({"kind":"selected","entries":[{"kind":"range","path":"src/a.rs","lines":{"first_line":20,"last_line":10}}]}),
    ] {
      let mut invalid = wire.clone();
      invalid["root"]["nodes"][0]["code"]["scope"] = bad_scope;
      assert!(serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(invalid).is_err());
    }
  });
}

#[test]
fn configuration_revalidates_available_code_instead_of_trusting_a_prevalidated_flow() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory(project).await;
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory);
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    let flow = code_queries(repositories.clone())
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: serde_json::from_value(wire).unwrap(),
        },
      )
      .await
      .unwrap();
    let mut fixture = fixture(project);
    fixture.draft.flow = Some(flow);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let outcome = query(
      &FactoryConfigurationHandlers::new(store.clone()),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft.clone(),
      },
    )
    .await;
    assert!(
      outcome.is_err(),
      "the configuration must revalidate Project code references through their owner"
    );
    let crate::ApplicationError::FactoryValidation(report) = outcome.unwrap_err() else {
      panic!("expected a source availability diagnostic")
    };
    assert_eq!(report.errors[0].field, "/code_sources/first/repository");
    query(
      &FactoryConfigurationHandlers::new(store.clone()).with_repositories(repositories),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap();
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
  });
}

#[test]
fn step_exact_revision_is_explicit_and_cannot_replace_an_exact_declared_source() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory(project).await;
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory);
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    wire["root"]["nodes"][0]["code"] = serde_json::json!({
      "kind":"source","source":"first","access":"read_only","scope":{"kind":"repository"},
      "revision":{"kind":"exact_revision","revision":"base-first"}
    });
    let decoded = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire.clone());
    assert!(
      decoded.is_ok(),
      "an explicit immutable step revision must decode: {decoded:?}"
    );
    let queries = code_queries(repositories);
    queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: decoded.unwrap(),
        },
      )
      .await
      .unwrap();
    wire["root"]["nodes"][0]["code"]["revision"]["revision"] = serde_json::json!("unreviewed-replacement");
    let error = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: serde_json::from_value(wire).unwrap(),
        },
      )
      .await
      .unwrap_err();
    let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
      panic!("expected revision narrowing diagnostic")
    };
    assert_eq!(report.errors[0].field, "/code/revision");
    assert_eq!(report.errors[0].node.as_ref().unwrap().as_str(), "measure_layout");
  });
}

#[test]
fn step_exact_revision_cannot_bypass_repository_selection_policy() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory_with_exact_policy(project, false).await;
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory);
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    wire["root"]["nodes"][0]["code"] = serde_json::json!({
      "kind":"source","source":"first","access":"read_only","scope":{"kind":"repository"},
      "revision":{"kind":"exact_revision","revision":"unreviewed-ref"}
    });
    let outcome = code_queries(repositories)
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: serde_json::from_value(wire).unwrap(),
        },
      )
      .await;
    assert!(
      outcome.is_err(),
      "a step cannot bypass the repository's exact-revision prohibition"
    );
    let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = outcome.unwrap_err()
    else {
      panic!("expected revision policy diagnostic")
    };
    assert_eq!(report.errors[0].field, "/code/revision");
    assert_eq!(report.errors[0].node.as_ref().unwrap().as_str(), "measure_layout");
  });
}

#[test]
fn candidate_code_selection_names_an_accepted_result_of_the_same_declared_repository() {
  run_ready(async {
    let project = ProjectId::generate();
    let (repositories, inventory) = code_inventory(project).await;
    let mut wire = serde_json::to_value(layout_draft()).unwrap();
    wire["root"]["code_sources"] = serde_json::Value::Object(inventory);
    for node in wire["root"]["nodes"].as_array_mut().unwrap() {
      node["code"] = serde_json::json!({"kind":"none"});
    }
    wire["root"]["nodes"][0]["code"] = serde_json::json!({
      "kind":"source","source":"first","access":"read_only","scope":{"kind":"repository"},"revision":{"kind":"admitted_base"}
    });
    wire["root"]["nodes"][1]["code"] = serde_json::json!({
      "kind":"source","source":"first","access":"read_only","scope":{"kind":"repository"},
      "revision":{"kind":"accepted_candidate","node":"measure_layout","outcome":"measured"}
    });
    let decoded = serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire.clone());
    assert!(
      decoded.is_ok(),
      "candidate selection must be explicit and tied to an accepted result: {decoded:?}"
    );
    let queries = code_queries(repositories);
    let validated = queries
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDefinitionQuery {
          id: FlowDefinitionId::generate(),
          project_id: project,
          draft: decoded.unwrap(),
        },
      )
      .await
      .unwrap();
    let root = validated.closure.definition(validated.closure.root()).unwrap();
    assert_eq!(
      serde_json::to_value(root.node(&key("proof_caption")).unwrap()).unwrap()["code"]["revision"],
      serde_json::json!({"kind":"accepted_candidate","node":"measure_layout","outcome":"measured"})
    );
    for (field, replacement) in [
      ("/root/nodes/1/code/source", serde_json::json!("second")),
      ("/root/nodes/1/code/revision/node", serde_json::json!("missing")),
      ("/root/nodes/1/code/revision/node", serde_json::json!("proof_caption")),
      ("/root/nodes/1/code/revision/outcome", serde_json::json!("not_accepted")),
      ("/root/nodes/0/code", serde_json::json!({"kind":"none"})),
    ] {
      let mut invalid = wire.clone();
      *invalid.pointer_mut(field).unwrap() = replacement;
      let error = queries
        .handle_query(
          &context(),
          crate::ValidateFactoryFlowDefinitionQuery {
            id: FlowDefinitionId::generate(),
            project_id: project,
            draft: serde_json::from_value(invalid).unwrap(),
          },
        )
        .await
        .unwrap_err();
      let ManagementHandlerError::Application(crate::ApplicationError::FactoryValidation(report)) = error else {
        panic!("expected candidate resource diagnostic")
      };
      assert_eq!(report.errors[0].field, "/code/revision");
      assert_eq!(report.errors[0].node.as_ref().unwrap().as_str(), "proof_caption");
    }
  });
}
