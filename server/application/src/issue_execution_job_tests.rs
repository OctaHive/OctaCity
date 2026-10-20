use super::*;
use std::sync::Mutex;

struct Jobs(Mutex<JobRecord>);
#[async_trait::async_trait]
impl BuildQueryStore for Jobs {
  async fn job(&self, job: octacity_server_domain::JobId) -> Result<JobRecord, StoreError> {
    let row = self.0.lock().unwrap();
    assert_eq!(job, row.id());
    Ok(row.clone())
  }
  async fn build(&self, _: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    panic!("no Build aggregate read")
  }
  async fn attempt(&self, _: octacity_server_domain::AttemptId) -> Result<AttemptRecord, StoreError> {
    panic!("no Attempt aggregate read")
  }
  async fn latest_attempt(&self, _: octacity_server_domain::BuildId) -> Result<AttemptRecord, StoreError> {
    panic!("never substitute latest Attempt")
  }
}

#[test]
fn issue_execution_job_projects_actual_worker_independently_of_profile_and_exact_log_reference() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let mut builds = crate::factory_node_test_support::Builds::new();
    builds.acceptance.build_id = fixture.execution.build_id();
    builds.acceptance.attempt_id = fixture.execution.attempt_id();
    builds.acceptance.job_ids = fixture.execution.jobs().to_vec();
    let job_id = fixture.execution.jobs()[0];
    let mut job = builds.job(job_id).await.unwrap();
    job.queue = Some(JobQueueRecord {
      priority: 0,
      enqueued_at: at(30),
    });
    let jobs = Arc::new(Jobs(Mutex::new(job)));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      policy.clone(),
    ));
    let queries = AuthorizedQueryHandler::new(
      policy,
      Arc::new(IssueExecutionJobHandlers::new(executions, jobs.clone())),
    );
    let query = GetIssueExecutionJobQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      build_execution: fixture.execution.digest().unwrap(),
      job_id,
    };
    let queued = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(queued.job.state, octacity_server_job::JobState::Ready);
    assert_eq!(queued.job.assignment, None);
    assert_eq!(queued.log.build_id, fixture.execution.build_id());
    assert_eq!(queued.log.attempt_id, fixture.execution.attempt_id());
    assert_eq!(queued.log.job_id, job_id);
    assert_eq!(
      queued.profile.as_ref().unwrap().operation_id,
      fixture.execution.operation_id()
    );
    let agent_id = octacity_server_domain::AgentId::generate();
    {
      let mut job = jobs.0.lock().unwrap();
      job.state = octacity_server_job::JobState::Running;
      job.queue = None;
      job.assignment = Some(JobAssignmentRecord {
        pool_id: octacity_server_domain::PoolId::generate(),
        agent_id,
      });
      job.event_cursor = 17;
    }
    let running = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(running.job.assignment.unwrap().assigned_agent_id, agent_id);
    assert_eq!(running.profile, queued.profile);
    assert_eq!(running.log.event_cursor, 17);
    let value = serde_json::to_value(running).unwrap().to_string();
    for private in [
      "job_spec",
      "placement",
      "dependencies",
      "outputs",
      "lease",
      "transcript",
      "payload",
    ] {
      assert!(!value.contains(&format!("\"{private}\"")), "{private}");
    }
    for blocked in [ManagementResourceKind::Build, ManagementResourceKind::Job] {
      let policy = Arc::new(ResourcePolicy { blocked });
      let executions = Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        policy.clone(),
      ));
      let denied = AuthorizedQueryHandler::new(
        policy,
        Arc::new(IssueExecutionJobHandlers::new(executions, jobs.clone())),
      );
      assert!(denied.handle_query(&context(), query.clone()).await.is_err());
    }
    let mut foreign = query;
    foreign.build_execution = FactoryDigest::from_bytes([88; 32]);
    assert!(queries.handle_query(&context(), foreign).await.is_err());
  });
}
