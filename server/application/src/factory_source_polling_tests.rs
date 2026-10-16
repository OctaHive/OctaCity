use super::*;
use crate::{
  FactoryExecutionClock, FactorySourceCandidateProcessor, FactorySourcePageReader, FactorySourcePoller, SourcePollPage,
  SourcePollRequest, SourcePollingPolicy, WorkSourceSnapshot,
};
use octacity_server_store::{
  ClaimWorkSourceScan, FactorySourceStore, PublishWorkSources, WorkSourceInputChange, WorkSourceScan,
  WorkSourceScanClaim, WorkSourceScanTarget,
};
use std::{
  collections::VecDeque,
  sync::atomic::{AtomicBool, AtomicI64, Ordering},
};

struct Clock(AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(time(self.0.load(Ordering::SeqCst)))
  }
}
#[derive(Default)]
struct Reader {
  fail_next: AtomicBool,
  requests: Mutex<Vec<SourcePollRequest>>,
  pages: Mutex<VecDeque<SourcePollPage>>,
}
#[async_trait]
impl FactorySourcePageReader for Reader {
  async fn read(&self, request: SourcePollRequest) -> Result<SourcePollPage, ApplicationError> {
    self.requests.lock().unwrap().push(request);
    if self.fail_next.swap(false, Ordering::SeqCst) {
      return Err(ApplicationError::unavailable());
    }
    self
      .pages
      .lock()
      .unwrap()
      .pop_front()
      .ok_or_else(ApplicationError::unavailable)
  }
}
#[derive(Default)]
struct Processor {
  fail_item: Mutex<Option<String>>,
  seen: Mutex<Vec<WorkSourceSnapshot>>,
}
#[async_trait]
impl FactorySourceCandidateProcessor for Processor {
  async fn settle(&self, _claim: WorkSourceScanClaim, snapshot: WorkSourceSnapshot) -> Result<(), ApplicationError> {
    let mut fail_item = self.fail_item.lock().unwrap();
    let fail = fail_item.as_deref() == Some(snapshot.item.item.as_str());
    if fail {
      fail_item.take();
    }
    self.seen.lock().unwrap().push(snapshot);
    if fail {
      return Err(ApplicationError::unavailable());
    }
    Ok(())
  }
}
fn policy() -> SourcePollingPolicy {
  SourcePollingPolicy {
    page_items: 2,
    max_pages: 2,
    max_items: 1,
    max_bytes: 65536,
    max_page_bytes: 32768,
    overlap_millis: 20,
    reconciliation_interval_millis: 1000,
    retention_millis: 86400000,
  }
}
async fn fixture(
  provider: &str,
) -> (
  Arc<InMemoryFactoryConfigurationStore>,
  FactorySourceAuthorization,
  WorkSourceScan,
) {
  let project = ProjectId::generate();
  let configuration = FactoryConfigurationId::generate();
  let repository_id = RepositoryId::generate();
  let store = seeded_store(
    project,
    configuration,
    repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
  );
  let authorized = installed(&store, &command(project, configuration, repository_id), provider).await;
  store
    .publish_work_sources(PublishWorkSources {
      project_id: project,
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: authorized.input.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  let scan = store
    .claim_source_scan(ClaimWorkSourceScan {
      project_id: project,
      target: WorkSourceScanTarget::Input {
        id: authorized.input.id(),
        version: authorized.input.version(),
      },
      owner: key("poller"),
      now: time(100),
      until: time(200),
    })
    .await
    .unwrap()
    .unwrap();
  (store, authorized, scan)
}
fn snapshot(authorization: &FactorySourceAuthorization, item: &str, title: &str) -> WorkSourceSnapshot {
  WorkSourceSnapshot {
    selection_matches: vec![],
    item: crate::FactorySourceItem {
      resource: authorization.resource_identity.clone(),
      item: ExternalWorkIdentity::new(item).unwrap(),
    },
    display_id: item.into(),
    update_identity: "same-provider-timestamp".into(),
    title: title.into(),
    body: Some("private complete task".into()),
    fields: Default::default(),
    labels: vec![],
    state: "arbitrary-provider-state".into(),
  }
}

#[test]
fn restart_mid_page_uses_retained_candidates_for_every_provider() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let (store, authorized, scan) = fixture(provider).await;
      let reader = Arc::new(Reader::default());
      let processor = Arc::new(Processor::default());
      let clock = Arc::new(Clock(AtomicI64::new(101)));
      reader.pages.lock().unwrap().push_back(SourcePollPage {
        candidates: vec![
          snapshot(&authorized, "one", "original first"),
          snapshot(&authorized, "two", "original second"),
        ],
        next_cursor: None,
      });
      let poller = FactorySourcePoller::new(
        store.clone(),
        reader.clone(),
        processor.clone(),
        clock.clone(),
        policy(),
      )
      .unwrap();
      let first = poller.run_once(scan.clone()).await.unwrap();
      assert_eq!(first.items_processed, 1);
      assert_eq!(reader.requests.lock().unwrap().len(), 1);
      assert_eq!(processor.seen.lock().unwrap()[0].item.item.as_str(), "one");
      clock.0.store(200, Ordering::SeqCst);
      let takeover = store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: scan.claim.project_id,
          target: scan.claim.target,
          owner: key("replacement-poller"),
          now: time(200),
          until: time(300),
        })
        .await
        .unwrap()
        .unwrap();
      assert!(takeover.claim.fence > scan.claim.fence);
      reader.pages.lock().unwrap().push_back(SourcePollPage {
        candidates: vec![snapshot(&authorized, "changed", "changed provider page")],
        next_cursor: None,
      });
      let restarted = FactorySourcePoller::new(store, reader.clone(), processor.clone(), clock, policy()).unwrap();
      let second = restarted.run_once(takeover).await.unwrap();
      assert_eq!(second.items_processed, 1);
      let seen = processor.seen.lock().unwrap();
      assert_eq!(
        seen.iter().map(|v| v.item.item.as_str()).collect::<Vec<_>>(),
        ["one", "two"]
      );
      assert_eq!(seen[1].title, "original second");
      assert_eq!(
        reader.requests.lock().unwrap().len(),
        1,
        "restart cannot replace an unprocessed snapshot by a mutable provider page"
      );
    }
  });
}

