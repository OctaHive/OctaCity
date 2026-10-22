//! Reviewed native-effect conflicts on the very card that owns both imported Work records.
use super::*;
use octacity_server_store::testing::management_mutation;
pub(super) async fn qualify(
  fixture: &Fixture<octacity_server_store_postgres::PostgresStore>,
  profile: &FactoryReporterProfile,
  connection: &WorkSourceConnectionRef,
  issue: IssueRevision,
  labels: &[&str; 2],
) -> IssueRevision {
  let store = &fixture.store;
  let project = issue.project_id;
  let mut base = issue.updated_at.unix_millis() + 10;
  let scope = octacity_server_store::ManagementSecurityScope::trusted_network();
  let mut mapping = store
    .issue_source_mapping(project, issue.id, &scope, None)
    .await
    .unwrap()
    .unwrap();
  let workflow = store
    .issue_workflow(project, issue.workflow_id, Some(issue.workflow_version))
    .await
    .unwrap();
  let edge = workflow
    .definition
    .transitions
    .iter()
    .find(|t| t.from == issue.status_id)
    .unwrap();
  let pin = ManagedIntegrationConnections::new(store.clone())
    .for_work_binding(connection, profile.binding(), IntegrationPurpose::TaskUpdate)
    .unwrap()
    .issue_writer_access();
  let publication = PublishIssueSourceMapping {
    writer_access: Some(pin),
    intent: PublishIssueSourceMappingIntent {
      project_id: project,
      issue_id: issue.id,
      expected_issue_version: issue.version,
      expected_mapping: Some(mapping.reference()),
      definition: IssueSourceMappingDefinition {
        source: mapping.definition.source.clone(),
        reporter: Some(profile.binding().clone()),
        push_enabled: true,
        fields: [
          (IssueField::Title, IssueSourceFieldAuthority::SourceOwnedPull),
          (IssueField::Status, IssueSourceFieldAuthority::OctacityOwnedPush),
        ]
        .into(),
        status: Some(IssueSourceStatusMapping {
          workflow: IssueWorkflowPin {
            id: issue.workflow_id,
            version: issue.workflow_version,
          },
          pull: vec![],
          push: Some(WorkSourceStatusMapping {
            statuses: vec![WorkSourceStatusEffects {
              status: edge.to.to_string(),
              add: vec![labels[0].into()],
              remove: vec![labels[1].into()],
              state: None,
            }],
          }),
        }),
      },
    },
    native_resource: ExternalWorkIdentity::new("native-container").unwrap(),
    readable: WorkSourceReportingPolicy {
      states: vec![],
      labels: vec![],
    },
    writable: Some(WorkSourceReportingPolicy {
      states: vec![],
      labels: labels.iter().map(|s| (*s).into()).collect(),
    }),
    idempotency_key: "pilot-push-policy".parse().unwrap(),
    published_at: time(base),
  };
  let reader = Arc::new(observation_reader(fixture, connection));
  let source = AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(IssueSourceHandlers::new(
      store.clone(),
      reader,
      Arc::new(TrustedNetworkManagementPolicy),
      fixture.clock.clone(),
    )),
  );
  for (index, condition) in [
    IssueSourceStatusCondition::Ambiguous,
    IssueSourceStatusCondition::ForbiddenTransition,
  ]
  .into_iter()
  .enumerate()
  {
    let mut pull = publication.clone();
    pull.writer_access = None;
    pull.intent.expected_mapping = Some(mapping.reference());
    pull.intent.definition.reporter = None;
    pull.intent.definition.push_enabled = false;
    pull.intent.definition.fields = [(IssueField::Status, IssueSourceFieldAuthority::SourceOwnedPull)].into();
    let status = pull.intent.definition.status.as_mut().unwrap();
    status.push = None;
    status.pull = if index == 0 {
      vec![edge.to, issue.status_id]
    } else {
      vec![edge.to]
    }
    .into_iter()
    .map(|status_id| IssueSourceStatusRule {
      native: WorkSourcePredicate {
        state: Some("native-state".into()),
        all_of: vec![],
        any_of: vec![],
        none_of: vec![],
      },
      status_id,
    })
    .collect();
    pull.readable.states = vec!["native-state".into()];
    pull.idempotency_key = format!("pilot-pull-{index}").parse().unwrap();
    pull.published_at = time(base);
    mapping = store
      .publish_issue_source_mapping(management_mutation(pull))
      .await
      .unwrap()
      .mapping;
    fixture.clock.0.store(base + 1, Ordering::SeqCst);
    let outcome = source
      .handle_command(
        &fixture.profile.read.context,
        RefreshIssueSourceCommand {
          project_id: project,
          issue_id: issue.id,
          read: observation_request(fixture),
          idempotency_key: format!("pilot-refresh-{index}").parse().unwrap(),
          refreshed_at: time(base + 1),
        },
      )
      .await
      .unwrap();
    assert_eq!(outcome.status.unwrap().condition, condition);
    assert_eq!(store.issue(project, issue.id, None).await.unwrap(), issue);
    base += 10;
  }
  let mut publication = publication;
  publication.intent.expected_mapping = Some(mapping.reference());
  publication.published_at = time(base);
  let mapping = store
    .publish_issue_source_mapping(management_mutation(publication.clone()))
    .await
    .unwrap()
    .mapping;
  let moved = store
    .transition_issue(management_mutation(TransitionIssue::manual(
      TransitionIssueIntent {
        project_id: project,
        id: issue.id,
        expected_version: issue.version,
        workflow_id: issue.workflow_id,
        workflow_version: issue.workflow_version,
        expected_status_id: issue.status_id,
        transition_id: edge.id,
        reason: None,
      },
      "pilot-push-move".parse().unwrap(),
      time(base + 1),
    )))
    .await
    .unwrap()
    .issue;
  let path = fixture
    .root
    .path()
    .join(fixture.profile.read.plugin.identity().as_str())
    .join("reporter-mode.json");
  std::fs::write(&path, serde_json::to_vec(&serde_json::json!({"source_effects":true,"conflict_on_apply":true,"fieldsets":{"original-effects":["b".repeat(64)]}})).unwrap()).unwrap();
  fixture.clock.0.store(base + 2, Ordering::SeqCst);
  let worker = reporting_worker(fixture, managed_projector(fixture, profile), "pilot-conflict", 10000);
  assert_eq!(worker.run_once(time(base + 2)).await.unwrap().failed, 1);
  let projections = store
    .issue_push_projections(project, issue.id, &scope, None, 10)
    .await
    .unwrap();
  let original = store
    .issue_push_projection(projections[0].record.operation_id)
    .await
    .unwrap();
  assert_eq!(
    original.record.failure,
    Some(FactoryOutboxFailure::Conflict),
    "state: {:?}, projections: {}",
    original.record.state,
    projections.len()
  );
  assert_eq!(store.issue(project, issue.id, None).await.unwrap(), moved);
  publication.intent.expected_issue_version = moved.version;
  publication.intent.expected_mapping = Some(mapping.reference());
  publication.idempotency_key = "pilot-reviewed-resolution".parse().unwrap();
  publication.published_at = time(base + 3);
  let resolution = ResolveIssueSourceConflict {
    publication,
    review: IssueSourceResolutionReview {
      operation_id: original.record.operation_id,
      expected_record: original.record.id,
    },
  };
  let resolved = store
    .resolve_issue_source_conflict(management_mutation(resolution.clone()))
    .await
    .unwrap();
  let next = resolved.projection.unwrap();
  assert_ne!(next, original.record.operation_id);
  std::fs::write(
    &path,
    serde_json::to_vec(&serde_json::json!({"source_effects":true,"fieldsets":{"original-effects":["b".repeat(64)]}}))
      .unwrap(),
  )
  .unwrap();
  fixture.clock.0.store(base + 4, Ordering::SeqCst);
  assert_eq!(
    reporting_worker(fixture, managed_projector(fixture, profile), "pilot-resolution", 10000)
      .run_once(time(base + 4))
      .await
      .unwrap()
      .delivered,
    1
  );
  assert_eq!(
    store
      .issue_push_projection(original.record.operation_id)
      .await
      .unwrap()
      .record,
    original.record
  );
  assert_eq!(
    store
      .resolve_issue_source_conflict(management_mutation(resolution))
      .await
      .unwrap()
      .projection,
    Some(next)
  );
  assert_eq!(store.issue(project, issue.id, None).await.unwrap(), moved);
  moved
}
