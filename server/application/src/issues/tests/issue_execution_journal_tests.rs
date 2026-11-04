use super::*;
use std::sync::Mutex;

struct Events(Mutex<Vec<DurableJobEvent>>);
#[async_trait::async_trait]
impl JobEventReadStore for Events {
  async fn read_job_events(&self, request: ReadJobEvents) -> Result<JobEventPage, StoreError> {
    assert!(request.visibility.allows(&request.job_id));
    let events: Vec<_> = self
      .0
      .lock()
      .unwrap()
      .iter()
      .filter(|event| event.sequence().get() > request.after_sequence)
      .take(usize::from(request.limit))
      .cloned()
      .collect();
    let cursor = events.last().map_or_else(
      || self.0.lock().unwrap().last().map_or(0, |event| event.sequence().get()),
      |event| event.sequence().get(),
    );
    Ok(JobEventPage { events, cursor })
  }
}
struct NoWait;
#[async_trait::async_trait]
impl JobEventWaiter for NoWait {
  async fn wait_for_job_events(&self, _: &str, _: u64, _: std::time::Duration) {
    panic!("journal reads never poll");
  }
}

struct UnreadableEvents;
#[async_trait::async_trait]
impl JobEventReadStore for UnreadableEvents {
  async fn read_job_events(&self, _: ReadJobEvents) -> Result<JobEventPage, StoreError> {
    panic!("denied journal reads must stop before event lookup or pagination");
  }
}

#[test]
fn issue_execution_journal_rechecks_each_owner_before_metadata_or_body_lookup() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    let selected = GetIssueExecutionJobQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      build_execution: fixture.execution.digest().unwrap(),
      job_id: fixture.execution.jobs()[0],
    };
    for blocked in [
      ManagementResourceKind::Issue,
      ManagementResourceKind::FactoryRun,
      ManagementResourceKind::Project,
      ManagementResourceKind::Build,
      ManagementResourceKind::Job,
      ManagementResourceKind::JobEvent,
    ] {
      let policy = Arc::new(ResourcePolicy { blocked });
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionJournalHandlers::new(
          Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
          jobs.clone(),
          Arc::new(JobEventLongPoll::new(Arc::new(UnreadableEvents), Arc::new(NoWait))),
        )),
      );
      assert!(
        queries
          .handle_query(
            &context(),
            ReadIssueExecutionJournalEventsQuery::try_new(selected.clone(), None, 1).unwrap()
          )
          .await
          .is_err(),
        "{blocked:?}"
      );
      assert!(
        queries
          .handle_query(
            &context(),
            GetIssueExecutionJournalEventQuery::try_new(selected.clone(), 1).unwrap()
          )
          .await
          .is_err(),
        "{blocked:?}"
      );
    }
  });
}

#[test]
fn issue_execution_journal_detail_never_substitutes_another_retained_event() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    let events = Arc::new(Events(Mutex::new(vec![
      operation_event(1, serde_json::json!({"kind":"command","exit_code":17,"failed":true})),
      operation_event(
        2,
        serde_json::json!({"kind":"file","path":"src/lib.rs","change":"modified","failed":false}),
      ),
    ])));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let service = Arc::new(IssueExecutionJournalHandlers::new(
      Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone())),
      jobs,
      Arc::new(JobEventLongPoll::new(events.clone(), Arc::new(NoWait))),
    ));
    let queries = AuthorizedQueryHandler::new(policy, service.clone());
    let query = GetIssueExecutionJournalEventQuery::try_new(
      GetIssueExecutionJobQuery {
        execution: GetIssueWorkExecutionQuery {
          project_id: issue.project_id,
          issue_id: issue.id,
          work_id: fixture.admission.work.id(),
        },
        build_execution: fixture.execution.digest().unwrap(),
        job_id: fixture.execution.jobs()[0],
      },
      1,
    )
    .unwrap();
    let detail = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.events[0].sequence, 1);
    assert_eq!(detail.events[0].payload["event"]["data"]["output"], "private output");
    let denied = AuthorizedQueryHandler::new(
      Arc::new(ResourcePolicy {
        blocked: ManagementResourceKind::JobEvent,
      }),
      service,
    );
    assert!(denied.handle_query(&context(), query.clone()).await.is_err());
    events.0.lock().unwrap().remove(0);
    assert_eq!(
      queries
        .handle_query(&context(), query)
        .await
        .unwrap_err()
        .application()
        .unwrap()
        .classification(),
      ApplicationFailure::NotFound
    );
  });
}
struct UnavailableEvents;
#[async_trait::async_trait]
impl JobEventReadStore for UnavailableEvents {
  async fn read_job_events(&self, _: ReadJobEvents) -> Result<JobEventPage, StoreError> {
    Err(StoreError::Unavailable)
  }
}
struct JobScopedEvents(octacity_server_domain::JobId);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for JobScopedEvents {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(
      if resource.kind() == ManagementResourceKind::JobEvent {
        ManagementVisibility::restricted([ManagementResource::instance(
          ManagementResourceKind::Job,
          crate::ManagementResourceIdentity::new(self.0.to_string()).unwrap(),
        )
        .unwrap()])
        .unwrap()
      } else {
        ManagementVisibility::all()
      },
    ))
  }
}

