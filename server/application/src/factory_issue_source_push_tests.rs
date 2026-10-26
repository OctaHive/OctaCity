//! Configured Issue actions retain their reporting review through the real node/command owner.
use super::*;
use octacity_server_domain::*;
#[test]
fn configured_issue_action_retains_exact_push_review_and_origin_without_provider_io() {
  crate::factory_admission_tests::run_ready(async {
    for linked in [false, true] {
      let base = store_testing::factory_node_journal_contract_fixture().admission.work;
      let project = base.subject().project_id();
      let issues = Arc::new(store_testing::InMemoryProjectStore::new());
      issues
        .create_project(store_testing::management_mutation(CreateProject {
          id: project,
          parent_id: None,
          name: ProjectName::new("Local actions").unwrap(),
          idempotency_key: IdempotencyKey::new("project").unwrap(),
          created_at: at(0),
        }))
        .await
        .unwrap();
      let issue = issues
        .create_issue(store_testing::management_mutation(CreateIssue {
          id: IssueId::generate(),
          intent: CreateIssueIntent {
            project_id: project,
            planning: Default::default(),
            content: IssueContent {
              title: "Manual task".into(),
              description: "Local requirements".into(),
              acceptance: Some("Exact acceptance".into()),
              priority: None,
            },
          },
          assignees: Default::default(),
          idempotency_key: IdempotencyKey::new("issue").unwrap(),
          created_at: at(1),
        }))
        .await
        .unwrap()
        .issue;
      let edge = issues
        .default_issue_workflow(project)
        .await
        .unwrap()
        .workflow
        .definition
        .transitions[0]
        .clone();
      let original_mapping = if linked {
        Some(install_push_mapping(&issues, &issue, edge.to, 1, None, true).await)
      } else {
        None
      };
      let request = issues
        .request_issue_execution(store_testing::management_mutation(RequestIssueExecution {
          id: IssueExecutionRequestId::generate(),
          intent: RequestIssueExecutionIntent {
            project_id: project,
            issue_id: issue.id,
            expected_version: issue.version,
            classification: None,
            target: IssueExecutionTarget {
              configuration_id: base.configuration().id(),
              configuration_version: base.configuration().version(),
              repository_id: base.subject().repository_id(),
              repository_version: RepositoryVersion::INITIAL,
              source: RepositorySourceSelection::DefaultReference,
              mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
            },
          },
          idempotency_key: IdempotencyKey::new("request").unwrap(),
          requested_at: at(2),
        }))
        .await
        .unwrap()
        .request;
      let work = WorkEnvelope::new(
        base.id(),
        base.configuration().clone(),
        request.external_identity(),
        base.subject().clone(),
        WorkArtifacts::new(
          FactoryArtifactReference::new(ArtifactId::generate(), request.task_digest, request.task_size).unwrap(),
          FactoryArtifactReference::new(
            ArtifactId::generate(),
            request.acceptance_digest,
            request.acceptance_size,
          )
          .unwrap(),
          vec![],
        )
        .unwrap(),
        WorkClassification::new(base.priority(), base.risk(), base.metadata().clone()),
      )
      .unwrap();
      let schema = FlowDataSchema::new(
        key("action.receipt"),
        key("v1"),
        FlowValueSchema::Object {
          fields: std::collections::BTreeMap::from([(
            key("receipt"),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 64,
              max_bytes: 64,
            }),
          )]),
        },
      )
      .unwrap();
      let selection = IssuePipelineStatusSelection {
        workflow_id: issue.workflow_id,
        workflow_version: issue.workflow_version,
        transition_id: edge.id,
        outcome: key("done"),
      };
      let h = harness_for_input(
        binding(
          "issue-action",
          "transition_issue",
          serde_json::to_value(selection).unwrap(),
        ),
        permissions("issue-action"),
        schema,
        work,
        FlowDataSchema::new(
          key("issue.input"),
          key("v1"),
          FlowValueSchema::Object {
            fields: Default::default(),
          },
        )
        .unwrap(),
        serde_json::json!({}),
      );
      let before = h.store.factory_run_snapshot(h.run).await.unwrap();
      issues
        .associate_issue_work(store_testing::management_mutation(AssociateIssueWork {
          project_id: project,
          issue_id: issue.id,
          request_id: Some(request.id),
          admission: PublishedFactoryAdmission {
            work: before.work.clone(),
            run: before.run.clone(),
            flow: before.admitted_flow.clone(),
            admitted_at: at(1),
          },
        }))
        .await
        .unwrap();
      let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
      let action = FactoryIssueStatusAction::new(
        reference("issue-action"),
        issues.clone(),
        h.store.clone(),
        context.clone(),
        Arc::new(TrustedNetworkManagementPolicy),
        crate::factory_node_test_support::clock(),
      );
      let runner = FactoryNodeRunner::new(
        h.store.clone(),
        Arc::new(FactoryFlowActionAdapter::new(Arc::new(action))),
        crate::factory_node_test_support::clock(),
      );
      runner
        .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
        .await
        .unwrap();
      let FactoryNodeStep::Completed(record) = runner.run_once(h.run, h.claim, h.target, None, at(12)).await.unwrap()
      else {
        panic!("configured local status action must complete");
      };
      let operation = serde_json::from_value(record.payload().value()["receipt"].clone()).unwrap();
      let retained = issues
        .issue_status_command(project, issue.id, &context.security_scope().to_store(), operation)
        .await
        .unwrap()
        .unwrap();
      assert_eq!(
        retained.push_review,
        Some(IssuePushReview {
          mapping: original_mapping.as_ref().map(|m| m.reference())
        })
      );
      let current = issues.issue(project, issue.id, None).await.unwrap();
      assert_eq!(current.status_id, edge.to);
      let pushes = issues.retained_issue_push_intents().unwrap();
      if linked {
        assert_eq!(pushes.len(), 1);
        let desired = pushes[0].desired.as_ref().unwrap();
        assert_eq!(desired.effects.as_ref().unwrap().add, vec!["managed-effect"]);
        let origin = desired.command.pipeline.as_ref().unwrap();
        assert_eq!(origin.work_id, before.work.id());
        assert_eq!(origin.run_id, h.run);
        assert_eq!(origin.flow_run_id, record.input().flow_run_id());
        assert_eq!(origin.cycle_id, record.input().cycle_id());
        assert_eq!(origin.node_attempt_id, record.node_attempt_id());
        assert_eq!(origin.operation_id, operation);
        assert_eq!(origin.requirements_version, issue.version);
      } else {
        assert!(pushes.is_empty());
      }
      // Replacing a mapping or attaching later cannot retrofit an already accepted action.
      install_push_mapping(
        &issues,
        &current,
        edge.to,
        20,
        original_mapping.map(|m| m.reference()),
        false,
      )
      .await;
      let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
      assert_eq!(
        issues
          .transition_issue(store_testing::management_mutation(retained.restore(&snapshot).unwrap()))
          .await
          .unwrap()
          .issue,
        current
      );
      assert_eq!(issues.retained_issue_push_intents().unwrap(), pushes);
    }
  });
}

