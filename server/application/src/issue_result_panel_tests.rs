use super::*;

#[test]
fn issue_checks_are_bound_to_the_original_visible_candidate_without_execution_effects() {
  run_ready(async {
    for independent_resource in [false, true] {
      let (store, factory, issue, mut fixture) = if independent_resource {
        associated_journal_for(independent_resource_journal_fixture()).await
      } else {
        associated_journal().await
      };
      let subject = octacity_server_factory::CandidateSubject::new(
        fixture.input.exact_resource().unwrap().clone(),
        octacity_server_domain::ImmutableRevision::new("original-head").unwrap(),
        FactoryDigest::from_bytes([91; 32]),
      );
      fixture.discovery = Some(
        serde_json::from_value(serde_json::json!({
          "kind":"candidate", "subject":subject
        }))
        .unwrap(),
      );
      testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
      let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      let record = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|r| r.discovery().is_some_and(|d| d.published_candidate().is_some()))
        .unwrap()
        .digest()
        .unwrap();
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      let page = queries
        .handle_query(
          &context(),
          ListIssueChecksQuery::try_new(
            GetIssueWorkExecutionQuery {
              project_id: issue.project_id,
              issue_id: issue.id,
              work_id: fixture.admission.work.id(),
            },
            record,
            None,
            1,
          )
          .unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(page.subject, subject);
      assert_eq!(page.association.run_id, snapshot.run.id());
      assert_eq!(page.coverage, IssueResultCoverage::Partial);
      assert!(
        page.items.is_empty(),
        "no check facts are invented without a declared check"
      );
      assert_eq!(factory.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    }
  });
}

#[test]
fn issue_checks_show_required_unproven_checks_without_using_completion_as_a_pass() {
  run_ready(async {
    let fixture = testing::factory_verification_journal_contract_fixture();
    let (store, factory, issue, mut fixture) = associated_journal_for(fixture).await;
    let subject = octacity_server_factory::CandidateSubject::new(
      fixture.admission.work.subject().repository().unwrap().clone(),
      octacity_server_domain::ImmutableRevision::new("candidate").unwrap(),
      FactoryDigest::from_bytes([95; 32]),
    );
    fixture.discovery =
      Some(serde_json::from_value(serde_json::json!({"kind":"candidate","subject":subject})).unwrap());
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.discovery().is_some())
      .unwrap()
      .digest()
      .unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store, factory, policy)),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueChecksQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          record,
          None,
          20,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].name, "accessibility_audit");
    assert!(page.items[0].required);
    assert_eq!(
      page.items[0].state,
      IssueCheckState::Indeterminate,
      "a completed Build without a typed qualified verification observation is not pass"
    );
  });
}

struct UnavailableArtifacts;
#[async_trait::async_trait]
impl IssueArtifactReader for UnavailableArtifacts {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    Err(ApplicationError::unavailable())
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("listing never reads bytes")
  }
}
#[test]
fn issue_artifacts_retain_original_input_references_without_fabricating_build_provenance() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(UnavailableArtifacts),
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          20,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 2);
    for row in &page.items {
      assert_eq!(row.group, IssueArtifactGroup::OriginalInput);
      assert_eq!(row.availability, IssueArtifactAvailability::Unavailable);
      assert!(
        row.producer.is_none(),
        "original task/acceptance is not a produced Build output"
      );
    }
    let ids: std::collections::BTreeSet<_> = page.items.iter().map(|r| r.artifact.artifact_id()).collect();
    assert_eq!(
      ids,
      [
        fixture.admission.work.artifacts().task().artifact_id(),
        fixture.admission.work.artifacts().acceptance().artifact_id()
      ]
      .into_iter()
      .collect()
    );
    assert_eq!(page.coverage, IssueResultCoverage::Partial);
  });
}
struct FixedPanelClock;
impl FactoryExecutionClock for FixedPanelClock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
    Ok(octacity_server_domain::Timestamp::from_unix_millis(10).unwrap())
  }
}

