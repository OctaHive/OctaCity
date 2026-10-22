//! Opt-in durable contracts against an explicitly selected disposable PostgreSQL service.
use super::*;
use octacity_server_store_postgres::PostgresStore;
use sqlx::{Connection as _, PgConnection, PgPool, postgres::PgConnectOptions};
use std::str::FromStr as _;

pub(crate) struct Database {
  admin: PgConnection,
  pool: PgPool,
  name: String,
}
impl Database {
  async fn new() -> Self {
    let options = PgConnectOptions::from_str(
      &std::env::var("OCTACITY_POSTGRES_URL").expect("explicit disposable PostgreSQL URL is required"),
    )
    .unwrap();
    let mut admin = PgConnection::connect_with(&options).await.unwrap();
    let name = format!("octacity_reporter_ordering_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
      .execute(&mut admin)
      .await
      .unwrap();
    let pool = PgPool::connect_with(options.database(&name)).await.unwrap();
    octacity_server_store_postgres::migrate(&pool).await.unwrap();
    Self { admin, pool, name }
  }
  async fn remigrate_ordering(&self) {
    // Only this test's private database: restore the pre-0063 technical schema
    // around genuine accepted history, then use the public migration runner.
    sqlx::query("DROP TABLE factory_source_projection_intents")
      .execute(&self.pool)
      .await
      .unwrap();
    sqlx::query("DROP TABLE factory_source_projection_journals")
      .execute(&self.pool)
      .await
      .unwrap();
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version=63")
      .execute(&self.pool)
      .await
      .unwrap();
    octacity_server_store_postgres::migrate(&self.pool).await.unwrap();
  }
  pub(crate) async fn cleanup(mut self) {
    self.pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
      "DROP DATABASE \"{}\" WITH (FORCE)",
      self.name
    )))
    .execute(&mut self.admin)
    .await
    .unwrap();
  }
}
pub(crate) async fn postgres_fixture(fixture: Fixture) -> (Fixture<PostgresStore>, Database) {
  let database = Database::new().await;
  let fixture = fixture.into_postgres(database.pool.clone()).await;
  (fixture, database)
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn ordered_projection_claims_and_hazards_survive_postgres_restart() {
  controlled_io(async move {
    let (fixture, profile) = fixture("jira").await;
    let (mut fixture, database) = postgres_fixture(fixture).await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let old_run = queue(&fixture, projector.clone()).await;
    let old = fixture.store.factory_run_snapshot(old_run).await.unwrap();
    let row = fixture
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(key("old-pg"), time(120), time(1120), 1)
          .unwrap()
          .only_kind(key("work.status.projection")),
      )
      .await
      .unwrap()
      .pop()
      .unwrap()
      .record;
    let mut request = SourceProjectionRequest {
      project_id: fixture.input.project_id(),
      work_id: Some(old.work.id()),
      operation_id: row.operation_id,
      claim: row.claim.unwrap(),
      observed_at: time(120),
      fields: vec![digest(1)],
      action: SourceProjectionAction::Register,
    };
    assert_eq!(
      fixture
        .store
        .coordinate_source_projection(request.clone())
        .await
        .unwrap(),
      SourceProjectionState::Ready {
        fields: vec![digest(1)],
        quiescent: true
      }
    );
    request.action = SourceProjectionAction::Begin {
      step_id: digest(70),
      fields: vec![digest(1)],
    };
    fixture
      .store
      .coordinate_source_projection(request.clone())
      .await
      .unwrap();
    next_input(&mut fixture, &profile).await;
    let new_run = queue_at(&fixture, projector, 200).await;
    let new = fixture.store.factory_run_snapshot(new_run).await.unwrap();
    let row = fixture
      .store
      .claim_factory_outbox(
        ClaimFactoryOutbox::new(key("new-pg"), time(220), time(1220), 1)
          .unwrap()
          .only_kind(key("work.status.projection")),
      )
      .await
      .unwrap()
      .pop()
      .unwrap()
      .record;
    let newer = SourceProjectionRequest {
      project_id: fixture.input.project_id(),
      work_id: Some(new.work.id()),
      operation_id: row.operation_id,
      claim: row.claim.unwrap(),
      observed_at: time(220),
      fields: vec![digest(1)],
      action: SourceProjectionAction::Register,
    };
    let restarted = PostgresStore::new(database.pool.clone());
    assert_eq!(
      restarted.coordinate_source_projection(newer.clone()).await.unwrap(),
      SourceProjectionState::Ready {
        fields: vec![digest(1)],
        quiescent: false
      }
    );
    request.observed_at = time(220);
    assert_eq!(
      restarted.coordinate_source_projection(request.clone()).await.unwrap(),
      SourceProjectionState::Superseded
    );
    request.action = SourceProjectionAction::Finish {
      step_id: digest(70),
      uncertain: false,
    };
    restarted.coordinate_source_projection(request).await.unwrap();
    assert_eq!(
      restarted.coordinate_source_projection(newer.clone()).await.unwrap(),
      SourceProjectionState::Ready {
        fields: vec![digest(1)],
        quiescent: true
      }
    );
    let mut foreign = newer;
    foreign.project_id = ProjectId::generate();
    assert!(restarted.coordinate_source_projection(foreign).await.is_err());
    assert_eq!(old.flow, restarted.factory_run_snapshot(old_run).await.unwrap().flow);
    assert_eq!(new.flow, restarted.factory_run_snapshot(new_run).await.unwrap().flow);
    drop(restarted);
    drop(fixture);
    database.cleanup().await;
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn postgres_late_writes_reconcile_through_the_verified_host_and_original_outbox() {
  controlled_io(async move {
    for lose_fence in [false, true] {
      let (fixture, profile) = fixture("github").await;
      let (fixture, database) = postgres_fixture(fixture).await;
      late_write_contract(fixture, profile, lose_fence, false).await;
      database.cleanup().await;
    }
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn postgres_projection_order_follows_acceptance_not_clock_timestamps() {
  controlled_io(async move {
    let (fixture, profile) = fixture("jira").await;
    let (fixture, database) = postgres_fixture(fixture).await;
    acceptance_order_contract(fixture, profile).await;
    database.cleanup().await;
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn migration_bootstraps_retained_accepted_history_without_reopening_the_flow() {
  controlled_io(async move {
    let (fixture, profile) = fixture("jira").await;
    let (mut fixture, database) = postgres_fixture(fixture).await;
    let projector = hosted(&fixture, vec![profile.clone()]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    database.remigrate_ordering().await;
    let orders: Vec<i64> =
      sqlx::query_scalar("SELECT sequence FROM factory_source_projection_intents ORDER BY sequence")
        .fetch_all(&database.pool)
        .await
        .unwrap();
    assert_eq!(orders, vec![1]);
    let reporter = recovery::worker(&fixture, projector.clone(), "legacy-reporter", 10000);
    assert_eq!(reporter.run_once(time(120)).await.unwrap().delivered, 1);
    let after = fixture.store.factory_run_snapshot(run).await.unwrap();
    assert_eq!(before.work, after.work);
    assert_eq!(before.run, after.run);
    assert_eq!(before.flow, after.flow);
    next_input(&mut fixture, &profile).await;
    queue_at(&fixture, projector, 200).await;
    let orders: Vec<i64> =
      sqlx::query_scalar("SELECT sequence FROM factory_source_projection_intents ORDER BY sequence")
        .fetch_all(&database.pool)
        .await
        .unwrap();
    assert_eq!(orders, vec![1, 2], "new acceptance continues the bootstrapped counter");
    drop(reporter);
    drop(fixture);
    database.cleanup().await;
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn postgres_separate_installations_share_native_item_order() {
  controlled_io(async move {
    let (fixture, profile) = fixture("github").await;
    let (fixture, database) = postgres_fixture(fixture).await;
    late_write_contract(fixture, profile, false, true).await;
    database.cleanup().await;
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn migration_ignores_status_actions_for_another_source_reporter_capability() {
  controlled_io(async move {
    let (fixture, original) = fixture("jira").await;
    let (mut fixture, database) = postgres_fixture(fixture).await;
    let other = mapped_installation_profile(
      &fixture,
      "other-reporter",
      fixture.profile.read.binding.installation.clone(),
      exact("other-status-adapter", 73),
    );
    let projector = hosted(&fixture, vec![original, other.clone()]);
    let run = queue(&fixture, projector.clone()).await;
    let before = fixture.store.factory_run_snapshot(run).await.unwrap();
    next_input(&mut fixture, &other).await;
    // This accepted configured action uses source-status, while its retained
    // source association selects other-status-adapter. It is not that Source's Reporter.
    let unrelated = queue_at(&fixture, projector.clone(), 200).await;
    let unrelated_before = fixture.store.factory_run_snapshot(unrelated).await.unwrap();
    assert_eq!(unrelated_before.flow.data.records.len(), 1);
    database.remigrate_ordering().await;
    let worker = recovery::worker(&fixture, projector, "mixed-legacy-adapters", 10000);
    assert_eq!(
      worker.run_once(time(220)).await.unwrap().delivered,
      1,
      "an unrelated adapter must not become an unresolved newer source intent"
    );
    assert_eq!(before.flow, fixture.store.factory_run_snapshot(run).await.unwrap().flow);
    assert_eq!(
      unrelated_before.flow,
      fixture.store.factory_run_snapshot(unrelated).await.unwrap().flow
    );
    drop(worker);
    drop(fixture);
    database.cleanup().await;
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn postgres_slow_admission_does_not_consume_the_late_write_window() {
  controlled_io(async move {
    let (fixture, profile) = fixture("github").await;
    let (fixture, database) = postgres_fixture(fixture).await;
    // Inject database latency at admission, outside the reporting contract. This
    // exceeds the unchanged 5s action budget without slowing any Reporter call.
    sqlx::raw_sql(
      "CREATE FUNCTION slow_test_admission() RETURNS trigger LANGUAGE plpgsql AS $$
       BEGIN PERFORM pg_sleep(5.1); RETURN NEW; END $$;
     CREATE TRIGGER slow_test_admission BEFORE INSERT ON factory_work_envelopes
       FOR EACH ROW EXECUTE FUNCTION slow_test_admission();",
    )
    .execute(&database.pool)
    .await
    .unwrap();
    late_write_contract(fixture, profile, false, true).await;
    database.cleanup().await;
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn postgres_slow_confirmation_does_not_spend_the_projection_budget() {
  controlled_io(async move {
    for expire in [false, true] {
      let (fixture, profile) = fixture("github").await;
      let (fixture, database) = postgres_fixture(fixture).await;
      let projector = hosted(&fixture, vec![profile]);
      let run = queue(&fixture, projector.clone()).await;
      let before = fixture.store.factory_run_snapshot(run).await.unwrap();
      let directory = fixture.root.path().join("github");
      std::fs::write(directory.join("reporter-mode.json"), br#"{"pause_after_apply":true}"#).unwrap();
      let worker = recovery::worker(&fixture, projector, "slow-confirmation", 10000);
      let task = tokio::spawn(async move { worker.run_once(time(120)).await });
      let ready = directory.join("reporter-awaiting-response");
      wait_for_provider(move || ready.exists()).await;
      // Real database latency exceeds the unchanged 2s process window. Wall-clock
      // scheduling must not decide the outcome of this controlled reporting journey.
      let observed = tokio::time::Instant::now();
      sqlx::query("SELECT pg_sleep(2.1)")
        .execute(&database.pool)
        .await
        .unwrap();
      assert_eq!(observed.elapsed(), Duration::ZERO);
      assert!(!task.is_finished(), "real latency cannot expire controlled time");
      if expire {
        tokio::time::advance(Duration::from_secs(2)).await;
      }
      std::fs::write(directory.join("reporter-release"), b"confirm").unwrap();
      let summary = task.await.unwrap().unwrap();
      if expire {
        assert_eq!((summary.delivered, summary.retrying, summary.failed), (0, 1, 0));
      } else {
        assert_eq!((summary.delivered, summary.retrying, summary.failed), (1, 0, 0));
      }
      assert_eq!(before.flow, fixture.store.factory_run_snapshot(run).await.unwrap().flow);
      assert_eq!(
        recovery::calls(&directory)
          .iter()
          .filter(|call| call["message"]["command"]["operation"] == "apply")
          .count(),
        1
      );
      drop(fixture);
      database.cleanup().await;
    }
  })
  .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "requires an explicitly selected disposable PostgreSQL service"]
async fn source_reporting_diagnostics_does_not_wait_for_a_run_writer() {
  controlled_io(async move {
    let (fixture, profile) = fixture("custom-tracker").await;
    let (fixture, database) = postgres_fixture(fixture).await;
    let run = queue(&fixture, hosted(&fixture, vec![profile])).await;
    // The writer holds its lock until the public query returns. A low database
    // lock timeout makes accidental locking reads fail without an elapsed-time assertion.
    let reads = sqlx::postgres::PgPoolOptions::new()
      .max_connections(1)
      .after_connect(|connection, _| {
        Box::pin(async move {
          sqlx::query("SET lock_timeout = '1ms'").execute(connection).await?;
          Ok(())
        })
      })
      .connect_with((*database.pool.connect_options()).clone())
      .await
      .unwrap();
    let queries = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(
        FactorySourceConfigurationHandlers::new(
          Arc::new(PostgresStore::new(reads.clone())),
          Arc::new(TrustedNetworkManagementPolicy),
          vec![],
        )
        .unwrap(),
      ),
    );
    let mut writer = database.pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM factory_runs WHERE id=$1 FOR UPDATE")
      .bind(run.as_uuid())
      .execute(&mut *writer)
      .await
      .unwrap();
    let result = queries
      .handle_query(
        &fixture.profile.read.context,
        GetWorkSourceDiagnosticsQuery {
          project_id: fixture.input.project_id(),
          target: WorkSourceScanTarget::Input {
            id: fixture.input.id(),
            version: fixture.input.version(),
          },
          after: None,
          limit: 1,
        },
      )
      .await;
    writer.rollback().await.unwrap();
    reads.close().await;
    database.cleanup().await;
    let diagnostic = result.expect("reporting diagnostics must finish while the Run writer owns its lock");
    let progress = serde_json::to_value(&diagnostic.admissions[0].reporting[0]).unwrap();
    assert_eq!(progress["origin"]["action"]["node"], "report-any-stage");
    assert_eq!(progress["origin"]["action"]["outcome"], "done");
    assert_eq!(diagnostic.admissions[0].association.run_id, run);
    assert_eq!(diagnostic.admissions[0].reporting.len(), 1);
    assert_eq!(
      diagnostic.admissions[0].reporting[0].state,
      WorkSourceReportingState::Pending
    );
  })
  .await;
}