#[test]
fn oversized_pages_are_rejected_before_processing_or_advancing_the_provider_cursor() {
  run_ready(async {
    let (store, authorized, scan) = fixture("github").await;
    let reader = Arc::new(Reader::default());
    let processor = Arc::new(Processor::default());
    reader.pages.lock().unwrap().push_back(SourcePollPage {
      candidates: vec![snapshot(&authorized, "one", "private oversized page")],
      next_cursor: Some("page-two".into()),
    });
    let mut limits = policy();
    limits.max_page_bytes = 128;
    limits.max_bytes = 256;
    let poller = FactorySourcePoller::new(
      store.clone(),
      reader,
      processor.clone(),
      Arc::new(Clock(AtomicI64::new(101))),
      limits,
    )
    .unwrap();
    assert!(poller.run_once(scan.clone()).await.is_err());
    assert!(processor.seen.lock().unwrap().is_empty());
    let next = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target: scan.claim.target,
        owner: key("takeover"),
        now: time(200),
        until: time(300),
      })
      .await
      .unwrap()
      .unwrap();
    assert!(next.progress.unwrap().continuation.is_none());
  });
}

#[test]
fn page_and_byte_budgets_bound_empty_continuations_without_claiming_completion() {
  run_ready(async {
    for (max_pages, max_bytes, expected) in [(1, 1024, 1), (4, 128, 2)] {
      let (store, _, scan) = fixture("jira").await;
      let reader = Arc::new(Reader::default());
      for index in 2..8 {
        reader.pages.lock().unwrap().push_back(SourcePollPage {
          candidates: vec![],
          next_cursor: Some(format!("page-{index}")),
        });
      }
      let mut limits = policy();
      limits.max_pages = max_pages;
      limits.max_items = 10;
      limits.max_bytes = max_bytes;
      limits.max_page_bytes = 64;
      let poller = FactorySourcePoller::new(
        store.clone(),
        reader.clone(),
        Arc::new(Processor::default()),
        Arc::new(Clock(AtomicI64::new(101))),
        limits,
      )
      .unwrap();
      let result = poller.run_once(scan.clone()).await.unwrap();
      assert_eq!(result.pages_read, expected);
      assert_eq!(reader.requests.lock().unwrap().len(), usize::from(expected));
      let next = store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: scan.claim.project_id,
          target: scan.claim.target,
          owner: key("takeover"),
          now: time(200),
          until: time(300),
        })
        .await
        .unwrap()
        .unwrap();
      assert!(
        next.progress.unwrap().continuation.is_some(),
        "a budget stop is not the end of discovery"
      );
    }
  });
}

#[test]
fn polling_policies_reject_zero_or_inconsistent_budgets_before_activity() {
  run_ready(async {
    let (store, _, _) = fixture("github").await;
    for defect in [
      "page-items",
      "pages",
      "items",
      "bytes",
      "page-bytes",
      "inconsistent",
      "period",
    ] {
      let mut limits = policy();
      match defect {
        "page-items" => limits.page_items = 0,
        "pages" => limits.max_pages = 0,
        "items" => limits.max_items = 0,
        "bytes" => limits.max_bytes = 0,
        "page-bytes" => limits.max_page_bytes = 0,
        "inconsistent" => limits.max_bytes = limits.max_page_bytes - 1,
        "period" => limits.reconciliation_interval_millis = 0,
        _ => unreachable!(),
      }
      assert!(
        FactorySourcePoller::new(
          store.clone(),
          Arc::new(Reader::default()),
          Arc::new(Processor::default()),
          Arc::new(Clock(AtomicI64::new(101))),
          limits
        )
        .is_err(),
        "{defect}"
      );
    }
  });
}