#[test]
fn issue_artifact_pages_bind_original_execution_visibility_and_run_version() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      policy.clone(),
    ));
    let handlers = Arc::new(IssueArtifactHandlers::new(
      executions,
      Arc::new(UnavailableArtifacts),
      Arc::new(FixedPanelClock),
      policy.clone(),
    ));
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let selection = GetIssueWorkExecutionQuery {
      project_id: issue.project_id,
      issue_id: issue.id,
      work_id: fixture.admission.work.id(),
    };
    let first = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(selection.clone(), None, 1).unwrap(),
      )
      .await
      .unwrap();
    let cursor = first.next_cursor.clone().expect("second original input remains");
    let second = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(selection.clone(), Some(cursor.clone()), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_ne!(
      first.items[0].artifact.artifact_id(),
      second.items[0].artifact.artifact_id()
    );
    assert!(second.next_cursor.is_none());
    assert!(
      queries
        .handle_query(
          &context(),
          ListIssueArtifactsQuery::try_new(selection.clone(), Some("untrusted-cursor".into()), 1).unwrap()
        )
        .await
        .is_err()
    );
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let stale = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(selection, Some(cursor), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(stale.coverage, IssueResultCoverage::Stale);
    assert!(stale.items.is_empty());
  });
}

struct TextArtifacts;
#[async_trait::async_trait]
impl IssueArtifactReader for TextArtifacts {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    Ok(IssueArtifactMetadata {
      name: "requirements.txt".into(),
      media_type: "text/plain".into(),
      origin: IssueArtifactOrigin::ProjectInput,
      availability: IssueArtifactAvailability::Available,
      published_at: Some(octacity_server_domain::Timestamp::from_unix_millis(1).unwrap()),
      expires_at: None,
    })
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    Ok(b"Explicit retained text <script>ignored</script>".to_vec())
  }
}
#[test]
fn issue_artifact_content_requires_exact_original_membership_and_independent_download_authority() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(
      store.clone(),
      factory.clone(),
      policy.clone(),
    ));
    let handlers = Arc::new(IssueArtifactHandlers::new(
      executions,
      Arc::new(TextArtifacts),
      Arc::new(FixedPanelClock),
      policy.clone(),
    ));
    let queries = AuthorizedQueryHandler::new(policy, handlers);
    let execution = GetIssueWorkExecutionQuery {
      project_id: issue.project_id,
      issue_id: issue.id,
      work_id: fixture.admission.work.id(),
    };
    let content = queries
      .handle_query(
        &context(),
        ReadIssueArtifactContentQuery {
          execution: execution.clone(),
          artifact_id: fixture.admission.work.artifacts().task().artifact_id(),
          mode: IssueArtifactReadMode::Preview,
        },
      )
      .await
      .unwrap();
    assert_eq!(content.media_type, "text/plain; charset=utf-8");
    assert_eq!(content.bytes, b"Explicit retained text <script>ignored</script>");
    assert!(
      queries
        .handle_query(
          &context(),
          ReadIssueArtifactContentQuery {
            execution: execution.clone(),
            artifact_id: octacity_server_domain::ArtifactId::generate(),
            mode: IssueArtifactReadMode::Preview
          }
        )
        .await
        .is_err()
    );
    let blocked = Arc::new(ResourcePolicy {
      blocked: ManagementResourceKind::Artifact,
    });
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, blocked.clone()));
    let blocked = AuthorizedQueryHandler::new(
      blocked.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(UnavailableArtifacts),
        Arc::new(FixedPanelClock),
        blocked,
      )),
    );
    assert!(
      blocked
        .handle_query(
          &context(),
          ReadIssueArtifactContentQuery {
            execution,
            artifact_id: fixture.admission.work.artifacts().task().artifact_id(),
            mode: IssueArtifactReadMode::Preview
          }
        )
        .await
        .is_err()
    );
  });
}