#[test]
fn issue_execution_journal_binds_continuation_to_typed_job_visibility_and_security_partition() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    let events = Arc::new(Events(Mutex::new(vec![operation_event(
      1,
      serde_json::json!({"kind":"command","exit_code":0,"failed":false}),
    )])));
    let policy = Arc::new(JobScopedEvents(fixture.execution.jobs()[0]));
    let service = Arc::new(IssueExecutionJournalHandlers::new(
      Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone())),
      jobs,
      Arc::new(JobEventLongPoll::new(events, Arc::new(NoWait))),
    ));
    let queries = AuthorizedQueryHandler::new(policy, service.clone());
    let selection = GetIssueExecutionJobQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      build_execution: fixture.execution.digest().unwrap(),
      job_id: fixture.execution.jobs()[0],
    };
    let first = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.events[0].sequence, 1);
    let continued = ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), Some(first.cursor), 1).unwrap();
    assert!(
      queries
        .handle_query(&verified_context("different-reader"), continued.clone())
        .await
        .is_err()
    );
    let broadened = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), service);
    assert!(broadened.handle_query(&context(), continued.clone()).await.is_err());
    let cursor = queries.handle_query(&context(), continued).await.unwrap().cursor;
    let mut other = selection;
    other.execution.work_id = octacity_server_factory::WorkEnvelopeId::generate();
    assert!(
      queries
        .handle_query(
          &context(),
          ReadIssueExecutionJournalEventsQuery::try_new(other, Some(cursor), 1).unwrap()
        )
        .await
        .is_err()
    );
  });
}

#[test]
fn issue_execution_journal_preserves_progress_and_cursor_when_event_storage_is_unavailable() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    let events = Arc::new(Events(Mutex::new(vec![operation_event(
      1,
      serde_json::json!({"kind":"command","exit_code":0,"failed":false}),
    )])));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let selection = GetIssueExecutionJobQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      build_execution: fixture.execution.digest().unwrap(),
      job_id: fixture.execution.jobs()[0],
    };
    let readable = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionJournalHandlers::new(
        executions.clone(),
        jobs.clone(),
        Arc::new(JobEventLongPoll::new(events, Arc::new(NoWait))),
      )),
    );
    let first = readable
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), None, 1).unwrap(),
      )
      .await
      .unwrap();
    let unavailable = AuthorizedQueryHandler::new(
      policy,
      Arc::new(IssueExecutionJournalHandlers::new(
        executions,
        jobs,
        Arc::new(JobEventLongPoll::new(Arc::new(UnavailableEvents), Arc::new(NoWait))),
      )),
    );
    let page = unavailable
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection, Some(first.cursor.clone()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.coverage, IssueResultCoverage::Unavailable);
    assert_eq!(page.cursor, first.cursor);
    assert!(page.events.is_empty());
    assert!(page.gaps.is_empty(), "a storage outage is not proof of lost events");
    assert_eq!(page.job.log.job_id, first.job.log.job_id);
  });
}

async fn journal_jobs(fixture: &testing::FactoryNodeJournalContractFixture) -> Arc<super::jobs::Jobs> {
  let mut builds = crate::factory_node_test_support::Builds::new();
  builds.acceptance.build_id = fixture.execution.build_id();
  builds.acceptance.attempt_id = fixture.execution.attempt_id();
  builds.acceptance.job_ids = fixture.execution.jobs().to_vec();
  let mut job = builds.job(fixture.execution.jobs()[0]).await.unwrap();
  job.queue = Some(JobQueueRecord {
    priority: 0,
    enqueued_at: at(30),
  });
  job.event_cursor = 2;
  Arc::new(super::jobs::Jobs(Mutex::new(job)))
}