#[test]
fn completed_windows_overlap_and_repeat_equal_timestamp_observations_without_deduplicating_in_the_poller() {
  run_ready(async {
    let (store, authorized, scan) = fixture("gitlab").await;
    let reader = Arc::new(Reader::default());
    let processor = Arc::new(Processor::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    reader.pages.lock().unwrap().extend([
      SourcePollPage {
        candidates: vec![snapshot(&authorized, "one", "first")],
        next_cursor: None,
      },
      SourcePollPage {
        candidates: vec![snapshot(&authorized, "one", "edited")],
        next_cursor: None,
      },
    ]);
    let mut limits = policy();
    limits.max_items = 10;
    limits.max_pages = 1;
    let poller =
      FactorySourcePoller::new(store.clone(), reader.clone(), processor.clone(), clock.clone(), limits).unwrap();
    poller.run_once(scan.clone()).await.unwrap();
    clock.0.store(200, Ordering::SeqCst);
    let next = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target: scan.claim.target,
        owner: key("takeover"),
        now: time(200),
        until: time(300),
      })
      .await
      .unwrap()
      .unwrap();
    assert_eq!(poller.run_once(next).await.unwrap().items_processed, 1);
    let requests = reader.requests.lock().unwrap();
    assert_eq!(requests[0].window, Some((time(0), time(101))));
    assert_eq!(requests[1].window, Some((time(81), time(200))));
    assert_ne!(requests[0].scan_id, requests[1].scan_id);
    assert_eq!(
      processor.seen.lock().unwrap().len(),
      2,
      "admission owns replay; polling must not invent another deduplication authority"
    );
  });
}

#[test]
fn reconciliation_and_incremental_pages_share_capacity_fairly_and_keep_each_original_cursor_context() {
  run_ready(async {
    let (store, _, mut scan) = fixture("custom-tracker").await;
    let reader = Arc::new(Reader::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    reader.pages.lock().unwrap().extend([
      SourcePollPage {
        candidates: vec![],
        next_cursor: Some("delta-two".into()),
      },
      SourcePollPage {
        candidates: vec![],
        next_cursor: Some("reconciliation-two".into()),
      },
      SourcePollPage {
        candidates: vec![],
        next_cursor: None,
      },
      SourcePollPage {
        candidates: vec![],
        next_cursor: None,
      },
    ]);
    let mut limits = policy();
    limits.max_pages = 1;
    limits.max_items = 10;
    limits.reconciliation_interval_millis = 50;
    let poller = FactorySourcePoller::new(
      store.clone(),
      reader.clone(),
      Arc::new(Processor::default()),
      clock.clone(),
      limits,
    )
    .unwrap();
    poller.run_once(scan.clone()).await.unwrap();
    for now in [200, 300, 400] {
      clock.0.store(now, Ordering::SeqCst);
      scan = store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: scan.claim.project_id,
          target: scan.claim.target,
          owner: key("takeover"),
          now: time(now),
          until: time(now + 100),
        })
        .await
        .unwrap()
        .unwrap();
      poller.run_once(scan.clone()).await.unwrap();
    }
    let requests = reader.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(
      requests.iter().map(|r| r.window).collect::<Vec<_>>(),
      [Some((time(0), time(101))), None, Some((time(0), time(101))), None]
    );
    assert_eq!(requests[2].continuation.as_deref(), Some("delta-two"));
    assert_eq!(requests[3].continuation.as_deref(), Some("reconciliation-two"));
    assert_eq!(requests[0].scan_id, requests[2].scan_id);
    assert_eq!(requests[1].scan_id, requests[3].scan_id);
    assert_ne!(requests[0].scan_id, requests[1].scan_id);
  });
}

#[test]
fn storage_rejects_unbounded_private_polling_state_and_preserves_legacy_checkpoint_json() {
  run_ready(async {
    use octacity_server_store::{WorkSourcePendingPage, WorkSourcePollingState, WorkSourceProgress};
    let (store, _, scan) = fixture("jira").await;
    let legacy = serde_json::json!({"window_start":0,"window_end":101,"continuation":null,"reconciliation":false});
    let restored: WorkSourceProgress = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), legacy);
    let progress = WorkSourceProgress {
      window_start: time(0),
      window_end: time(101),
      continuation: None,
      reconciliation: false,
      polling: Some(WorkSourcePollingState {
        scan_id: FactoryDigest::content_sha256(b"stable pass"),
        finished: false,
        pending_page: Some(WorkSourcePendingPage {
          candidates: vec![serde_json::json!({"private":"snapshot"}); 101],
          next_cursor: None,
          expires_at: time(1000),
        }),
        other_pass: None,
        reconciliation_due_at: time(1101),
        switch_after_page: false,
      }),
    };
    for defect in [
      "items",
      "snapshot-bytes",
      "page-bytes",
      "cursor",
      "finished",
      "other-lane",
    ] {
      let mut invalid = progress.clone();
      let state = invalid.polling.as_mut().unwrap();
      state.pending_page.as_mut().unwrap().candidates = vec![serde_json::json!({"private":"snapshot"})];
      match defect {
        "items" => state.pending_page.as_mut().unwrap().candidates = vec![serde_json::json!({}); 101],
        "snapshot-bytes" => {
          state.pending_page.as_mut().unwrap().candidates = vec![serde_json::json!("x".repeat(1_048_577))]
        }
        "page-bytes" => {
          state.pending_page.as_mut().unwrap().candidates = vec![serde_json::json!("x".repeat(900_000)); 5]
        }
        "cursor" => state.pending_page.as_mut().unwrap().next_cursor = Some("x".repeat(4097)),
        "finished" => state.finished = true,
        "other-lane" => {
          state.other_pass = Some(octacity_server_store::WorkSourcePollPass {
            scan_id: state.scan_id,
            window_start: time(0),
            window_end: time(101),
            continuation: None,
            reconciliation: false,
            finished: false,
          })
        }
        _ => unreachable!(),
      }
      assert!(
        store
          .checkpoint_source_scan(scan.claim.clone(), time(101), None, invalid)
          .await
          .is_err(),
        "{defect}"
      );
    }
  });
}