struct PreviewArtifacts {
  media_type: &'static str,
  bytes: Vec<u8>,
}
#[async_trait::async_trait]
impl IssueArtifactReader for PreviewArtifacts {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    Ok(IssueArtifactMetadata {
      name: "safe-preview".into(),
      media_type: self.media_type.into(),
      origin: IssueArtifactOrigin::BuildOutput,
      availability: IssueArtifactAvailability::Available,
      published_at: None,
      expires_at: None,
    })
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    Ok(self.bytes.clone())
  }
}
#[test]
fn issue_artifact_preview_decodes_raster_and_rejects_executable_or_disguised_bytes() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let request = ReadIssueArtifactContentQuery {
      execution: GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      artifact_id: fixture.admission.work.artifacts().task().artifact_id(),
      mode: IssueArtifactReadMode::Preview,
    };
    for (media, bytes, allowed) in [
      (
        "image/png",
        include_bytes!("../fixtures/issue-preview.png").to_vec(),
        true,
      ),
      ("image/png", b"<svg onload='unsafe()'></svg>".to_vec(), false),
      ("image/svg+xml", b"<svg></svg>".to_vec(), false),
      ("text/html", b"<script>unsafe()</script>".to_vec(), false),
    ] {
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueArtifactHandlers::new(
          executions.clone(),
          Arc::new(PreviewArtifacts {
            media_type: media,
            bytes,
          }),
          Arc::new(FixedPanelClock),
          policy.clone(),
        )),
      );
      let result = queries.handle_query(&context(), request.clone()).await;
      assert_eq!(result.is_ok(), allowed, "{media}");
      if let Ok(preview) = result {
        assert_eq!(preview.media_type, "image/png");
        assert!(preview.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
      }
    }
  });
}

#[test]
fn issue_artifacts_include_retained_reports_with_exact_execution_producer() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    let record = &snapshot.flow.data.records[0];
    let output = &record.build_provenance().unwrap().outputs()[0];
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(UnavailableArtifacts),
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          20,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    let report = page
      .items
      .iter()
      .find(|r| r.artifact == output.artifact)
      .expect("retained produced report remains visible even without bytes");
    assert_eq!(report.group, IssueArtifactGroup::Report);
    assert_eq!(report.producer.as_ref(), Some(&output.producer));
    assert_eq!(report.record, Some(record.digest().unwrap()));
    assert_eq!(report.availability, IssueArtifactAvailability::Unavailable);
  });
}

#[test]
fn issue_checks_qualify_typed_outcomes_and_preserve_original_producer_and_head() {
  run_ready(async {
    for (outcome, expected) in [
      (
        octacity_server_factory::DeterministicGateOutcome::Passed,
        IssueCheckState::Pass,
      ),
      (
        octacity_server_factory::DeterministicGateOutcome::Failed,
        IssueCheckState::Fail,
      ),
      (
        octacity_server_factory::DeterministicGateOutcome::Indeterminate,
        IssueCheckState::Indeterminate,
      ),
    ] {
      let (store, factory, issue, mut fixture) =
        associated_journal_for(testing::factory_verified_check_journal_fixture(outcome)).await;
      let subject = octacity_server_factory::CandidateSubject::new(
        fixture.admission.work.subject().repository().unwrap().clone(),
        octacity_server_domain::ImmutableRevision::new("candidate").unwrap(),
        FactoryDigest::from_bytes([95; 32]),
      );
      fixture.discovery = Some(octacity_server_factory::FlowDiscoveryFacts::candidate(subject.clone()));
      testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
      let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      let record = &snapshot.flow.data.records[0];
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store, factory, policy)),
      );
      let page = queries
        .handle_query(
          &context(),
          ListIssueChecksQuery::try_new(
            GetIssueWorkExecutionQuery {
              project_id: issue.project_id,
              issue_id: issue.id,
              work_id: fixture.admission.work.id(),
            },
            record.digest().unwrap(),
            None,
            20,
          )
          .unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(page.subject, subject);
      assert_eq!(page.items[0].state, expected);
      assert_eq!(
        page.items[0].producer.as_ref(),
        Some(&record.build_provenance().unwrap().outputs()[0].producer)
      );
      assert_eq!(page.items[0].record, Some(record.digest().unwrap()));
      assert_eq!(page.items[0].observed_at, Some(record.observed_at()));
    }
  });
}

