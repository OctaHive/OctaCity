use super::*;
use octacity_server_factory::{CandidateSubject, FlowDiscoveryFacts};
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(50))
  }
}
struct Reader {
  repository: octacity_server_domain::RepositoryId,
}
#[async_trait::async_trait]
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
    _: ProjectId,
    _: &IssueCodeReviewSelection,
  ) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn authorize_candidate(
    &self,
    _: &ManagementRequestContext,
    _: &IssueCodeReviewSelection,
    candidate: &CandidateSubject,
  ) -> Result<(), ApplicationError> {
    if candidate.exact().repository_id() == self.repository {
      Ok(())
    } else {
      Err(ApplicationError::capability_unavailable())
    }
  }
  async fn read(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    selected: &IssueCodeReviewSelection,
  ) -> Result<IssueCodeReviewObservation, ApplicationError> {
    Ok(IssueCodeReviewObservation {
      selection: selected.clone(),
      observed_at: at(20),
      title: None,
      number: None,
      url: None,
      author: None,
      state: Some(IssueCodeReviewState::Merged),
      draft: None,
      head: Some("verified-head".parse().unwrap()),
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
fn issue_code_review_binding_requires_visible_original_association_record_and_candidate_without_advancing_factory() {
  run_ready(async {
    for independent_resource in [false, true] {
      let (store, factory, issue, mut fixture) = if independent_resource {
        associated_journal_for(independent_resource_journal_fixture()).await
      } else {
        associated_journal().await
      };
      let candidate = CandidateSubject::new(
        fixture.input.exact_resource().unwrap().clone(),
        "verified-head".parse().unwrap(),
        FactoryDigest::from_bytes([81; 32]),
      );
      fixture.discovery = Some(FlowDiscoveryFacts::candidate(candidate.clone()));
      testing::verify_factory_node_journal_contract(&*factory, &*factory, &fixture).await;
      let snapshot = factory.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      let association = store
        .issue_work_association(issue.project_id, fixture.admission.work.id())
        .await
        .unwrap()
        .unwrap();
      let binding = IssueCodeReviewBinding {
        association,
        record: snapshot.flow.data.records[0].digest().unwrap(),
        candidate,
      };
      let policy = Arc::new(TrustedNetworkManagementPolicy);
      let executions = Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        policy.clone(),
      ));
      let handlers = Arc::new(
        IssueCodeReviewHandlers::new(
          store.clone(),
          Arc::new(Reader {
            repository: fixture.input.exact_resource().unwrap().repository_id(),
          }),
          policy.clone(),
          Arc::new(Clock),
        )
        .with_execution_reader(executions),
      );
      let commands = AuthorizedCommandHandler::new(policy.clone(), handlers.clone());
      let command = AttachIssueCodeReviewCommand {intent:AttachIssueCodeReviewIntent {project_id:issue.project_id, issue_id:issue.id, expected_version:issue.version, selection:serde_json::from_value(serde_json::json!({"source":{"installation":{"identity":"forge","version":"v1","digest":"01".repeat(32)},"capability":{"identity":"read-reviews","version":"v1","digest":"02".repeat(32)},"configuration":{"identity":"code-config","version":"v1","digest":"03".repeat(32)},"resource":{"identity":"native-repo","version":"v1","digest":"04".repeat(32)}},"item":{"resource":"code-repository","item":"23"}})).unwrap(), binding:Some(binding.clone())}, idempotency_key:key("verified-review")};
      let result = commands.handle_command(&context(), command.clone()).await.unwrap();
      assert_eq!(result.association.binding, Some(binding));
      assert!(matches!(
        result.association.provenance,
        octacity_server_store::IssueCodeReviewProvenance::Explicit { .. }
      ));
      assert_eq!(factory.factory_run_snapshot(snapshot.run.id()).await.unwrap(), snapshot);
      assert_eq!(store.issue(issue.project_id, issue.id, None).await.unwrap(), issue);
      let mut forged = command.clone();
      forged.idempotency_key = key("wrong-record");
      forged.intent.binding.as_mut().unwrap().record = FactoryDigest::from_bytes([82; 32]);
      assert!(commands.handle_command(&context(), forged).await.is_err());
      let mut foreign = command.clone();
      foreign.idempotency_key = key("foreign-issue");
      foreign.intent.binding.as_mut().unwrap().association.issue_id = octacity_server_domain::IssueId::generate();
      assert!(commands.handle_command(&context(), foreign).await.is_err());
      let mut forged_delivery = command.clone();
      forged_delivery.idempotency_key = key("fake-delivery");
      assert!(
        commands
          .handle_command(
            &context(),
            AttachIssueCodeReviewDeliveryCommand {
              attachment: forged_delivery
            }
          )
          .await
          .is_err(),
        "a candidate fact is not a Delivery receipt"
      );
      let hidden = Arc::new(ResourcePolicy {
        blocked: ManagementResourceKind::FactoryRun,
      });
      let hidden_executions = Arc::new(IssueExecutionHandlers::new(
        store.clone(),
        factory.clone(),
        hidden.clone(),
      ));
      let hidden_commands = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(
          IssueCodeReviewHandlers::new(
            store.clone(),
            Arc::new(Reader {
              repository: fixture.input.exact_resource().unwrap().repository_id(),
            }),
            policy,
            Arc::new(Clock),
          )
          .with_execution_reader(hidden_executions),
        ),
      );
      let mut hidden_command = command.clone();
      hidden_command.idempotency_key = key("hidden-run");
      assert!(
        hidden_commands
          .handle_command(&context(), hidden_command)
          .await
          .is_err()
      );

      for delivery in [false, true] {
        let handlers = Arc::new(
          IssueCodeReviewHandlers::new(
            store.clone(),
            Arc::new(Reader {
              repository: fixture.input.exact_resource().unwrap().repository_id(),
            }),
            Arc::new(TrustedNetworkManagementPolicy),
            Arc::new(Clock),
          )
          .with_execution_reader(Arc::new(ReceiptContract { receipt: !delivery })),
        );
        let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
        let mut command = command.clone();
        command.idempotency_key = key(if delivery {
          "missing-delivery-receipt"
        } else {
          "unexpected-delivery-receipt"
        });
        if delivery {
          assert!(
            commands
              .handle_command(&context(), AttachIssueCodeReviewDeliveryCommand { attachment: command })
              .await
              .is_err()
          );
        } else {
          assert!(commands.handle_command(&context(), command).await.is_err());
        }
      }
    }
  });
}

struct ReceiptContract {
  receipt: bool,
}
#[async_trait::async_trait]
impl IssueCodeReviewExecutionReader for ReceiptContract {
  async fn verify(
    &self,
    _: &ManagementRequestContext,
    _: &IssueCodeReviewBinding,
    _: &IssueCodeReviewSelection,
    _: bool,
  ) -> Result<Option<IssueCodeReviewPublication>, ApplicationError> {
    Ok(self.receipt.then(|| IssueCodeReviewPublication {
      receipt: FactoryDigest::from_bytes([85; 32]),
      target: crate::factory_admission_tests::exact("review_branch", 85),
    }))
  }
}