#[test]
fn expired_owner_cannot_process_a_retained_candidate_before_its_checkpoint_is_rejected() {
  run_ready(async {
    let (store, authorized, scan) = fixture("jira").await;
    let reader = Arc::new(Reader::default());
    let processor = Arc::new(Processor::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    reader.pages.lock().unwrap().push_back(SourcePollPage {
      candidates: vec![
        snapshot(&authorized, "one", "first"),
        snapshot(&authorized, "two", "second"),
      ],
      next_cursor: None,
    });
    let poller = FactorySourcePoller::new(store.clone(), reader, processor.clone(), clock.clone(), policy()).unwrap();
    poller.run_once(scan.clone()).await.unwrap();
    let next = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target: scan.claim.target,
        owner: key("takeover"),
        now: time(200),
        until: time(300),
      })
      .await
      .unwrap()
      .unwrap();
    clock.0.store(300, Ordering::SeqCst);
    assert!(poller.run_once(next).await.is_err());
    assert_eq!(
      processor.seen.lock().unwrap().len(),
      1,
      "the expired lease must fail before invoking admission for remaining private data"
    );
  });
}

#[test]
fn partial_processing_failure_retries_the_original_candidate_and_never_skips_the_tail() {
  run_ready(async {
    let (store, authorized, scan) = fixture("github").await;
    let reader = Arc::new(Reader::default());
    let processor = Arc::new(Processor::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    *processor.fail_item.lock().unwrap() = Some("two".into());
    reader.pages.lock().unwrap().push_back(SourcePollPage {
      candidates: vec![
        snapshot(&authorized, "one", "original first"),
        snapshot(&authorized, "two", "original second"),
        snapshot(&authorized, "three", "original third"),
      ],
      next_cursor: None,
    });
    let mut limits = policy();
    limits.page_items = 3;
    limits.max_items = 2;
    let poller =
      FactorySourcePoller::new(store.clone(), reader.clone(), processor.clone(), clock.clone(), limits).unwrap();
    assert!(poller.run_once(scan.clone()).await.is_err());
    clock.0.store(200, Ordering::SeqCst);
    let next = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target: scan.claim.target,
        owner: key("takeover"),
        now: time(200),
        until: time(300),
      })
      .await
      .unwrap()
      .unwrap();
    assert_eq!(poller.run_once(next).await.unwrap().items_processed, 2);
    let seen = processor.seen.lock().unwrap();
    assert_eq!(
      seen.iter().map(|v| v.item.item.as_str()).collect::<Vec<_>>(),
      ["one", "two", "two", "three"]
    );
    assert_eq!(seen[2].title, "original second");
    assert_eq!(reader.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn provider_failure_keeps_the_same_frozen_window_and_continuation_for_retry() {
  run_ready(async {
    let (store, _, mut scan) = fixture("gitlab").await;
    let reader = Arc::new(Reader::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    reader.pages.lock().unwrap().extend([
      SourcePollPage {
        candidates: vec![],
        next_cursor: Some("private-next".into()),
      },
      SourcePollPage {
        candidates: vec![],
        next_cursor: None,
      },
    ]);
    let mut limits = policy();
    limits.max_pages = 1;
    let poller = FactorySourcePoller::new(
      store.clone(),
      reader.clone(),
      Arc::new(Processor::default()),
      clock.clone(),
      limits,
    )
    .unwrap();
    poller.run_once(scan.clone()).await.unwrap();
    for now in [200, 300] {
      clock.0.store(now, Ordering::SeqCst);
      scan = store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: scan.claim.project_id,
          target: scan.claim.target,
          owner: key("takeover"),
          now: time(now),
          until: time(now + 100),
        })
        .await
        .unwrap()
        .unwrap();
      if now == 200 {
        reader.fail_next.store(true, Ordering::SeqCst);
        assert!(poller.run_once(scan.clone()).await.is_err());
      } else {
        assert_eq!(poller.run_once(scan.clone()).await.unwrap().pages_read, 1);
      }
    }
    let requests = reader.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].continuation.as_deref(), Some("private-next"));
    assert_eq!(requests[1].continuation, requests[2].continuation);
    assert_eq!(requests[0].scan_id, requests[2].scan_id);
    assert_eq!(requests[0].window, requests[2].window);
  });
}