struct ExpiringPanelClock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for ExpiringPanelClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(Timestamp::from_unix_millis(self.0.load(std::sync::atomic::Ordering::SeqCst)).unwrap())
  }
}
struct ExpiringContent(Arc<ExpiringPanelClock>);
#[async_trait::async_trait]
impl IssueArtifactReader for ExpiringContent {
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    let mut metadata = TextArtifacts.metadata(request).await?;
    metadata.expires_at = Some(Timestamp::from_unix_millis(11).unwrap());
    Ok(metadata)
  }
  async fn content(&self, request: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    self.0.0.store(12, std::sync::atomic::Ordering::SeqCst);
    TextArtifacts.content(request).await
  }
}
#[test]
fn issue_artifact_content_expiring_during_read_is_not_returned() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let clock = Arc::new(ExpiringPanelClock(std::sync::atomic::AtomicI64::new(10)));
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(ExpiringContent(clock.clone())),
        clock,
        policy,
      )),
    );
    let result = queries
      .handle_query(
        &context(),
        ReadIssueArtifactContentQuery {
          execution: GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          artifact_id: fixture.admission.work.artifacts().task().artifact_id(),
          mode: IssueArtifactReadMode::Download,
        },
      )
      .await;
    assert!(
      result.is_err(),
      "retention must still hold when bytes leave the application"
    );
  });
}

struct AdvancingInventory {
  factory: Arc<testing::InMemoryFactoryConfigurationStore>,
  fixture: testing::FactoryNodeJournalContractFixture,
  changed: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl IssueArtifactReader for AdvancingInventory {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    if !self.changed.swap(true, std::sync::atomic::Ordering::SeqCst) {
      testing::verify_factory_node_journal_contract(&*self.factory, &*self.factory, &self.fixture).await;
    }
    Err(ApplicationError::unavailable())
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("inventory must not fetch content")
  }
}
#[test]
fn issue_artifacts_discard_a_page_when_the_run_changes_during_metadata_reads() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory.clone(), policy.clone()));
    let reader = Arc::new(AdvancingInventory {
      factory,
      fixture: fixture.clone(),
      changed: std::sync::atomic::AtomicBool::new(false),
    });
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        reader,
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.coverage, IssueResultCoverage::Stale);
    assert!(
      page.items.is_empty() && page.next_cursor.is_none(),
      "a torn inventory must not issue a usable cursor"
    );
  });
}

struct CountingInventory(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl IssueArtifactReader for CountingInventory {
  async fn metadata(&self, _: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Err(ApplicationError::unavailable())
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("metadata pagination must not read bytes")
  }
}
#[test]
fn issue_artifact_inventory_expands_metadata_only_for_the_requested_page() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let reader = Arc::new(CountingInventory(std::sync::atomic::AtomicUsize::new(0)));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        reader.clone(),
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(
      reader.0.load(std::sync::atomic::Ordering::SeqCst),
      1,
      "unrequested metadata is not fetched"
    );
  });
}

