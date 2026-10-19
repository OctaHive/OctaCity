//! PostgreSQL parity and recovery through the existing Issue transition port.
use super::*;

fn action(issue: &IssueRevision, work_id: WorkEnvelopeId, run_id: FactoryRunId, operation: u8) -> TransitionIssue {
  let edge = "20000000-0000-4000-8000-000000000001".parse().unwrap();
  let outcome = FactoryKey::new("eligible").unwrap();
  TransitionIssue::pipeline(
    TransitionIssueIntent {
      project_id: issue.project_id,
      id: issue.id,
      expected_version: issue.version,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      expected_status_id: issue.status_id,
      transition_id: edge,
      reason: None,
    },
    AcceptedIssueStatusAction::for_selection(
      IssuePipelineStatusSelection {
        workflow_id: issue.workflow_id,
        workflow_version: issue.workflow_version,
        transition_id: edge,
        outcome: outcome.clone(),
      },
      IssuePipelineActionProvenance {
        issue_id: issue.id,
        requirements_version: issue.version,
        work_id,
        run_id,
        flow_run_id: FlowRunId::generate(),
        cycle_id: WorkflowCycleId::generate(),
        node_attempt_id: NodeAttemptId::generate(),
        operation_id: FactoryDigest::from_bytes([operation; 32]),
        candidate: None,
        evidence: vec![],
        outcome,
      },
    )
    .unwrap(),
    at(30),
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_missing_association_conflicts_without_a_move() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let original = issue(&store).await;
  let command = action(&original, WorkEnvelopeId::generate(), FactoryRunId::generate(), 1);
  assert!(matches!(
    store.transition_issue(management_mutation(command)).await,
    Err(StoreError::Conflict {
      entity: EntityKind::Issue
    })
  ));
  assert_eq!(
    store.issue(original.project_id, original.id, None).await.unwrap(),
    original
  );
  assert_eq!(
    store
      .issue_status_history(original.project_id, original.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  database.cleanup().await;
}

async fn linked(store: &PostgresStore, pool: &sqlx::PgPool) -> (IssueRevision, IssueWorkAssociation) {
  linked_at_schema(store, pool, false).await
}
async fn linked_at_schema(
  store: &PostgresStore,
  pool: &sqlx::PgPool,
  legacy: bool,
) -> (IssueRevision, IssueWorkAssociation) {
  let fixture = factory_node_journal_contract_fixture();
  let template = fixture.admission.work;
  let project = template.subject().project_id();
  store
    .create_project(management_mutation(CreateProject {
      id: project,
      parent_id: None,
      name: ProjectName::new("Local status actions").unwrap(),
      idempotency_key: key(&project.to_string()),
      created_at: at(1),
    }))
    .await
    .unwrap();
  let issue = store
    .create_issue(management_mutation(CreateIssue {
      id: IssueId::generate(),
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id: project,
        planning: Default::default(),
        content: IssueContent {
          title: "Reviewed task".into(),
          description: "Requirements".into(),
          acceptance: Some("Checks".into()),
          priority: None,
        },
      },
      idempotency_key: key("issue"),
      created_at: at(2),
    }))
    .await
    .unwrap()
    .issue;
  let repository = template.subject().repository_id();
  // Ordinary repository metadata only; no VCS/provider is dispatched by local status actions.
  sqlx::query("INSERT INTO repositories(id,project_id,name,created_at) VALUES($1,$2,'local',to_timestamp(0))")
    .bind(repository.as_uuid())
    .bind(project.as_uuid())
    .execute(pool)
    .await
    .unwrap();
  sqlx::query("INSERT INTO repository_versions(repository_id,version,vcs_integration_id,repository_locator,selection_policy,published_at) VALUES($1,1,$2,'https://example.test/local.git',$3,to_timestamp(0))")
    .bind(repository.as_uuid()).bind(IntegrationId::generate().as_uuid()).bind(sqlx::types::Json(RepositorySelectionPolicy {
      allowed_references: Default::default(), default_reference: None, allow_exact_revision: true,
    })).execute(pool).await.unwrap();
  let mut configured = support::factory_configuration::fixture(project);
  configured.draft.wip_limits = fixture.admission.flow.wip_limits();
  configured.draft.flow = Some(
    FactoryFlowConfiguration::new(
      fixture.admission.flow.closure().clone(),
      fixture.admission.flow.limits().clone(),
      vec![fixture.schema],
      Default::default(),
    )
    .unwrap(),
  );
  let configuration = FactoryConfiguration::publish(
    template.configuration().id(),
    template.configuration().version(),
    project,
    template.configuration().definition_digest(),
    configured.draft.clone(),
    &configured.choices,
  )
  .unwrap();
  store
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      idempotency_key: key("configuration"),
      published_at: at(3),
      intent: FactoryConfigurationMutationIntent::Create {
        id: configuration.reference().id(),
        project_id: project,
        definition_digest: configuration.reference().definition_digest(),
        draft: configured.draft,
      },
      configuration: configuration.clone(),
    }))
    .await
    .unwrap();
  let mut requested = request(&issue);
  requested.intent.target.configuration_id = configuration.reference().id();
  requested.intent.target.repository_id = repository;
  requested.intent.target.source = RepositorySourceSelection::ExactRevision(template.subject().base_revision().clone());
  let reviewed = store
    .request_issue_execution(management_mutation(requested))
    .await
    .unwrap()
    .request;
  let work = WorkEnvelope::new(
    template.id(),
    template.configuration().clone(),
    reviewed.external_identity(),
    template.subject().clone(),
    WorkArtifacts::new(
      FactoryArtifactReference::new(ArtifactId::generate(), reviewed.task_digest, reviewed.task_size).unwrap(),
      FactoryArtifactReference::new(
        ArtifactId::generate(),
        reviewed.acceptance_digest,
        reviewed.acceptance_size,
      )
      .unwrap(),
      vec![],
    )
    .unwrap(),
    WorkClassification::new(template.priority(), template.risk(), template.metadata().clone()),
  )
  .unwrap();
  let run = FactoryRun::admitted(fixture.admission.run.id(), &work);
  let admitted = store
    .admit_factory_work(management_mutation(AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: FactoryKey::new("manual").unwrap(),
          security_scope: ManagementSecurityScope::trusted_network(),
        },
        external_identity: reviewed.external_identity(),
        intent_digest: FactoryDigest::from_bytes([99; 32]),
        idempotency_key: reviewed.admission_key(),
      },
      repository_version: RepositoryVersion::INITIAL,
      work,
      flow: AdmittedFlow::from_configuration(&configuration, &run).unwrap(),
      run,
      admitted_at: at(4),
    }))
    .await
    .unwrap();
  let command = AssociateIssueWork {
    project_id: project,
    issue_id: issue.id,
    request_id: Some(reviewed.id),
    admission: admitted.admission,
  };
  let association = if legacy {
    // Previous-schema fixture; assertions still use the public transition/read ports after upgrade.
    let association = command.association(Some(&reviewed)).unwrap();
    sqlx::query("INSERT INTO issue_work_associations(work_id,run_id,project_id,issue_id,request_id,document) VALUES($1,$2,$3,$4,$5,$6)")
      .bind(association.work_id.as_uuid()).bind(association.run_id.as_uuid()).bind(project.as_uuid()).bind(issue.id.as_uuid())
      .bind(reviewed.id.as_uuid()).bind(sqlx::types::Json(&association)).execute(pool).await.unwrap();
    association
  } else {
    store
      .associate_issue_work(management_mutation(command))
      .await
      .unwrap()
      .association
  };
  (issue, association)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_manual_move_invalidates_even_a_freshly_retargeted_command_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, association) = linked(&store, &database.pool).await;
  let workflow = store
    .issue_workflow(issue.project_id, issue.workflow_id, Some(issue.workflow_version))
    .await
    .unwrap();
  let edge = &workflow.definition.transitions[0];
  let moved = store
    .transition_issue(management_mutation(TransitionIssue::manual(
      TransitionIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        workflow_id: issue.workflow_id,
        workflow_version: issue.workflow_version,
        expected_status_id: issue.status_id,
        transition_id: edge.id,
        reason: None,
      },
      key("manual"),
      at(20),
    )))
    .await
    .unwrap()
    .issue;
  let next_edge = workflow
    .definition
    .transitions
    .iter()
    .find(|e| e.from == moved.status_id && e.origins.contains(&IssueTransitionOrigin::Pipeline))
    .unwrap();
  let mut command = action(&moved, association.work_id, association.run_id, 2);
  command.intent.transition_id = next_edge.id;
  let mut provenance = command.pipeline_action().unwrap().provenance().clone();
  provenance.requirements_version = association.requirements_version.unwrap();
  command = TransitionIssue::pipeline(
    command.intent,
    AcceptedIssueStatusAction::for_selection(
      IssuePipelineStatusSelection {
        workflow_id: moved.workflow_id,
        workflow_version: moved.workflow_version,
        transition_id: next_edge.id,
        outcome: provenance.outcome.clone(),
      },
      provenance,
    )
    .unwrap(),
    at(30),
  );
  let restarted = PostgresStore::new(database.pool.clone());
  assert!(matches!(
    restarted.transition_issue(management_mutation(command)).await,
    Err(StoreError::Conflict { .. })
  ));
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), moved);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_rejected_command_cannot_rebind_after_restart() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, association) = linked(&store, &database.pool).await;
  let command = action(&issue, association.work_id, association.run_id, 3);
  let current = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: issue.version,
        content: IssueContent {
          priority: Some(8),
          ..issue.content.clone()
        },
      },
      idempotency_key: key("priority"),
      edited_at: at(20),
    }))
    .await
    .unwrap()
    .issue;
  assert!(matches!(
    store.transition_issue(management_mutation(command.clone())).await,
    Err(StoreError::Conflict { .. })
  ));
  let mut changed = command;
  changed.intent.expected_version = current.version;
  let restarted = PostgresStore::new(database.pool.clone());
  assert!(matches!(
    restarted.transition_issue(management_mutation(changed)).await,
    Err(StoreError::Conflict { .. })
  ));
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    current
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_commit_failure_recovers_the_original_command_and_one_receipt() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let (issue, association) = linked(&store, &database.pool).await;
  let command = action(&issue, association.work_id, association.run_id, 4);
  let operation = command.pipeline_action().unwrap().provenance().operation_id;
  sqlx::query(
    "ALTER TABLE outbox_entries ADD CONSTRAINT interrupt_status CHECK (topic <> 'issue.transitioned') NOT VALID",
  )
  .execute(&database.pool)
  .await
  .unwrap();
  assert!(matches!(
    store.transition_issue(management_mutation(command.clone())).await,
    Err(StoreError::Unavailable)
  ));
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  assert_eq!(
    restarted
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  let retained = restarted
    .issue_status_command(
      issue.project_id,
      issue.id,
      &ManagementSecurityScope::trusted_network(),
      operation,
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained.intent, command.intent);
  assert_eq!(&retained.provenance, command.pipeline_action().unwrap().provenance());
  assert!(
    restarted
      .issue_status_command(
        issue.project_id,
        issue.id,
        &ManagementSecurityScope::new("another-partition").unwrap(),
        operation
      )
      .await
      .unwrap()
      .is_none()
  );
  sqlx::query("ALTER TABLE outbox_entries DROP CONSTRAINT interrupt_status")
    .execute(&database.pool)
    .await
    .unwrap();
  let snapshot = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  let mut retry = retained.restore(&snapshot).unwrap();
  retry.moved_at = at(45);
  let applied = restarted.transition_issue(management_mutation(retry)).await.unwrap();
  let recovered = PostgresStore::new(database.pool.clone());
  let sealed = recovered
    .issue_status_command(
      issue.project_id,
      issue.id,
      &ManagementSecurityScope::trusted_network(),
      operation,
    )
    .await
    .unwrap()
    .unwrap();
  assert_eq!(sealed, retained);
  let replay = recovered
    .transition_issue(management_mutation(sealed.restore(&snapshot).unwrap()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.transition, applied.transition);
  assert_eq!(applied.transition.occurred_at, at(30));
  let audit = recovered
    .list_audit_facts(ListAuditFacts {
      visibility: AuditFactListVisibility::all(),
      query: AuditFactQuery {
        actor_kind: None,
        actor_identity: None,
        operation: Some("transition-issue".into()),
        target_kind: Some("issue".into()),
        target_identity: Some(issue.id.to_string()),
        request_identity: None,
        occurred_from: None,
        occurred_through: None,
        after: None,
        limit: 10,
      },
    })
    .await
    .unwrap();
  assert_eq!(audit.items.len(), 1);
  assert_eq!(
    audit.items[0].occurred_at,
    at(30),
    "recovery must preserve the original command time in audit as well as history"
  );

  assert_eq!(
    recovered
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  assert_eq!(
    recovered.factory_run_snapshot(association.run_id).await.unwrap().run,
    snapshot.run
  );
  database.cleanup().await;
}

fn configured_action(
  issue: &IssueRevision,
  association: &IssueWorkAssociation,
  edge: IssueTransitionId,
  operation: u8,
) -> TransitionIssue {
  let command = action(issue, association.work_id, association.run_id, operation);
  let mut intent = command.intent.clone();
  intent.transition_id = edge;
  let mut provenance = command.pipeline_action().unwrap().provenance().clone();
  provenance.requirements_version = association.requirements_version.unwrap();
  let selection = IssuePipelineStatusSelection {
    workflow_id: issue.workflow_id,
    workflow_version: issue.workflow_version,
    transition_id: edge,
    outcome: provenance.outcome.clone(),
  };
  TransitionIssue::pipeline(
    intent,
    AcceptedIssueStatusAction::for_selection(selection, provenance).unwrap(),
    at(30),
  )
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_sequential_moves_and_original_provenance_survive_later_edits() {
  let db = TestDatabase::migrated().await;
  let store = PostgresStore::new(db.pool.clone());
  let (issue, association) = linked(&store, &db.pool).await;
  let first = store
    .transition_issue(management_mutation(action(
      &issue,
      association.work_id,
      association.run_id,
      5,
    )))
    .await
    .unwrap();
  let command = configured_action(
    &first.issue,
    &association,
    "20000000-0000-4000-8000-000000000002".parse().unwrap(),
    6,
  );
  let second = store
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(
    second.issue.status_id.to_string(),
    "10000000-0000-4000-8000-000000000003"
  );
  let current = store
    .edit_issue(management_mutation(EditIssue {
      intent: EditIssueIntent {
        project_id: issue.project_id,
        id: issue.id,
        expected_version: second.issue.version,
        content: IssueContent {
          description: "Later requirements".into(),
          ..issue.content.clone()
        },
      },
      idempotency_key: key("later"),
      edited_at: at(40),
    }))
    .await
    .unwrap()
    .issue;
  let restarted = PostgresStore::new(db.pool.clone());
  let replay = restarted
    .transition_issue(management_mutation(command.clone()))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.issue, second.issue);
  assert_eq!(
    restarted.issue(issue.project_id, issue.id, None).await.unwrap(),
    current
  );
  let history = restarted
    .issue_status_history(issue.project_id, issue.id, None, 20)
    .await
    .unwrap();
  assert_eq!(history.items.len(), 3);
  assert_eq!(
    history.items[2].pipeline.as_ref().unwrap().requirements_version,
    issue.version
  );
  let mut changed = command.retained_pipeline_command().unwrap();
  changed.provenance.node_attempt_id = NodeAttemptId::generate();
  let snapshot = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  assert!(matches!(
    restarted
      .transition_issue(management_mutation(changed.restore(&snapshot).unwrap()))
      .await,
    Err(StoreError::Conflict { .. })
  ));
  db.cleanup().await;
}

async fn parallel_work(
  store: &PostgresStore,
  issue: &IssueRevision,
  association: &IssueWorkAssociation,
) -> IssueWorkAssociation {
  let original = store
    .issue_execution(issue.project_id, issue.id, association.request_id.unwrap())
    .await
    .unwrap();
  let mut command = request(issue);
  command.intent.target = original.intent.target;
  command.idempotency_key = key("parallel-execution");
  let reviewed = store
    .request_issue_execution(management_mutation(command))
    .await
    .unwrap()
    .request;
  let previous = store.factory_run_snapshot(association.run_id).await.unwrap();
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    previous.work.configuration().clone(),
    reviewed.external_identity(),
    previous.work.subject().clone(),
    previous.work.artifacts().clone(),
    WorkClassification::new(
      previous.work.priority(),
      previous.work.risk(),
      previous.work.metadata().clone(),
    ),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let configuration = store
    .factory_configuration_version(work.configuration().id(), work.configuration().version())
    .await
    .unwrap()
    .configuration;
  let admitted = store
    .admit_factory_work(management_mutation(AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: FactoryKey::new("manual").unwrap(),
          security_scope: ManagementSecurityScope::trusted_network(),
        },
        external_identity: reviewed.external_identity(),
        intent_digest: FactoryDigest::from_bytes([98; 32]),
        idempotency_key: reviewed.admission_key(),
      },
      repository_version: reviewed.intent.target.repository_version,
      work,
      flow: AdmittedFlow::from_configuration(&configuration, &run).unwrap(),
      run,
      admitted_at: at(5),
    }))
    .await
    .unwrap();
  store
    .associate_issue_work(management_mutation(AssociateIssueWork {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: Some(reviewed.id),
      admission: admitted.admission,
    }))
    .await
    .unwrap()
    .association
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_parallel_executions_have_one_winner_without_advancing_the_losing_marker() {
  let db = TestDatabase::migrated().await;
  let store = PostgresStore::new(db.pool.clone());
  let (issue, left) = linked(&store, &db.pool).await;
  let right = parallel_work(&store, &issue, &left).await;
  let a = action(&issue, left.work_id, left.run_id, 7);
  let b = action(&issue, right.work_id, right.run_id, 8);
  let (a, b) = tokio::join!(
    store.transition_issue(management_mutation(a)),
    store.transition_issue(management_mutation(b))
  );
  assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
  let loser = if a.is_err() { left } else { right };
  let current = store.issue(issue.project_id, issue.id, None).await.unwrap();
  let fresh = configured_action(
    &current,
    &loser,
    "20000000-0000-4000-8000-000000000002".parse().unwrap(),
    9,
  );
  assert!(matches!(
    PostgresStore::new(db.pool.clone())
      .transition_issue(management_mutation(fresh))
      .await,
    Err(StoreError::Conflict { .. })
  ));
  assert_eq!(
    store
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_upgrade_binds_the_original_reviewed_marker_instead_of_the_latest_head() {
  use octacity_server_store_postgres::{MIGRATOR, migrate, run_migrator};
  for requirements_changed in [false, true] {
    let db = TestDatabase::empty().await;
    let previous = sqlx::migrate::Migrator {
      migrations: std::borrow::Cow::Owned(MIGRATOR.iter().filter(|m| m.version <= 77).cloned().collect()),
      ..sqlx::migrate::Migrator::DEFAULT
    };
    run_migrator(&db.pool, &previous).await.unwrap();
    let store = PostgresStore::new(db.pool.clone());
    let (issue, association) = linked_at_schema(&store, &db.pool, true).await;
    let mut content = issue.content.clone();
    if requirements_changed {
      content.description = "Changed requirements".into();
    } else {
      content.priority = Some(5);
    }
    let current = store
      .edit_issue(management_mutation(EditIssue {
        intent: EditIssueIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          content,
        },
        idempotency_key: key("before-upgrade"),
        edited_at: at(20),
      }))
      .await
      .unwrap()
      .issue;
    migrate(&db.pool).await.unwrap();
    migrate(&db.pool).await.unwrap();
    let command = configured_action(
      &current,
      &association,
      "20000000-0000-4000-8000-000000000001".parse().unwrap(),
      10,
    );
    let result = PostgresStore::new(db.pool.clone())
      .transition_issue(management_mutation(command))
      .await;
    assert_eq!(
      result.is_ok(),
      !requirements_changed,
      "upgrade must not bless new requirements"
    );
    if requirements_changed {
      assert!(matches!(result, Err(StoreError::Conflict { .. })));
    }
    db.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_terminated_backend_keeps_the_sealed_command_without_partial_history() {
  let db = TestDatabase::migrated().await;
  let store = PostgresStore::new(db.pool.clone());
  let (issue, association) = linked(&store, &db.pool).await;
  let command = action(&issue, association.work_id, association.run_id, 11);
  let operation = command.pipeline_action().unwrap().provenance().operation_id;
  let mut blocker = db.pool.acquire().await.unwrap();
  sqlx::query("SELECT pg_advisory_lock(3707)")
    .execute(&mut *blocker)
    .await
    .unwrap();
  sqlx::query("CREATE FUNCTION pause_status_revision() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM pg_advisory_xact_lock(3707); RETURN NEW; END$$")
    .execute(&db.pool).await.unwrap();
  sqlx::query("CREATE TRIGGER pause_status_revision BEFORE INSERT ON issue_revisions FOR EACH ROW EXECUTE FUNCTION pause_status_revision()")
    .execute(&db.pool).await.unwrap();
  let options = db
    .pool
    .connect_options()
    .as_ref()
    .clone()
    .application_name("status-command-crash");
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with(options)
    .await
    .unwrap();
  let isolated = PostgresStore::new(pool.clone());
  let task = tokio::spawn(async move { isolated.transition_issue(management_mutation(command)).await });
  // Synchronize with the actual blocked statement, rather than a timing assumption.
  let pid=tokio::time::timeout(std::time::Duration::from_secs(10),async {
    loop {
      let pid:Option<i32>=sqlx::query_scalar("SELECT a.pid FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.datname=current_database() AND a.application_name='status-command-crash' AND l.locktype='advisory' AND NOT l.granted")
        .fetch_optional(&db.pool).await.unwrap();
      if let Some(pid)=pid {break pid;}
      tokio::task::yield_now().await;
    }
  }).await.unwrap();
  sqlx::query("SELECT pg_terminate_backend($1)")
    .bind(pid)
    .execute(&db.pool)
    .await
    .unwrap();
  assert!(matches!(task.await.unwrap(), Err(StoreError::Unavailable)));
  pool.close().await;
  sqlx::query("DROP TRIGGER pause_status_revision ON issue_revisions")
    .execute(&db.pool)
    .await
    .unwrap();
  sqlx::query("SELECT pg_advisory_unlock(3707)")
    .execute(&mut *blocker)
    .await
    .unwrap();
  let restarted = PostgresStore::new(db.pool.clone());
  assert_eq!(restarted.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
  assert_eq!(
    restarted
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    1
  );
  let retained = restarted
    .issue_status_command(
      issue.project_id,
      issue.id,
      &ManagementSecurityScope::trusted_network(),
      operation,
    )
    .await
    .unwrap()
    .unwrap();
  let snapshot = restarted.factory_run_snapshot(association.run_id).await.unwrap();
  restarted
    .transition_issue(management_mutation(retained.restore(&snapshot).unwrap()))
    .await
    .unwrap();
  assert_eq!(
    restarted
      .issue_status_history(issue.project_id, issue.id, None, 20)
      .await
      .unwrap()
      .items
      .len(),
    2
  );
  drop(blocker);
  db.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn pipeline_status_history_retains_the_supported_full_record_reference_inventory() {
  let db = TestDatabase::migrated().await;
  let store = PostgresStore::new(db.pool.clone());
  let (issue, association) = linked(&store, &db.pool).await;
  let command = action(&issue, association.work_id, association.run_id, 12);
  let workflow = store
    .issue_workflow(issue.project_id, issue.workflow_id, Some(issue.workflow_version))
    .await
    .unwrap();
  let audit = management_mutation(());
  let mut historical = issue
    .transitioned(&command, &workflow, &Default::default(), audit.audit().actor())
    .unwrap();
  // Trusted historical metadata can outlive the producing journal. This fixture
  // exercises storage capacity/readability, never evidence authority or a new action.
  historical.transition.pipeline.as_mut().unwrap().evidence = (0..256)
    .map(|i| FactoryDigest::content_sha256(format!("retained-record-{i}").as_bytes()))
    .collect();
  let mut tx = db.pool.begin().await.unwrap();
  sqlx::query("INSERT INTO issue_revisions(project_id,issue_id,version,workflow_id,workflow_version,document) VALUES($1,$2,2,$3,$4,$5)")
    .bind(issue.project_id.as_uuid()).bind(issue.id.as_uuid()).bind(issue.workflow_id.as_uuid())
    .bind(i64::try_from(issue.workflow_version.get()).unwrap()).bind(sqlx::types::Json(&historical.issue)).execute(&mut *tx).await.unwrap();
  sqlx::query("INSERT INTO issue_status_history(project_id,issue_id,version,document) VALUES($1,$2,2,$3)")
    .bind(issue.project_id.as_uuid())
    .bind(issue.id.as_uuid())
    .bind(sqlx::types::Json(&historical.transition))
    .execute(&mut *tx)
    .await
    .unwrap();
  sqlx::query("UPDATE issues SET current_version=2 WHERE id=$1")
    .bind(issue.id.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
  sqlx::query("UPDATE issue_work_associations SET status_applicability_version=2 WHERE work_id=$1")
    .bind(association.work_id.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
  tx.commit().await.unwrap();
  let history = PostgresStore::new(db.pool.clone())
    .issue_status_history(issue.project_id, issue.id, None, 20)
    .await
    .unwrap();
  assert_eq!(history.items.len(), 2);
  assert_eq!(history.items[1].pipeline.as_ref().unwrap().evidence.len(), 256);
  db.cleanup().await;
}