#[derive(Default)]
struct PublishedBytes(Mutex<Vec<octacity_artifact_store::ArtifactObject>>);
#[async_trait]
impl octacity_artifact_store::ArtifactBytePublisher for PublishedBytes {
  async fn publish_verified(
    &self,
    object: &octacity_artifact_store::ArtifactObject,
    _bytes: Vec<u8>,
  ) -> Result<(), octacity_artifact_store::ArtifactStoreError> {
    self.0.lock().unwrap().push(object.clone());
    Ok(())
  }
}
struct AdmissionProcessor {
  store: Arc<InMemoryFactoryConfigurationStore>,
  authorization: FactorySourceAuthorization,
  clock: Arc<Clock>,
  preparer: crate::FactoryWorkSourcePreparer<InMemoryFactoryConfigurationStore, PublishedBytes>,
  admission: FactorySourceAdmissionHandlers<InMemoryFactoryConfigurationStore>,
  receipts: Mutex<Vec<octacity_server_factory::FactoryRunId>>,
  lose_reply: AtomicBool,
}
#[async_trait]
impl FactorySourceCandidateProcessor for AdmissionProcessor {
  async fn settle(&self, claim: WorkSourceScanClaim, snapshot: WorkSourceSnapshot) -> Result<(), ApplicationError> {
    let input = &self.authorization.input;
    let prior = self
      .admission
      .recover(crate::RecoverSourceFactoryWorkCommand {
        input_id: input.id(),
        input_version: input.version(),
        item: snapshot.item.clone(),
      })
      .await?;
    let run = if let Some(prior) = prior {
      prior.run.id()
    } else {
      let preparation = self
        .store
        .reserve_source_preparation(octacity_server_store::ReserveWorkSourcePreparation {
          claim: claim.clone(),
          input_id: input.id(),
          item: snapshot.item.clone(),
          snapshot: serde_json::to_vec(&snapshot).unwrap(),
          now: self.clock.now()?,
        })
        .await?;
      let ordinary = command(
        input.project_id(),
        input.definition().target.id(),
        input.definition().repository.id,
      );
      self
        .preparer
        .prepare(
          claim.clone(),
          preparation.id,
          crate::SourcePreparationSettings {
            mapping: crate::WorkSourceMapping {
              source: input.definition().source.clone(),
              acceptance: crate::SourceAcceptanceMapping::Body,
              require_body: true,
              max_title_bytes: 1024,
              max_body_bytes: 65536,
              max_artifact_bytes: 131072,
              retention_millis: 86400000,
            },
            source: ordinary.source,
            classification: ordinary.classification,
          },
        )
        .await?;
      self
        .admission
        .admit_prepared(crate::AdmitPreparedSourceFactoryWorkCommand {
          input_id: input.id(),
          input_version: input.version(),
          guard: octacity_server_store::AdmitPreparedWorkSource {
            claim,
            preparation_id: preparation.id,
            now: self.clock.now()?,
          },
        })
        .await?
        .factory_run_id
    };
    self.receipts.lock().unwrap().push(run);
    if self.lose_reply.swap(false, Ordering::SeqCst) {
      return Err(ApplicationError::unavailable());
    }
    Ok(())
  }
}