#[test]
fn issue_checks_retain_missing_and_active_check_lifecycle_without_acceptance() {
  use octacity_server_factory::*;
  use octacity_server_store::FactoryBudgetRecord;
  use octacity_server_store::testing as journal_testing;
  run_ready(async {
    let (store, factory, issue, mut fixture) =
      associated_journal_for(journal_testing::factory_two_check_journal_fixture()).await;
    let subject = fixture.input.context().unwrap().subject().candidate().unwrap().clone();
    fixture.discovery = Some(FlowDiscoveryFacts::candidate(subject));
    journal_testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    let record = snapshot.flow.data.records[0].digest().unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueExecutionHandlers::new(store, factory.clone(), policy)),
    );
    let query = ListIssueChecksQuery::try_new(
      GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      },
      record,
      None,
      20,
    )
    .unwrap();
    let first = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(
      first.items.iter().find(|c| c.name == "second_check").unwrap().state,
      IssueCheckState::Missing
    );
    let admitted = &snapshot.admitted_flow;
    let input = FlowNodeInput::prepare(FlowInputPreparation {
      work: &snapshot.work,
      admitted,
      flow: admitted.root_run(),
      cycle: admitted.initial_cycle(),
      node: "second_check".parse().unwrap(),
      schema: &fixture.schema,
      history: snapshot.flow.runtime(),
      records: &snapshot.flow.data.records,
      inputs: &snapshot.flow.data.inputs,
      incoming: None,
    })
    .unwrap()
    .with_context(fixture.input.context().unwrap().clone(), vec![])
    .unwrap();
    let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
    let node = definition.node(input.node()).unwrap();
    let claim = snapshot.current_claim.as_ref().unwrap();
    let ownership = FactoryClaimOwnership::new(claim.owner.clone(), claim.claim);
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: input.node().clone(),
        node_kind: node.kind(),
        number: NodeAttemptNumber::new(2).unwrap(),
        input_digest: input.digest().unwrap(),
        budget: node.budget(),
        started_at: at(30),
        deadline: at(80),
        execution: fixture.attempt.execution().clone(),
        ownership: ownership.clone(),
      },
    )
    .unwrap();
    let mut start = journal_testing::factory_flow_contract_transition(&snapshot, claim, 30);
    start.append.flow.attempts.push(attempt.clone());
    start.append.flow.data.inputs.push(input.clone());
    start.append.flow.data.build_intents.push(
      FlowBuildIntent::new(
        input,
        admitted,
        attempt.clone(),
        snapshot
          .budgets
          .iter()
          .find(|row| row.id == snapshot.current.budget_id)
          .unwrap()
          .usage,
        i64::from(snapshot.work.priority().get()),
      )
      .unwrap(),
    );
    start.budget.usage.attempts += 1;
    start.budget = FactoryBudgetRecord::new(snapshot.run.id(), start.next_run.version(), start.budget.usage, at(30));
    start.current.budget_id = start.budget.id;
    factory.commit_factory_run_transition(start).await.unwrap();
    let active = queries.handle_query(&context(), query.clone()).await.unwrap();
    assert_eq!(
      active.items.iter().find(|c| c.name == "second_check").unwrap().state,
      IssueCheckState::Running
    );
    let snapshot = factory.factory_run_snapshot(snapshot.run.id()).await.unwrap();
    let mut stop =
      journal_testing::factory_flow_contract_transition(&snapshot, snapshot.current_claim.as_ref().unwrap(), 31);
    stop.append.flow.data.stops.push(
      FlowNodeStop::new(
        &attempt,
        ownership,
        FlowNodeStopReason::ExecutionFailed,
        at(31),
        BudgetUsage::default(),
        None,
      )
      .unwrap(),
    );
    factory.commit_factory_run_transition(stop).await.unwrap();
    let failed = queries.handle_query(&context(), query).await.unwrap();
    assert_eq!(
      failed.items.iter().find(|c| c.name == "second_check").unwrap().state,
      IssueCheckState::Fail
    );
  });
}

struct RevocableArtifactPolicy(std::sync::atomic::AtomicBool);
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for RevocableArtifactPolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == ManagementResourceKind::Artifact && self.0.load(std::sync::atomic::Ordering::SeqCst) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
  }
}
struct RevokingInventory(Arc<RevocableArtifactPolicy>);
#[async_trait::async_trait]
impl IssueArtifactReader for RevokingInventory {
  async fn metadata(&self, request: ReadFactoryArtifact) -> Result<IssueArtifactMetadata, ApplicationError> {
    self.0.0.store(true, std::sync::atomic::Ordering::SeqCst);
    TextArtifacts.metadata(request).await
  }
  async fn content(&self, _: ReadFactoryArtifact) -> Result<Vec<u8>, ApplicationError> {
    panic!("inventory never fetches content")
  }
}
#[test]
fn issue_artifact_inventory_discards_metadata_when_visibility_is_revoked_during_expansion() {
  run_ready(async {
    let (store, factory, issue, fixture) = associated_journal().await;
    let policy = Arc::new(RevocableArtifactPolicy(std::sync::atomic::AtomicBool::new(false)));
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(RevokingInventory(policy.clone())),
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(page.coverage, IssueResultCoverage::Unavailable);
    assert!(
      page.items.is_empty() && page.next_cursor.is_none(),
      "revoked metadata must not leave the application"
    );
  });
}

