use super::*;
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
    Ok(at(50))
  }
}
struct Reader;
#[async_trait]
impl IssueCodeReviewObservations for Reader {
  async fn visibility(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
  ) -> Result<octacity_server_store::IssueCodeReviewRepositoryVisibility, ApplicationError> {
    Ok(octacity_server_store::IssueCodeReviewRepositoryVisibility::all())
  }

  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    _: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn authorize_candidate(
    &self,
    _: &ManagementRequestContext,
    _: &IssueCodeReviewSelection,
    _: &CandidateSubject,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn authorize_publication(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    selection: &IssueCodeReviewSelection,
    target: &ImmutableReference,
  ) -> Result<(), ApplicationError> {
    // Fixture's installed policy maps only this destination; both mirrors remain readable.
    if target == &reference("review_branch")
      && selection.source.installation.identity().as_str() == "code-forge"
      && selection.item.resource.as_str() == "repository-1"
    {
      Ok(())
    } else {
      Err(ApplicationError::capability_unavailable())
    }
  }
  async fn read(
    &self,
    _: &ManagementRequestContext,
    _: octacity_server_domain::ProjectId,
    selection: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    Ok(IssueCodeReviewObservation {
      selection: selection.clone(),
      observed_at: at(30),
      title: None,
      number: None,
      url: None,
      author: None,
      state: Some(IssueCodeReviewState::Open),
      draft: None,
      head: Some("reviewed-candidate".parse().unwrap()),
      base: None,
      changed_files: None,
      additions: None,
      deletions: None,
      review: None,
      checks: None,
      check_details: None,
    })
  }
}
#[test]
fn issue_code_review_delivery_uses_the_retained_publication_receipt_without_republishing_or_retargeting() {
  crate::factory_admission_tests::run_ready(async {
    for publication in [true, false] {
      let mut h = harness_with_result(
        binding(
          "publisher",
          if publication { "publish_candidate" } else { "notify" },
          serde_json::json!({"bundle":"candidate_bytes", "manifest":"candidate_manifest", "target":reference("review_branch")}),
        ),
        permissions("publisher"),
        publication_schema(),
      );
      let candidate = attach_candidate(&mut h);
      let publisher = Arc::new(Publisher {
        requests: Mutex::new(vec![]),
        published: Mutex::new(None),
        lose_response: std::sync::atomic::AtomicBool::new(false),
      });
      let plugin: Arc<dyn FactoryFlowActionPlugin> = if publication {
        Arc::new(FactoryCandidatePublicationAction::new(
          reference("publisher"),
          publisher.clone(),
          h.outcome.clone(),
        ))
      } else {
        Arc::new(ReceiptShapedNotification {
          candidate: candidate.clone(),
          outcome: h.outcome.clone(),
        })
      };
      let adapter = Arc::new(FactoryFlowActionAdapter::new(plugin));
      let runner = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
      runner
        .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
        .await
        .unwrap();
      let FactoryNodeStep::Completed(record) = runner
        .run_once(h.run, h.claim, h.target.clone(), None, at(12))
        .await
        .unwrap()
      else {
        panic!("publication completion")
      };
      let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
      let project = snapshot.work.subject().project_id();
      let store = crate::issue_test_support::manual_issue_store(project).await;
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
      let issue = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(IssueHandlers::new(store.clone(), policy.clone())),
      )
      .handle_command(
        &context,
        CreateIssueCommand {
          intent: CreateIssueIntent {
            project_id: project,
            content: IssueContent {
              title: "Manual review task".into(),
              description: String::new(),
              acceptance: None,
              priority: None,
            },
            planning: Default::default(),
          },
          idempotency_key: "manual-review".parse().unwrap(),
          created_at: at(1),
        },
      )
      .await
      .unwrap()
      .issue;
      let associated = store
        .associate_issue_work(store_testing::management_mutation(AssociateIssueWork {
          project_id: project,
          issue_id: issue.id,
          request_id: None,
          admission: PublishedFactoryAdmission {
            work: snapshot.work.clone(),
            run: FactoryRun::admitted(h.run, &snapshot.work),
            flow: snapshot.admitted_flow.clone(),
            admitted_at: at(1),
          },
        }))
        .await
        .unwrap()
        .association;
      let executions = Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        h.store.clone(),
        policy.clone(),
      ));
      let commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(
          IssueCodeReviewHandlers::new(store.clone(), Arc::new(Reader), policy, Arc::new(Clock))
            .with_execution_reader(executions),
        ),
      );
      let binding = IssueCodeReviewBinding {
        association: associated,
        record: record.digest().unwrap(),
        candidate,
      };
      let selection = IssueCodeReviewSelection {
        source: WorkSourceBinding {
          installation: reference("code-forge"),
          capability: reference("review-read"),
          configuration: reference("review-config"),
          resource: reference("native-code"),
        },
        item: WorkSourceItem {
          resource: "repository-1".parse().unwrap(),
          item: "23".parse().unwrap(),
        },
      };
      let command = AttachIssueCodeReviewDeliveryCommand {
        attachment: AttachIssueCodeReviewCommand {
          intent: AttachIssueCodeReviewIntent {
            project_id: project,
            issue_id: issue.id,
            expected_version: issue.version,
            selection,
            binding: Some(binding.clone()),
          },
          idempotency_key: "delivery-association".parse().unwrap(),
        },
      };
      let result = commands.handle_command(&context, command.clone()).await;
      if !publication {
        assert!(
          result.is_err(),
          "another declared action's receipt-shaped output is not Delivery provenance"
        );
        continue;
      }
      let outcome = result.unwrap();
      assert_eq!(outcome.association.binding, Some(binding));
      assert_eq!(
        outcome.association.provenance,
        IssueCodeReviewProvenance::Delivery {
          receipt: FactoryDigest::from_bytes([88; 32])
        }
      );
      // Both mirrors are independently readable and can contain the same head/review ID.
      // Only the original publication destination can establish Delivery provenance.
      for other_installation in [true, false] {
        let mut foreign = command.clone();
        foreign.attachment.idempotency_key = if other_installation {
          "delivery-other-installation"
        } else {
          "delivery-other-repository"
        }
        .parse()
        .unwrap();
        if other_installation {
          foreign.attachment.intent.selection.source.installation = reference("other-code-forge");
        } else {
          foreign.attachment.intent.selection.item.resource = "repository-2".parse().unwrap();
        }
        assert!(
          commands.handle_command(&context, foreign).await.is_err(),
          "a publication receipt cannot prove Delivery to another installed native repository"
        );
      }
      assert_eq!(
        commands.handle_command(&context, command).await.unwrap().association,
        outcome.association
      );
      assert_eq!(h.store.factory_run_snapshot(h.run).await.unwrap(), snapshot);
      assert_eq!(publisher.requests.lock().unwrap().len(), 1);
      assert_eq!(store.issue(project, issue.id, None).await.unwrap(), issue);
    }
  });
}

struct ReceiptShapedNotification {
  candidate: CandidateSubject,
  outcome: FlowOutcomeDefinition,
}
#[async_trait]
impl FactoryFlowActionPlugin for ReceiptShapedNotification {
  fn identity(&self) -> ImmutableReference {
    reference("publisher")
  }
  fn validate(&self, _: &FlowActionBinding, _: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    Ok(Some(BudgetUsage::default()))
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference() == self.outcome.schema())
      .unwrap();
    Ok(FactoryFlowActionObservation {discovery:Some(FlowDiscoveryFacts::candidate(self.candidate.clone())),outcome:self.outcome.key().clone(),payload:FlowPayload::new(schema,serde_json::json!({"receipt":FactoryDigest::from_bytes([88;32]),"candidate_revision":self.candidate.candidate_revision(),"changeset_digest":self.candidate.changeset_digest(),"external":{"branch_reference":"refs/heads/reviewed-candidate","remote_head":self.candidate.candidate_revision(),"review_identity":"23","review_state":"open"}})).unwrap(),usage:BudgetUsage::default(),outbox:vec![]})
  }
}