#[test]
fn uncertain_admission_and_repeated_observations_reuse_the_original_run_without_another_vcs_or_artifact_publication() {
  run_ready(async {
    let (store, authorized, mut scan) = fixture("jira").await;
    let reader = Arc::new(Reader::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    let bytes = Arc::new(PublishedBytes::default());
    let resolver = Arc::new(RecordingResolver::default());
    let processor = Arc::new(AdmissionProcessor {
      store: store.clone(),
      authorization: authorized.clone(),
      clock: clock.clone(),
      preparer: crate::FactoryWorkSourcePreparer::new(store.clone(), bytes.clone(), clock.clone()),
      admission: FactorySourceAdmissionHandlers::new(
        store.clone(),
        resolver.clone(),
        Arc::new(crate::TrustedNetworkManagementPolicy),
        Arc::new(InstalledSource(authorized.clone())),
      )
      .with_clock(clock.clone()),
      receipts: Mutex::new(vec![]),
      lose_reply: AtomicBool::new(true),
    });
    reader.pages.lock().unwrap().extend([
      SourcePollPage {
        candidates: vec![snapshot(&authorized, "one", "original task")],
        next_cursor: None,
      },
      SourcePollPage {
        candidates: vec![snapshot(&authorized, "one", "edited task")],
        next_cursor: None,
      },
    ]);
    let poller = FactorySourcePoller::new(
      store.clone(),
      reader.clone(),
      processor.clone(),
      clock.clone(),
      policy(),
    )
    .unwrap();
    assert!(poller.run_once(scan.clone()).await.is_err());
    for now in [200, 300] {
      clock.0.store(now, Ordering::SeqCst);
      scan = store
        .claim_source_scan(ClaimWorkSourceScan {
          project_id: scan.claim.project_id,
          target: scan.claim.target,
          owner: key("takeover"),
          now: time(now),
          until: time(now + 100),
        })
        .await
        .unwrap()
        .unwrap();
      assert_eq!(poller.run_once(scan.clone()).await.unwrap().items_processed, 1);
    }
    let receipts = processor.receipts.lock().unwrap();
    assert_eq!(receipts.len(), 3);
    assert!(receipts.iter().all(|run| *run == receipts[0]));
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(
      bytes.0.lock().unwrap().len(),
      2,
      "only original task and acceptance documents are published"
    );
    assert_eq!(
      reader.requests.lock().unwrap().len(),
      2,
      "lost admission reply retries the retained page before another provider read"
    );
  });
}

#[test]
fn retained_pages_expire_without_renewal_and_cleanup_forces_a_safe_restart() {
  run_ready(async {
    let (store, authorized, scan) = fixture("github").await;
    let reader = Arc::new(Reader::default());
    let processor = Arc::new(Processor::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    reader.pages.lock().unwrap().push_back(SourcePollPage {
      candidates: vec![
        snapshot(&authorized, "one", "first"),
        snapshot(&authorized, "two", "private pending"),
      ],
      next_cursor: Some("private-next-page".into()),
    });
    let mut limits = policy();
    limits.retention_millis = 10;
    let poller = FactorySourcePoller::new(store.clone(), reader, processor, clock, limits).unwrap();
    poller.run_once(scan.clone()).await.unwrap();
    assert_eq!(store.expire_source_poll_pages(time(110), 1).await.unwrap(), 0);
    assert_eq!(store.expire_source_poll_pages(time(111), 1).await.unwrap(), 1);
    assert_eq!(store.expire_source_poll_pages(time(111), 1).await.unwrap(), 0);
    let next = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target: scan.claim.target,
        owner: key("takeover"),
        now: time(200),
        until: time(300),
      })
      .await
      .unwrap()
      .unwrap();
    assert!(
      next.progress.is_none(),
      "expired private pages cannot leave a covered cursor that skips their unprocessed data"
    );
  });
}

#[test]
fn legacy_or_expired_private_positions_restart_from_epoch_without_reusing_unknown_cursors() {
  run_ready(async {
    for legacy in [true, false] {
      let (store, authorized, mut scan) = fixture("gitlab").await;
      let reader = Arc::new(Reader::default());
      let processor = Arc::new(Processor::default());
      let clock = Arc::new(Clock(AtomicI64::new(101)));
      let mut limits = policy();
      limits.retention_millis = 10;
      let poller =
        FactorySourcePoller::new(store.clone(), reader.clone(), processor.clone(), clock.clone(), limits).unwrap();
      if legacy {
        let prior = octacity_server_store::WorkSourceProgress {
          window_start: time(80),
          window_end: time(100),
          continuation: Some("unknown-private-legacy-cursor".into()),
          reconciliation: false,
          polling: None,
        };
        store
          .checkpoint_source_scan(scan.claim.clone(), time(101), None, prior.clone())
          .await
          .unwrap();
        scan.progress = Some(prior);
      } else {
        reader.pages.lock().unwrap().push_back(SourcePollPage {
          candidates: vec![
            snapshot(&authorized, "old-first", "old first"),
            snapshot(&authorized, "old-second", "expired private tail"),
          ],
          next_cursor: Some("expired-private-cursor".into()),
        });
        poller.run_once(scan.clone()).await.unwrap();
        clock.0.store(200, Ordering::SeqCst);
        scan = store
          .claim_source_scan(ClaimWorkSourceScan {
            project_id: scan.claim.project_id,
            target: scan.claim.target,
            owner: key("takeover"),
            now: time(200),
            until: time(300),
          })
          .await
          .unwrap()
          .unwrap();
      }
      reader.pages.lock().unwrap().push_back(SourcePollPage {
        candidates: vec![snapshot(&authorized, "fresh-first", "fresh first")],
        next_cursor: None,
      });
      assert_eq!(poller.run_once(scan).await.unwrap().items_processed, 1);
      let requests = reader.requests.lock().unwrap();
      let last = requests.last().unwrap();
      assert_eq!(last.continuation, None, "legacy={legacy}");
      assert_eq!(last.window, Some((time(0), if legacy { time(101) } else { time(200) })));
      assert_eq!(processor.seen.lock().unwrap().last().unwrap().title, "fresh first");
    }
  });
}

#[test]
fn provider_cannot_enlarge_the_installed_item_ceiling_even_when_its_page_fits_the_byte_budget() {
  run_ready(async {
    let (store, authorized, scan) = fixture("custom-tracker").await;
    let reader = Arc::new(Reader::default());
    let processor = Arc::new(Processor::default());
    reader.pages.lock().unwrap().push_back(SourcePollPage {
      candidates: vec![
        snapshot(&authorized, "one", "first"),
        snapshot(&authorized, "two", "second"),
        snapshot(&authorized, "three", "third"),
      ],
      next_cursor: None,
    });
    let poller = FactorySourcePoller::new(
      store.clone(),
      reader,
      processor.clone(),
      Arc::new(Clock(AtomicI64::new(101))),
      policy(),
    )
    .unwrap();
    assert!(poller.run_once(scan).await.is_err());
    assert!(processor.seen.lock().unwrap().is_empty());
  });
}

