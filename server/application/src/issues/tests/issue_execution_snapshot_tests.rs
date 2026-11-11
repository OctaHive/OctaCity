//! A manual Issue supplies snapshot data without a producing execution.
use super::*;
use octacity_server_factory::*;

#[path = "issue_execution_snapshot_fixture.rs"]
mod fixture;
use fixture::*;

#[path = "issue_execution_snapshot_semantics.rs"]
mod semantics;

#[path = "issue_execution_snapshot_presence.rs"]
mod presence;

#[path = "issue_execution_snapshot_size.rs"]
mod size;

#[path = "issue_execution_snapshot_compatibility.rs"]
mod compatibility;

#[test]
fn manual_issue_snapshot_request_freezes_its_selection_without_a_predecessor_or_run() {
  run_ready(async {
    let (store, factory, issue, command) = snapshot_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let before = factory.mutation_evidence_counts().await;
    let request = commands
      .handle_command(&context(), command.clone())
      .await
      .expect("manually classified Issue needs no predecessor")
      .request;
    let encoded = serde_json::to_value(&request).unwrap();
    assert!(
      encoded["snapshot_digest"].is_string(),
      "request retains its server-resolved selection identity"
    );
    assert!(request.intent.target.predecessor_run.is_none());
    assert_eq!(request.intent.expected_version, issue.version);
    assert!(request.association.is_none());
    assert_eq!(factory.mutation_evidence_counts().await, before);
    assert_eq!(
      commands.handle_command(&context(), command).await.unwrap().request,
      request
    );
  });
}

#[test]
fn manual_issue_snapshot_publishes_frozen_inputs_and_code_without_a_triage_run() {
  run_ready(async {
    let (store, factory, issue, command) = snapshot_fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        policy.clone(),
      )),
    );
    let request = requests.handle_command(&context(), command).await.unwrap().request;
    let published = publish_snapshot_request(store.clone(), factory.clone(), &request).await;
    let association = published.association.unwrap();
    let run = factory.factory_run_snapshot(association.run_id).await.unwrap();
    assert_eq!(run.work.artifacts().task().content_digest(), request.task_digest);
    assert_eq!(
      run.work.artifacts().acceptance().content_digest(),
      request.acceptance_digest
    );
    assert_eq!(
      run.work.subject().repository().unwrap().base_revision().as_str(),
      "0123456789abcdef"
    );
    assert_eq!(run.flow.data.incoming.len(), 1);
    let incoming = &run.flow.data.incoming[0];
    assert!(incoming.predecessor().is_none());
    assert_eq!(
      incoming.payload().value(),
      &serde_json::json!({"title":"Manual feature","size":"small","priority":"high"})
    );
    let encoded = serde_json::to_value(incoming).unwrap();
    assert_eq!(
      encoded["issue_snapshot"]["digest"],
      serde_json::to_value(request.snapshot_digest).unwrap()
    );
    assert_eq!(encoded["issue_snapshot"]["issue_id"], issue.id.to_string());
    let preparation = factory
      .project_text_preparation(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let documents = factory
      .project_text_documents(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    let documents = serde_json::to_value(documents).unwrap();
    let snapshot = &documents["issue_snapshot"];
    assert_eq!(snapshot["version"], serde_json::to_value(issue.version).unwrap());
    assert_eq!(
      snapshot["applicability_version"],
      serde_json::to_value(issue.applicability_version).unwrap()
    );
    let catalog = &snapshot["definitions"]["0"]["selected_v1"];
    assert_eq!(catalog["version"], 0);
    assert_eq!(catalog["priorities"][0]["id"], "blocker");
    assert_eq!(snapshot["attribution"]["size"]["origin"]["kind"], "manual");
    assert_eq!(snapshot["attribution"]["priority"]["origin"]["kind"], "manual");
    assert_eq!(snapshot["values"], incoming.payload().value().clone());
    assert!(snapshot["omissions"].as_object().unwrap().is_empty());
    assert_eq!(
      serde_json::to_value(&preparation.intent).unwrap()["snapshot_digest"],
      encoded["issue_snapshot"]["digest"]
    );
    assert_eq!(
      preparation.frozen_revision.as_ref(),
      Some(run.work.subject().repository().unwrap().base_revision())
    );
    assert_eq!(preparation.admission.as_ref().unwrap().run.id(), association.run_id);
  });
}

