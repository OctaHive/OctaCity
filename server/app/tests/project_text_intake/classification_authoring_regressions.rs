//! Public authoring must depend only on the fields and effects selected by the action.
use super::*;

fn size_only_draft(
  issue: &IssueRevision,
  offered: &FactoryIssueClassificationChoices,
  version: u64,
) -> FactoryFlowDefinitionDraft {
  let mut draft = typed_draft(issue, offered);
  let schema = FlowDataSchema::new(
    "selected.size".parse().unwrap(),
    "1".parse().unwrap(),
    FlowValueSchema::Object {
      fields: [(
        "size".parse().unwrap(),
        FlowFieldSchema::optional(FlowValueSchema::Enum {
          values: vec![
            serde_json::json!("small"),
            serde_json::json!("medium"),
            serde_json::json!("large"),
          ],
        }),
      )]
      .into(),
    },
  )
  .unwrap();
  let mut root = serde_json::to_value(&draft.root).unwrap();
  for node in root["nodes"].as_array_mut().unwrap() {
    if node["key"] == "triage" {
      node["outcomes"][0]["schema"] = serde_json::to_value(schema.reference()).unwrap();
      node["gate"] = serde_json::to_value(
        FlowGateProgram::new(vec![], "classified".parse().unwrap())
          .unwrap()
          .with_default_output_mapping(
            FlowDataMapping::new(FlowValueMapping::Constant {
              value: serde_json::json!({"size":"small"}),
            })
            .unwrap(),
          )
          .unwrap()
          .binding()
          .unwrap(),
      )
      .unwrap();
    }
    if node["key"] == "apply" {
      node["input_schema"] = serde_json::to_value(schema.reference()).unwrap();
      node["action"]["parameters"]["fields"] = serde_json::json!({"definition_version":version,"size":true});
    }
  }
  draft.root = serde_json::from_value(root).unwrap();
  draft.data_schemas.push(schema);
  draft
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn valid_large_catalog_can_publish_a_small_field_selection() {
  let (db, store, issue, _, offered) = fixture().await;
  let mut definition = store.issue_properties(issue.project_id, None).await.unwrap().definition;
  let fill = |choices: &mut Vec<IssuePropertyChoice>| {
    for n in choices.len()..64 {
      choices.push(IssuePropertyChoice {
        id: format!("choice_{n:057}"),
        name: "Option".into(),
        retired: false,
      });
    }
  };
  fill(&mut definition.priorities);
  fill(&mut definition.types);
  fill(&mut definition.components);
  let IssuePropertyKind::Choice { choices } = &mut definition.properties[0].kind else {
    panic!("reproduction choices")
  };
  fill(choices);
  for slot in ["risk", "severity", "recommended_route"] {
    let mut choices = vec![];
    fill(&mut choices);
    definition.properties.push(IssuePropertyDefinition {
      id: slot.into(),
      name: slot.into(),
      retired: false,
      kind: IssuePropertyKind::Choice { choices },
    });
  }
  definition.classification.risk = Some("risk".into());
  definition.classification.severity = Some("severity".into());
  definition.classification.recommended_route = Some("recommended_route".into());
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 1,
        definition,
      },
      idempotency_key: key("large-catalog"),
      published_at: at(4),
    }))
    .await
    .expect("the catalog is within the Project's supported bounds");
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let roles = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(
      FactoryRoleProfileHandlers::with_policy(store.clone(), policy.clone()).with_issue_properties(store.clone()),
    ),
  );
  let current = roles
    .handle_query(
      &context(),
      GetFactoryAuthoringCapabilitiesQuery {
        project_id: issue.project_id,
      },
    )
    .await
    .unwrap()
    .issue_classification
    .unwrap();
  assert!(
    current.field_contract.is_none(),
    "the all-fields preview exceeds one Flow schema budget"
  );
  let commands = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy).with_issue_properties(store.clone())),
  );
  let published = commands
    .handle_command(
      &context(),
      CreateFactoryFlowDefinitionCommand {
        id: FlowDefinitionId::generate(),
        project_id: issue.project_id,
        draft: size_only_draft(&issue, &offered, 2),
        idempotency_key: key("selected-size"),
        published_at: at(5),
      },
    )
    .await
    .expect("unselected Project vocabularies cannot block a bounded size-only publication");
  assert_eq!(published.disposition, MutationDisposition::Applied);
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn retired_legacy_label_does_not_disable_typed_classification() {
  let (db, original, issue, _, offered) = fixture().await;
  let FactoryConfigurationAvailability::Available(choices) = original
    .factory_configuration_availability(issue.project_id)
    .await
    .unwrap()
  else {
    panic!("installed choices")
  };
  let choices = choices
    .with_choice_metadata(
      FactoryChoiceKind::NodeCapability,
      "local-issue-classification".parse().unwrap(),
      serde_json::from_value(serde_json::json!({
        "node_kinds": ["trusted_action"],
        "signal_purposes": [],
        "enforcement": [],
        "compatible_choices": [],
        "issue_classification": {
          "policy": "triage",
          "size_labels": {"small": "retired_size"}
        }
      }))
      .unwrap(),
    )
    .unwrap();
  let store = Arc::new(PostgresStore::with_factory_capabilities(
    db.pool.clone(),
    Arc::new(Installed(choices)),
  ));
  let mut definition = store.issue_properties(issue.project_id, None).await.unwrap().definition;
  definition.labels.push(IssuePropertyChoice {
    id: "retired_size".into(),
    name: "Retired size".into(),
    retired: true,
  });
  store
    .publish_issue_properties(management_mutation(PublishIssueProperties {
      intent: PublishIssuePropertiesIntent {
        project_id: issue.project_id,
        expected_version: 1,
        definition,
      },
      idempotency_key: key("retired-size"),
      published_at: at(4),
    }))
    .await
    .unwrap();
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let roles = AuthorizedQueryHandler::new(
    policy.clone(),
    Arc::new(
      FactoryRoleProfileHandlers::with_policy(store.clone(), policy.clone()).with_issue_properties(store.clone()),
    ),
  );
  let current = roles
    .handle_query(
      &context(),
      GetFactoryAuthoringCapabilitiesQuery {
        project_id: issue.project_id,
      },
    )
    .await
    .unwrap()
    .issue_classification
    .expect("typed fields do not depend on retired legacy labels");
  assert!(
    current.size_labels.is_empty(),
    "unavailable legacy mappings cannot be offered as active labels"
  );
  assert!(current.field_contract.is_some());
  let commands = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy).with_issue_properties(store.clone())),
  );
  commands
    .handle_command(
      &context(),
      CreateFactoryFlowDefinitionCommand {
        id: FlowDefinitionId::generate(),
        project_id: issue.project_id,
        draft: size_only_draft(&issue, &offered, 2),
        idempotency_key: key("size-without-labels"),
        published_at: at(5),
      },
    )
    .await
    .unwrap();
  // Explicit legacy effects still require their original active mapping.
  let mut legacy = size_only_draft(&issue, &offered, 2);
  let mut root = serde_json::to_value(&legacy.root).unwrap();
  let action = root["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "apply")
    .unwrap();
  action["action"]["parameters"]["size_labels"] = serde_json::json!({"small":"retired_size"});
  legacy.root = serde_json::from_value(root).unwrap();
  assert!(
    commands
      .handle_command(
        &context(),
        CreateFactoryFlowDefinitionCommand {
          id: FlowDefinitionId::generate(),
          project_id: issue.project_id,
          draft: legacy,
          idempotency_key: key("retired-label-effect"),
          published_at: at(5)
        }
      )
      .await
      .is_err()
  );
  assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn published_classification_preserves_declared_material_dependencies() {
  let (db, store, issue, submission, offered) = fixture().await;
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(TrustedNetworkManagementPolicy);
  let clock = Arc::new(Clock(AtomicI64::new(4)));
  let comment = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueDiscussionHandlers::new(
      store.clone(),
      policy.clone(),
      clock.clone(),
    )),
  )
  .handle_command(
    &context(),
    AppendIssueCommentCommand {
      intent: AppendIssueCommentIntent {
        project_id: issue.project_id,
        issue_id: issue.id,
        parent_id: None,
        body: IssueCommentBody::try_new("Assessment depends on these requirements").unwrap(),
      },
      idempotency_key: key("classification-comment"),
    },
  )
  .await
  .unwrap()
  .comment;
  let reference = IssueMaterialReference::Comment {
    project_id: issue.project_id,
    issue_id: issue.id,
    comment_id: comment.id,
    body_digest: comment.body_digest,
  };
  let materials = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueMaterialHandlers::new(
      store.clone(),
      store.clone(),
      policy.clone(),
      clock.clone(),
    )),
  );
  let issue = materials
    .handle_command(
      &context(),
      EditIssueMaterialsCommand {
        intent: EditIssueMaterialsIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          references: vec![reference.clone()],
        },
        idempotency_key: key("classification-material"),
      },
    )
    .await
    .unwrap()
    .issue;
  let mut draft = size_only_draft(&issue, &offered, 1);
  let size = draft.data_schemas.last().unwrap();
  let FlowValueSchema::Object { mut fields } = size.shape().clone() else {
    panic!("classification object")
  };
  let size = fields.get_mut(&"size".parse().unwrap()).unwrap();
  *size = FlowFieldSchema::required(size.value().clone());
  let string = |length| {
    FlowFieldSchema::required(FlowValueSchema::String {
      min_bytes: length,
      max_bytes: length,
    })
  };
  fields.insert(
    "assessment_inputs".parse().unwrap(),
    FlowFieldSchema::required(FlowValueSchema::Object {
      fields: [(
        "materials".parse().unwrap(),
        FlowFieldSchema::required(FlowValueSchema::Array {
          min_items: 1,
          max_items: 32,
          item: Box::new(FlowValueSchema::Object {
            fields: [
              (
                "kind".parse().unwrap(),
                FlowFieldSchema::required(FlowValueSchema::Enum {
                  values: vec![serde_json::json!("comment")],
                }),
              ),
              ("project_id".parse().unwrap(), string(36)),
              ("issue_id".parse().unwrap(), string(36)),
              ("comment_id".parse().unwrap(), string(36)),
              ("body_digest".parse().unwrap(), string(64)),
            ]
            .into(),
          }),
        }),
      )]
      .into(),
    }),
  );
  let schema = FlowDataSchema::new(
    "classification.with-dependencies".parse().unwrap(),
    "1".parse().unwrap(),
    FlowValueSchema::Object { fields },
  )
  .unwrap();
  let accepted = serde_json::json!({"size":"small","assessment_inputs":{"materials":[reference]}});
  let mut root = serde_json::to_value(&draft.root).unwrap();
  for node in root["nodes"].as_array_mut().unwrap() {
    if node["key"] == "triage" {
      node["outcomes"][0]["schema"] = serde_json::to_value(schema.reference()).unwrap();
      node["gate"] = serde_json::to_value(
        FlowGateProgram::new(vec![], "classified".parse().unwrap())
          .unwrap()
          .with_default_output_mapping(
            FlowDataMapping::new(FlowValueMapping::Constant {
              value: accepted.clone(),
            })
            .unwrap(),
          )
          .unwrap()
          .binding()
          .unwrap(),
      )
      .unwrap();
    }
    if node["key"] == "apply" {
      node["input_schema"] = serde_json::to_value(schema.reference()).unwrap();
    }
  }
  draft.root = serde_json::from_value(root).unwrap();
  draft.data_schemas.push(schema);
  let commands = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(FactoryFlowDefinitionHandlers::new(store.clone(), policy.clone()).with_issue_properties(store.clone())),
  );
  let created = commands
    .handle_command(
      &context(),
      CreateFactoryFlowDefinitionCommand {
        id: FlowDefinitionId::generate(),
        project_id: issue.project_id,
        draft: draft.clone(),
        idempotency_key: key("publish-with-dependencies"),
        published_at: at(5),
      },
    )
    .await
    .expect("the exact accepted dependency envelope must be authorable");
  let (before, after) = execute_triage(
    store.clone(),
    db.pool.clone(),
    &issue,
    submission,
    created.publication.flow,
  )
  .await;
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  assert_eq!(current.planning.size, Some(IssueSize::Small));
  assert_eq!(current.classification.fields.len(), 1);
  assert_eq!(
    current.classification.fields[0].inputs.materials,
    vec![reference.clone()]
  );
  assert_eq!(before.work, after.work);
  assert_eq!(before.outbox, after.outbox);
  let mut omitted = draft.clone();
  let size_schema = omitted
    .data_schemas
    .iter()
    .find(|schema| schema.reference().identity().as_str() == "selected.size")
    .unwrap()
    .reference()
    .clone();
  let mut omitted_root = serde_json::to_value(&omitted.root).unwrap();
  let action = omitted_root["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "apply")
    .unwrap();
  action["input_schema"] = serde_json::to_value(size_schema).unwrap();
  action["input_binding"]["mapping"]["value"] =
    serde_json::json!({"mapping":"object","fields":{"size":{"mapping":"pointer","path":"/assessment/size"}}});
  omitted.root = serde_json::from_value(omitted_root).unwrap();
  assert!(
    commands
      .handle_command(
        &context(),
        CreateFactoryFlowDefinitionCommand {
          id: FlowDefinitionId::generate(),
          project_id: issue.project_id,
          draft: omitted,
          idempotency_key: key("omitted-dependencies"),
          published_at: at(6)
        }
      )
      .await
      .is_err(),
    "an accepted assessment's declared dependencies cannot be projected away"
  );
  let mut substituted = serde_json::to_value(&draft.root).unwrap();
  let apply = substituted["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|node| node["key"] == "apply")
    .unwrap();
  let mut forged = accepted["assessment_inputs"].clone();
  forged["materials"][0]["body_digest"] = serde_json::json!("0".repeat(64));
  apply["input_binding"]["mapping"]["value"] = serde_json::json!({"mapping":"object","fields":{"size":{"mapping":"pointer","path":"/assessment/size"},"assessment_inputs":{"mapping":"constant","value":forged}}});
  draft.root = serde_json::from_value(substituted).unwrap();
  assert!(
    commands
      .handle_command(
        &context(),
        CreateFactoryFlowDefinitionCommand {
          id: FlowDefinitionId::generate(),
          project_id: issue.project_id,
          draft,
          idempotency_key: key("substituted-dependencies"),
          published_at: at(6)
        }
      )
      .await
      .is_err(),
    "declaring a dependency slot cannot authorize substituted provenance"
  );
  clock.0.store(30, Ordering::SeqCst);
  materials
    .handle_command(
      &context(),
      EditIssueMaterialsCommand {
        intent: EditIssueMaterialsIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: current.version,
          references: vec![],
        },
        idempotency_key: key("remove-assessed-material"),
      },
    )
    .await
    .unwrap();
  let review = AuthorizedQueryHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store, policy)))
    .handle_query(
      &context(),
      ReviewIssueClassificationQuery {
        project_id: issue.project_id,
        id: issue.id,
        version: None,
        subject: None,
        checked_at: at(40),
      },
    )
    .await
    .unwrap();
  assert!(
    review.fields.iter().any(|field| field
      .problems
      .iter()
      .any(|problem| matches!(problem, IssueClassificationProblem::MaterialChanged { .. }))),
    "removing a declared dependency must invalidate the applied assessment"
  );
  db.cleanup().await;
}