#[test]
fn private_page_expiry_during_processing_stops_before_the_next_candidate() {
  struct AdvancingProcessor {
    clock: Arc<Clock>,
    seen: Mutex<Vec<String>>,
  }
  #[async_trait]
  impl FactorySourceCandidateProcessor for AdvancingProcessor {
    async fn settle(&self, _: WorkSourceScanClaim, snapshot: WorkSourceSnapshot) -> Result<(), ApplicationError> {
      self.seen.lock().unwrap().push(snapshot.title);
      self.clock.0.store(111, Ordering::SeqCst);
      Ok(())
    }
  }
  run_ready(async {
    let (store, authorized, scan) = fixture("jira").await;
    let reader = Arc::new(Reader::default());
    let clock = Arc::new(Clock(AtomicI64::new(101)));
    let processor = Arc::new(AdvancingProcessor {
      clock: clock.clone(),
      seen: Mutex::new(vec![]),
    });
    reader.pages.lock().unwrap().push_back(SourcePollPage {
      candidates: vec![
        snapshot(&authorized, "one", "first"),
        snapshot(&authorized, "two", "expired tail"),
      ],
      next_cursor: None,
    });
    let mut limits = policy();
    limits.max_items = 2;
    limits.retention_millis = 10;
    let poller = FactorySourcePoller::new(store, reader, processor.clone(), clock, limits).unwrap();
    assert!(poller.run_once(scan).await.is_err());
    assert_eq!(*processor.seen.lock().unwrap(), ["first"]);
  });
}

#[test]
fn polling_request_diagnostics_redact_private_provider_continuations() {
  run_ready(async {
    let (_, _, scan) = fixture("jira").await;
    let request = SourcePollRequest {
      claim: scan.claim,
      scan_id: FactoryDigest::content_sha256(b"scan"),
      window: None,
      continuation: Some("private-provider-cursor".into()),
      max_items: 2,
      max_page_bytes: 32768,
    };
    assert!(!format!("{request:?}").contains("private-provider-cursor"));
  });
}