#[test]
fn issue_execution_journal_marks_retained_gaps_and_does_not_replay_expired_ranges() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    let events = Arc::new(Events(Mutex::new(vec![operation_event(
      2,
      serde_json::json!({"kind":"command","exit_code":null,"failed":null}),
    )])));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionJournalHandlers::new(
        Arc::new(IssueExecutionHandlers::new(store, factory, policy)),
        jobs.clone(),
        Arc::new(JobEventLongPoll::new(events.clone(), Arc::new(NoWait))),
      )),
    );
    let selection = GetIssueExecutionJobQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      build_execution: fixture.execution.digest().unwrap(),
      job_id: fixture.execution.jobs()[0],
    };
    let first = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(&first).unwrap()["gaps"],
      serde_json::json!([{"from_sequence":1,"through_sequence":1}])
    );
    assert_eq!(first.events[0].sequence, 2);
    jobs.0.lock().unwrap().event_cursor = 4;
    events.0.lock().unwrap().clear();
    let later = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), Some(first.cursor), 1).unwrap(),
      )
      .await
      .unwrap();
    assert!(later.events.is_empty());
    assert_eq!(
      serde_json::to_value(&later).unwrap()["gaps"],
      serde_json::json!([{"from_sequence":3,"through_sequence":4}])
    );
    assert_ne!(later.cursor, "");
    let resumed = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection, Some(later.cursor.clone()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(serde_json::to_value(&resumed).unwrap()["gaps"], serde_json::json!([]));
    assert_eq!(resumed.cursor, later.cursor);
  });
}

fn operation_event(sequence: u64, operation: serde_json::Value) -> DurableJobEvent {
  DurableJobEvent::new(
    EventSequence::new(sequence).unwrap(),
    JobEventKind::new("progress").unwrap(),
    at(100 + sequence as i64),
    serde_json::json!({"source":"runner","event":{
      "schema_version":4,"sequence":sequence+10,"timestamp":"2026-10-21T00:00:00Z","category":"execution",
      "data":{"type":"progress","run_id":2,"step_id":9,"command_id":"command-1",
      "progress":{"message":"private prompt and transcript", "usage":{"tokens":999}, "observation":{
        "operation_id":"ab".repeat(32),"phase":"completed","observed_at_unix_ms":77,"operation":operation
      }},"arguments":"private arguments", "output":"private output"}
    }}),
  )
  .unwrap()
}

#[test]
fn issue_execution_journal_keeps_measured_zero_distinct_from_missing_usage_and_prose() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    jobs.0.lock().unwrap().event_cursor = 3;
    let payloads = [
      serde_json::json!({"source":"agent","event":{"type":"state_changed","state":"running"}}),
      serde_json::json!({"source":"agent","event":{"type":"resource_usage","usage":{
        "observed_at_unix_ms":77,"elapsed_ms":4,"cpu_time_ms":0,"memory_current_bytes":2,
        "memory_peak_bytes":3,"disk_current_bytes":5,"disk_peak_bytes":6,"io_read_bytes":0,"io_written_bytes":8,
        "network_received_bytes":null,"network_transmitted_bytes":0
      }}}),
      serde_json::json!({"source":"runner","event":{"schema_version":4,"sequence":13,"timestamp":"original","category":"execution",
        "data":{"type":"progress","progress":{"message":"ran a tool and accepted results","usage":{"tokens":999},"reserved_tokens":999}}
      }}),
    ];
    let events = Arc::new(Events(Mutex::new(
      payloads
        .into_iter()
        .enumerate()
        .map(|(index, payload)| {
          DurableJobEvent::new(
            EventSequence::new(index as u64 + 1).unwrap(),
            JobEventKind::new("progress").unwrap(),
            at(100),
            payload,
          )
          .unwrap()
        })
        .collect(),
    )));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionJournalHandlers::new(
        Arc::new(IssueExecutionHandlers::new(store, factory, policy)),
        jobs,
        Arc::new(JobEventLongPoll::new(events, Arc::new(NoWait))),
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(
          GetIssueExecutionJobQuery {
            execution: GetIssueWorkExecutionQuery {
              project_id: issue.project_id,
              issue_id: issue.id,
              work_id: fixture.admission.work.id(),
            },
            build_execution: fixture.execution.digest().unwrap(),
            job_id: fixture.execution.jobs()[0],
          },
          None,
          3,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    let value = serde_json::to_value(page).unwrap();
    assert_eq!(
      value["events"][0]["facts"],
      serde_json::json!({"kind":"lifecycle","state":"running"})
    );
    assert_eq!(value["events"][1]["facts"]["usage"]["cpu_time_ms"], 0);
    assert_eq!(
      value["events"][1]["facts"]["usage"]["network_received_bytes"],
      serde_json::Value::Null
    );
    assert_eq!(value["events"][1]["facts"]["usage"]["network_transmitted_bytes"], 0);
    assert_eq!(value["events"][2]["facts"], serde_json::json!({"kind":"not_reported"}));
    assert!(!value.to_string().contains("\"tokens\":999"));
    assert!(!value.to_string().contains("accepted results"));
  });
}