pub(crate) async fn install_push_mapping(
  store: &store_testing::InMemoryProjectStore,
  issue: &IssueRevision,
  target: IssueStatusId,
  time: i64,
  expected: Option<ImmutableReference>,
  add: bool,
) -> PublishedIssueSourceMapping {
  let source = WorkSourceBinding {
    installation: reference("native-installation"),
    capability: reference("native-reader"),
    configuration: reference("read-scope"),
    resource: reference("native-container"),
  };
  if store
    .issue_source_link(
      issue.project_id,
      issue.id,
      &octacity_server_store::ManagementSecurityScope::trusted_network(),
    )
    .await
    .unwrap()
    .is_none()
  {
    store
      .attach_issue_source(store_testing::management_mutation(AttachIssueSource {
        intent: AttachIssueSourceIntent {
          selection: IssueSourceSelection {
            connection: None,
            project_id: issue.project_id,
            source: source.clone(),
            item: WorkSourceItem {
              resource: ExternalWorkIdentity::new("opaque-container").unwrap(),
              item: ExternalWorkIdentity::new("opaque-native-item").unwrap(),
            },
          },
          issue_id: issue.id,
          expected_version: issue.version,
          expected_field_policy: store.issue_field_policy(issue.project_id, issue.id).await.unwrap(),
        },
        observation: IssueSourceObservation {
          display_id: None,
          source: source.clone(),
          plugin: reference("native-reader"),
          provider_configuration: FactoryDigest::from_bytes([7; 32]),
          selection_digest: FactoryDigest::from_bytes([8; 32]),
          snapshot_digest: FactoryDigest::from_bytes([9; 32]),
          observed_at: at(time),
          expires_at: at(100),
        },
        idempotency_key: IdempotencyKey::new("attach-original").unwrap(),
        attached_at: at(time),
      }))
      .await
      .unwrap();
  }
  store
    .publish_issue_source_mapping(store_testing::management_mutation(PublishIssueSourceMapping {
      writer_access: None,
      intent: PublishIssueSourceMappingIntent {
        project_id: issue.project_id,
        issue_id: issue.id,
        expected_issue_version: issue.version,
        expected_mapping: expected,
        definition: IssueSourceMappingDefinition {
          source: source.clone(),
          reporter: Some(WorkSourceBinding {
            capability: reference("native-reporter"),
            configuration: reference("writer-scope"),
            ..source
          }),
          push_enabled: true,
          fields: [(IssueField::Status, IssueSourceFieldAuthority::OctacityOwnedPush)].into(),
          status: Some(IssueSourceStatusMapping {
            workflow: IssueWorkflowPin {
              id: issue.workflow_id,
              version: issue.workflow_version,
            },
            pull: vec![],
            push: Some(WorkSourceStatusMapping {
              statuses: vec![WorkSourceStatusEffects {
                status: target.to_string(),
                add: if add { vec!["managed-effect".into()] } else { vec![] },
                remove: if add { vec![] } else { vec!["managed-effect".into()] },
                state: None,
              }],
            }),
          }),
        },
      },
      native_resource: ExternalWorkIdentity::new("opaque-container").unwrap(),
      readable: WorkSourceReportingPolicy {
        labels: vec![],
        states: vec![],
      },
      writable: Some(WorkSourceReportingPolicy {
        labels: vec!["managed-effect".into()],
        states: vec![],
      }),
      idempotency_key: IdempotencyKey::new(format!("mapping-{time}")).unwrap(),
      published_at: at(time),
    }))
    .await
    .unwrap()
    .mapping
}