#[test]
fn issue_artifacts_preserve_the_original_producing_input_candidate_without_discovery_facts() {
  run_ready(async {
    let (store, factory, issue, fixture) =
      associated_journal_for(testing::factory_verification_journal_contract_fixture()).await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let expected = fixture.input.context().unwrap().subject().candidate().unwrap().clone();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(UnavailableArtifacts),
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          20,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    let report = page
      .items
      .iter()
      .find(|r| r.group == IssueArtifactGroup::Report)
      .unwrap();
    assert_eq!(report.subject.as_ref(), Some(&expected));
  });
}

#[test]
fn issue_artifact_provenance_respects_repository_visibility_before_inventory_membership() {
  run_ready(async {
    let (store, factory, issue, fixture) =
      associated_journal_for(testing::factory_verification_journal_contract_fixture()).await;
    testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
    let policy = Arc::new(ResourcePolicy {
      blocked: ManagementResourceKind::Repository,
    });
    let executions = Arc::new(IssueExecutionHandlers::new(store, factory, policy.clone()));
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueArtifactHandlers::new(
        executions,
        Arc::new(UnavailableArtifacts),
        Arc::new(FixedPanelClock),
        policy,
      )),
    );
    let page = queries
      .handle_query(
        &context(),
        ListIssueArtifactsQuery::try_new(
          GetIssueWorkExecutionQuery {
            project_id: issue.project_id,
            issue_id: issue.id,
            work_id: fixture.admission.work.id(),
          },
          None,
          20,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert!(
      page.items.iter().all(|r| r.group == IssueArtifactGroup::OriginalInput),
      "protected candidate provenance must not appear in inventory or cursors"
    );
  });
}

#[test]
fn issue_candidate_choices_include_capture_and_changeset_publications_without_changing_legacy_reads() {
  run_ready(async {
    for kind in ["candidate", "changeset"] {
      let (store, factory, issue, mut fixture) = associated_journal().await;
      let subject = octacity_server_factory::CandidateSubject::new(
        fixture.admission.work.subject().repository().unwrap().clone(),
        octacity_server_domain::ImmutableRevision::new("original-published-head").unwrap(),
        FactoryDigest::from_bytes([91; 32]),
      );
      fixture.discovery = Some(if kind == "candidate" {
        octacity_server_factory::FlowDiscoveryFacts::candidate(subject.clone())
      } else {
        serde_json::from_value(serde_json::json!({"kind":"changeset", "subject":subject,
          "id":"77777777-7777-4777-8777-777777777777", "node_attempt_id":fixture.attempt.id()}))
        .unwrap()
      });
      testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
      let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let queries = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(IssueExecutionHandlers::new(store.clone(), factory.clone(), policy)),
      );
      let candidates = serde_json::from_value::<FactoryDiagnosticCollection>(serde_json::json!("candidates"))
        .expect("candidate publications must be selectable through the common result reader");
      let execution = GetIssueWorkExecutionQuery {
        project_id: issue.project_id,
        issue_id: issue.id,
        work_id: fixture.admission.work.id(),
      };
      let page = queries
        .handle_query(
          &context(),
          ListIssueResultsQuery::try_new(execution.clone(), candidates, None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(page.coverage, IssueResultCoverage::Partial);
      assert_eq!(
        page.items.len(),
        1,
        "both trusted publication forms are candidate choices"
      );
      let FactoryDiagnosticRecord::DomainFact(publication) = &page.items[0] else {
        panic!("original publication required")
      };
      assert_eq!(publication.facts.published_candidate(), Some(&subject));
      let checks = queries
        .handle_query(
          &context(),
          ListIssueChecksQuery::try_new(execution.clone(), publication.id, None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(checks.subject, subject);
      let legacy = queries
        .handle_query(
          &context(),
          ListIssueResultsQuery::try_new(execution, FactoryDiagnosticCollection::ChangeSets, None, 1).unwrap(),
        )
        .await
        .unwrap();
      assert_eq!(
        legacy.items.len(),
        usize::from(kind == "changeset"),
        "legacy changesets retains its original meaning"
      );
      assert_eq!(factory.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
    }
  });
}