#[test]
fn issue_execution_journal_events_reconnect_exactly_without_private_bodies_or_invented_usage() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let jobs = journal_jobs(&fixture).await;
    let events = Arc::new(Events(Mutex::new(vec![
      operation_event(1, serde_json::json!({"kind":"command","exit_code":17,"failed":true})),
      operation_event(
        2,
        serde_json::json!({"kind":"file","path":"src/lib.rs","change":"modified","failed":false}),
      ),
    ])));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionJournalHandlers::new(
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory, policy)),
        jobs.clone(),
        Arc::new(JobEventLongPoll::new(events.clone(), Arc::new(NoWait))),
      )),
    );
    let selection = GetIssueExecutionJobQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      build_execution: fixture.execution.digest().unwrap(),
      job_id: fixture.execution.jobs()[0],
    };
    let before = store.mutation_evidence_counts().await;
    let first = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.job.log.attempt_id, fixture.execution.attempt_id());
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].sequence, 1);
    let value = serde_json::to_value(&first).unwrap();
    assert_eq!(value["events"][0]["facts"]["observation"]["operation"]["exit_code"], 17);
    assert_eq!(value["events"][0]["facts"]["observation"]["observed_at_unix_ms"], 77);
    assert_eq!(value["events"][0]["occurred_at_unix_ms"], 101);
    assert_eq!(value["events"][0]["facts"]["runner_sequence"], 11);
    assert_eq!(value["events"][0]["facts"]["run_id"], 2);
    assert_eq!(value["events"][0]["facts"]["step_id"], 9);
    assert_eq!(value["events"][0]["facts"]["command_id"], "command-1");
    for private in [
      "private prompt",
      "private arguments",
      "private output",
      "\"tokens\":999",
    ] {
      assert!(!value.to_string().contains(private));
    }
    let second = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), Some(first.cursor), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(second.events[0].sequence, 2);
    assert_eq!(
      serde_json::to_value(&second).unwrap()["events"][0]["facts"]["observation"]["operation"]["path"],
      "src/lib.rs"
    );
    let tail = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), Some(second.cursor.clone()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert!(tail.events.is_empty());
    assert_eq!(tail.cursor, second.cursor);
    events.0.lock().unwrap().push(operation_event(
      3,
      serde_json::json!({"kind":"command","exit_code":0,"failed":false}),
    ));
    let live = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), Some(tail.cursor), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(live.events[0].sequence, 3);
    assert_eq!(serde_json::to_value(&live).unwrap()["job_stale"], true);
    let live_detail = queries
      .handle_query(
        &context(),
        GetIssueExecutionJournalEventQuery::try_new(selection.clone(), 3).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(live_detail.events[0].sequence, 3);
    let still_stale = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection.clone(), Some(live.cursor.clone()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert!(still_stale.events.is_empty());
    assert_eq!(still_stale.cursor, live.cursor);
    assert!(still_stale.job_stale);
    jobs.0.lock().unwrap().event_cursor = 3;
    let caught_up = queries
      .handle_query(
        &context(),
        ReadIssueExecutionJournalEventsQuery::try_new(selection, Some(live.cursor), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(serde_json::to_value(&caught_up).unwrap()["job_stale"], false);
    assert!(caught_up.events.is_empty());
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_execution_journal_reads_selected_hierarchy_without_dispatch() {
  run_ready(async {
    let (store, factory, issue, command) = fixture().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      policy.clone(),
    ));
    let request = AuthorizedCommandHandler::new(policy.clone(), handlers.clone())
      .handle_command(&context(), command)
      .await
      .unwrap()
      .request;
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let query = ListIssueExecutionJournalQuery::try_new(
      GetIssueExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: request.id,
      },
      FactoryDiagnosticCollection::FlowRuns,
      None,
      1,
    )
    .unwrap();
    let preparing = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert!(matches!(preparing.execution.run, IssueRunObservation::Preparing));
    assert!(preparing.records.is_none());
    let admission = admit_request(factory.clone(), &request).await;
    store
      .associate_issue_work(management_mutation(AssociateIssueWork {
        project_id: issue.project_id,
        issue_id: issue.id,
        request_id: Some(request.id),
        admission: admission.clone(),
      }))
      .await
      .unwrap();
    let before = store.mutation_evidence_counts().await;
    let observed = queries.handle_query(&context(), query).await.unwrap();
    assert_eq!(observed.execution.request.id, request.id);
    assert_eq!(observed.execution.request.intent.expected_version, issue.version);
    let records = observed.records.unwrap();
    assert_eq!(records.association.work_id, admission.work.id());
    assert_eq!(records.association.run_id, admission.run.id());
    assert_eq!(records.coverage, IssueResultCoverage::Partial);
    assert_eq!(records.items.len(), 1);
    assert!(matches!(records.items[0], FactoryDiagnosticRecord::FlowRun(_)));
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      store
        .issue_executions(issue.project_id, issue.id, None, None, 10)
        .await
        .unwrap()
        .items
        .len(),
      1
    );
  });
}