#[test]
fn manual_issue_snapshot_checks_code_dependent_assessments_against_the_resolved_base() {
  run_ready(async {
    let (store, factory, mut issue, mut command) = snapshot_fixture().await;
    issue.version = issue.version.next().unwrap();
    issue.updated_at = at(35);
    issue
      .classification
      .fields
      .iter_mut()
      .find(|f| f.field == IssueClassificationFieldReference::Size)
      .unwrap()
      .inputs
      .subject = Some(FactoryTaskSubject::Exact(ExactSubject::new(
      issue.project_id,
      command.intent.target.repository_id,
      octacity_server_domain::ImmutableRevision::new("another-base").unwrap(),
    )));
    store.seed_retained_issue_revision(issue.clone()).unwrap();
    command.intent.expected_version = issue.version;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        policy.clone(),
      )),
    );
    let request = requests.handle_command(&context(), command).await.unwrap().request;
    let bytes = Arc::new(crate::management::tests::project_text_intake_tests::DocumentBytes::default());
    let publisher = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionPublisher::new(
        store,
        factory.clone(),
        bytes,
        Arc::new(crate::factory::admission::tests::factory_admission_tests::RecordingResolver::default()),
        policy,
        crate::factory_node_test_support::clock(),
      )),
    );
    let classification = crate::factory::admission::tests::factory_admission_tests::command(
      issue.project_id,
      request.intent.target.configuration_id,
      request.intent.target.repository_id,
    )
    .classification;
    let before = factory.mutation_evidence_counts().await;
    let error = publisher
      .handle_command(
        &context(),
        PublishIssueExecutionCommand {
          project_id: issue.project_id,
          issue_id: issue.id,
          request_id: request.id,
          classification,
        },
      )
      .await
      .expect_err("an assessment of another base cannot qualify this code");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected a snapshot diagnostic: {error:?}");
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/fields/size");
    assert_eq!(report.errors[0].code, "issue_snapshot_wrong_subject");
    assert_eq!(factory.mutation_evidence_counts().await, before);
    let preparation = factory
      .project_text_preparation(
        issue.project_id,
        &context().security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    assert!(preparation.admission.is_none());
    assert!(preparation.published_at.is_none());
  });
}

#[test]
fn manual_issue_snapshot_rejects_a_value_outside_its_pinned_project_definition() {
  run_ready(async {
    let (store, factory, issue, mut command) = snapshot_fixture().await;
    let mut definition = store
      .issue_properties(issue.project_id, Some(0))
      .await
      .unwrap()
      .definition;
    definition.priorities.push(IssuePropertyChoice {
      id: "critical".into(),
      name: "Critical".into(),
      retired: false,
    });
    store
      .publish_issue_properties(management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: issue.project_id,
          expected_version: 0,
          definition,
        },
        idempotency_key: key("new-priority"),
        published_at: at(32),
      }))
      .await
      .unwrap();
    let issue = store
      .edit_issue(management_mutation(EditIssue {
        intent: EditIssueIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          content: IssueContent {
            priority: Some(IssuePriority::Choice("critical".into())),
            ..issue.content
          },
        },
        idempotency_key: key("critical-priority"),
        edited_at: at(35),
      }))
      .await
      .unwrap()
      .issue;
    command.intent.expected_version = issue.version;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
    );
    let before = store.mutation_evidence_counts().await;
    let error = requests
      .handle_command(&context(), command)
      .await
      .expect_err("the v0 input cannot reinterpret a choice introduced in v1");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected a field diagnostic: {error:?}");
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/fields/priority");
    assert_eq!(report.errors[0].code, "issue_snapshot_incompatible_value");
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn manual_issue_snapshot_does_not_treat_an_empty_choice_catalog_as_arbitrary_text() {
  run_ready(async {
    let (store, factory, issue, mut command) = snapshot_fixture().await;
    let mut definition = store
      .issue_properties(issue.project_id, Some(0))
      .await
      .unwrap()
      .definition;
    definition.components.push(IssuePropertyChoice {
      id: "engine".into(),
      name: "Engine".into(),
      retired: false,
    });
    store
      .publish_issue_properties(management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: issue.project_id,
          expected_version: 0,
          definition,
        },
        idempotency_key: key("component-catalog"),
        published_at: at(32),
      }))
      .await
      .unwrap();
    let issue = store
      .edit_issue_planning(management_mutation(EditIssuePlanning {
        intent: EditIssuePlanningIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          planning: IssuePlanning {
            definition_version: 1,
            component: Some("engine".into()),
            ..issue.planning
          },
        },
        assignees: Default::default(),
        idempotency_key: key("select-component"),
        edited_at: at(33),
      }))
      .await
      .unwrap()
      .issue;
    let id = FactoryConfigurationId::generate();
    crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
      &factory,
      PublishedFactoryConfiguration {
        configuration: snapshot_configuration_with_component(issue.project_id, id, true),
        published_at: at(34),
      },
    )
    .unwrap();
    command.intent.target.configuration_id = id;
    command.intent.expected_version = issue.version;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let requests = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store.clone(), factory, policy)),
    );
    let before = store.mutation_evidence_counts().await;
    let error = requests
      .handle_command(&context(), command)
      .await
      .expect_err("an optional empty choice list permits omission, never an arbitrary ID");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected a field diagnostic: {error:?}");
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/fields/component");
    assert_eq!(report.errors[0].code, "issue_snapshot_incompatible_value");
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}