#[test]
fn source_discovery_pages_only_current_enabled_targets_and_never_schedules_exclusive_members_individually() {
  run_ready(async {
    use octacity_server_factory::{WorkSourceGroup, WorkSourceGroupId};
    let (store, authorized, scan) = fixture("jira").await;
    let project = scan.claim.project_id;
    assert_eq!(
      store.source_scan_targets(project, None, 1).await.unwrap(),
      vec![scan.claim.target]
    );
    assert!(
      store
        .source_scan_targets(ProjectId::generate(), None, 1)
        .await
        .unwrap()
        .is_empty()
    );
    let group =
      WorkSourceGroup::publish(WorkSourceGroupId::generate(), project, vec![authorized.input.clone()]).unwrap();
    store
      .publish_work_sources(PublishWorkSources {
        project_id: project,
        inputs: vec![],
        group: Some(octacity_server_store::WorkSourceGroupChange {
          expected_version: None,
          group: group.clone(),
        }),
      })
      .await
      .unwrap();
    let target = WorkSourceScanTarget::Group {
      id: group.id(),
      version: group.version(),
    };
    assert_eq!(store.source_scan_targets(project, None, 1).await.unwrap(), vec![target]);
    assert!(
      store
        .source_scan_targets(project, Some(target), 1)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(store.source_scan_targets(project, None, 0).await.is_err());
    assert!(store.source_scan_targets(project, None, 65).await.is_err());
    let mut value = serde_json::to_value(&authorized.input).unwrap();
    value["version"] = serde_json::json!(2);
    value["definition"]["enabled"] = serde_json::json!(false);
    let disabled = serde_json::from_value(value).unwrap();
    store
      .publish_work_sources(PublishWorkSources {
        project_id: project,
        inputs: vec![WorkSourceInputChange {
          expected_version: Some(authorized.input.version()),
          input: disabled,
        }],
        group: None,
      })
      .await
      .unwrap();
    assert!(
      store.source_scan_targets(project, None, 64).await.unwrap().is_empty(),
      "an obsolete group cannot fall back to independent admission"
    );
  });
}

#[test]
fn source_retry_time_failure_and_exhaustion_survive_takeover_and_reset_only_for_a_new_definition() {
  run_ready(async {
    use octacity_server_store::{WorkSourceFailure, WorkSourceSchedule};
    let (store, authorized, scan) = fixture("gitlab").await;
    let request = |now| ClaimWorkSourceScan {
      project_id: scan.claim.project_id,
      target: scan.claim.target,
      owner: key("retry-owner"),
      now: time(now),
      until: time(now + 100),
    };
    let schedule = WorkSourceSchedule {
      retry_at: time(1000),
      attempts: 1,
      failure: Some(WorkSourceFailure::RateLimited),
      exhausted: false,
    };
    store
      .finish_source_scan(scan.claim.clone(), time(101), schedule.clone())
      .await
      .unwrap();
    assert_eq!(
      store
        .source_scan_schedule(scan.claim.project_id, scan.claim.target)
        .await
        .unwrap(),
      Some(schedule.clone())
    );
    assert!(store.claim_source_scan(request(200)).await.unwrap().is_none());
    assert!(store.claim_source_scan(request(999)).await.unwrap().is_none());
    let next = store.claim_source_scan(request(1000)).await.unwrap().unwrap();
    assert_eq!(next.schedule, Some(schedule.clone()));
    assert!(
      store
        .finish_source_scan(scan.claim.clone(), time(1001), schedule.clone())
        .await
        .is_err()
    );
    store
      .finish_source_scan(
        next.claim,
        time(1001),
        WorkSourceSchedule {
          exhausted: true,
          attempts: 3,
          ..schedule
        },
      )
      .await
      .unwrap();
    assert!(store.claim_source_scan(request(2000)).await.unwrap().is_none());
    let mut value = serde_json::to_value(&authorized.input).unwrap();
    value["version"] = serde_json::json!(2);
    let changed: octacity_server_factory::WorkSourceInput = serde_json::from_value(value).unwrap();
    store
      .publish_work_sources(PublishWorkSources {
        project_id: changed.project_id(),
        inputs: vec![WorkSourceInputChange {
          expected_version: Some(authorized.input.version()),
          input: changed.clone(),
        }],
        group: None,
      })
      .await
      .unwrap();
    let fresh = store
      .claim_source_scan(ClaimWorkSourceScan {
        target: WorkSourceScanTarget::Input {
          id: changed.id(),
          version: changed.version(),
        },
        ..request(2000)
      })
      .await
      .unwrap()
      .unwrap();
    assert!(fresh.schedule.is_none());
    assert!(fresh.claim.fence > scan.claim.fence);
  });
}

#[test]
fn group_selection_is_fenced_durable_and_ambiguous_observations_choose_no_target() {
  run_ready(async {
    use octacity_server_factory::{WorkSourceGroup, WorkSourceGroupId, WorkSourceGroupVersion, WorkSourceSelection};
    let (store, authorized, scan) = fixture("jira").await;
    let mut second = serde_json::to_value(&authorized.input).unwrap();
    second["id"] = serde_json::to_value(WorkSourceInputId::generate()).unwrap();
    let second: octacity_server_factory::WorkSourceInput = serde_json::from_value(second).unwrap();
    let group = WorkSourceGroup::publish(
      WorkSourceGroupId::generate(),
      scan.claim.project_id,
      vec![authorized.input.clone(), second.clone()],
    )
    .unwrap();
    store
      .publish_work_sources(PublishWorkSources {
        project_id: scan.claim.project_id,
        inputs: vec![WorkSourceInputChange {
          expected_version: None,
          input: second.clone(),
        }],
        group: Some(octacity_server_store::WorkSourceGroupChange {
          expected_version: None,
          group: group.clone(),
        }),
      })
      .await
      .unwrap();
    let target = WorkSourceScanTarget::Group {
      id: group.id(),
      version: group.version(),
    };
    let claimed = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target,
        owner: key("group-owner"),
        now: time(100),
        until: time(200),
      })
      .await
      .unwrap()
      .unwrap();
    let item = snapshot(&authorized, "one", "private title").item;
    assert_eq!(
      store
        .select_source_input(claimed.claim.clone(), time(101), item.clone(), vec![])
        .await
        .unwrap(),
      WorkSourceSelection::Filtered
    );
    assert!(
      matches!(store.select_source_input(claimed.claim.clone(),time(102),item.clone(),vec![second.id()]).await.unwrap(),WorkSourceSelection::Selected(selected) if selected.id()==second.id())
    );
    let matches = vec![authorized.input.id(), second.id()];
    assert!(
      matches!(store.select_source_input(claimed.claim.clone(),time(103),item.clone(),matches.clone()).await.unwrap(),WorkSourceSelection::Ambiguous(ids) if ids.len()==2)
    );
    assert!(
      store
        .select_source_input(
          claimed.claim.clone(),
          time(104),
          item.clone(),
          vec![WorkSourceInputId::generate()]
        )
        .await
        .is_err()
    );
    let resumed = store
      .claim_source_scan(ClaimWorkSourceScan {
        project_id: scan.claim.project_id,
        target,
        owner: key("group-takeover"),
        now: time(200),
        until: time(300),
      })
      .await
      .unwrap()
      .unwrap();
    let record = resumed.last_selection.unwrap();
    assert_eq!(record.item, item);
    assert_eq!(record.matching_inputs, matches);
    let changed = group
      .replace(WorkSourceGroupVersion::new(2).unwrap(), group.members().to_vec())
      .unwrap();
    store
      .publish_work_sources(PublishWorkSources {
        project_id: scan.claim.project_id,
        inputs: vec![],
        group: Some(octacity_server_store::WorkSourceGroupChange {
          expected_version: Some(group.version()),
          group: changed,
        }),
      })
      .await
      .unwrap();
    assert!(
      store
        .select_source_input(resumed.claim, time(201), item, vec![second.id()])
        .await
        .is_err()
    );
  });
}
