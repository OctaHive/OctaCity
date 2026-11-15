//! Direct field claims consume independently qualified ordinary command observations.
use super::*;
use crate::factory::admission::tests::factory_admission_tests::{
  configuration_with_flow, seed_permitted_configuration,
};

fn consumer(definition_version: u64, reference: &IssueClassificationEvidenceReference) -> FactoryFlowConfiguration {
  let key = |v: &str| FactoryKey::new(v).unwrap();
  let schema = FlowDataSchema::new(
    key("ci.snapshot"),
    key("1"),
    FlowValueSchema::Object {
      fields: [
        (
          key("signal"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 256,
          }),
        ),
        (
          key("proof"),
          FlowFieldSchema::required(FlowValueSchema::Array {
            item: Box::new(FlowValueSchema::String {
              min_bytes: 64,
              max_bytes: 64,
            }),
            min_items: 1,
            max_items: 1,
          }),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  let budget = BudgetLimit::new(2, 10000, 1000, 1000, 10000).unwrap();
  let permissions = FactoryPermissionSet::deny_all();
  let node=FlowNodeDefinition::new(FlowNodeDefinitionInput{key:key("consume_ci"),kind:FlowNodeKind::DeterministicGate,input_schema:Some(schema.reference().clone()),outcomes:vec![FlowOutcomeDefinition::new(key("done"),FlowOutcomeKind::Success,schema.reference().clone())],budget,permissions:permissions.clone(),required:true,subflow:None}).unwrap()
    .with_gate(FlowGateProgram::new(vec![],key("done")).unwrap().with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer{path:String::new()}).unwrap()).unwrap().binding().unwrap()).unwrap()
    .with_input_binding(serde_json::from_value(serde_json::json!({"sources":{"issue":{"source":"incoming","schema":schema.reference(),"path":""}},"mapping":{"value":{"mapping":"pointer","path":"/issue"}}})).unwrap()).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("consume_ci"),
      key("done"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, permissions).unwrap(),
    vec![schema.clone()],
    Default::default(),
  )
  .unwrap()
  .with_issue_snapshot(
    serde_json::from_value(serde_json::json!({
      "version":1,"schema":schema.reference(),"definition_version":definition_version,
      "fields":{"signal":{"source":{"kind":"property","id":"reproduction"},"required":true}},
      "admission":{"priority":{"field":"signal","values":{"reproduced":17}},"risk":{"field":"signal","values":{"reproduced":"low"}}},
      "evidence":{"proof":{"field":"signal","required":true,"maximum":1,"observation":{
        "environment":reference.environment,"criterion":reference.criterion,"finding":"matched"
      }}}
    }))
    .unwrap(),
  )
  .unwrap()
}

#[test]
fn direct_issue_snapshot_requires_current_trusted_observation_without_fixed_roles() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let (f, reference) = retained_observation(|intent, profile| {
      Some(
        crate::factory::commands::tests::factory_command_observation_tests::report(
          intent,
          profile.observation_contract.as_ref().unwrap(),
        ),
      )
    })
    .await;
    let issues = Arc::new(InMemoryProjectStore::new());
    let issue = retained_issue_assessment(&issues, &reference, "reproduced").await;
    let mut catalog = issues.issue_properties(issue.project_id, None).await.unwrap();
    catalog.definition.classification = Default::default();
    let catalog = issues
      .publish_issue_properties(management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: issue.project_id,
          expected_version: catalog.version,
          definition: catalog.definition,
        },
        idempotency_key: "no-observation-roles".parse().unwrap(),
        published_at: at(4),
      }))
      .await
      .unwrap()
      .resource;
    let configuration = configuration_with_flow(
      issue.project_id,
      FactoryConfigurationId::generate(),
      Some(consumer(catalog.version, &reference)),
    );
    let target = IssueExecutionTarget {
      configuration_id: configuration.reference().id(),
      configuration_version: configuration.reference().version(),
      repository_id: reference.subject.exact().unwrap().repository_id(),
      repository_version: RepositoryVersion::INITIAL,
      source: RepositorySourceSelection::ExactRevision(reference.subject.exact().unwrap().base_revision().clone()),
      mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
      predecessor_run: None,
    };
    f.runs.seed_project(issue.project_id).unwrap();
    f.runs
      .seed_repository_version(crate::factory::admission::tests::factory_admission_tests::repository(
        issue.project_id,
        target.repository_id,
        None,
      ))
      .unwrap();
    seed_permitted_configuration(
      &f.runs,
      PublishedFactoryConfiguration {
        configuration,
        published_at: at(4),
      },
    )
    .unwrap();
    let artifact_policy = Arc::new(ArtifactPolicy(std::sync::atomic::AtomicBool::new(true)));
    let reader = Arc::new(
      FactoryIssueClassificationEvidenceReader::new(
        f.runs.clone(),
        Arc::new(FactoryArtifactReader::new(
          f.artifacts.clone(),
          Arc::new(f.query.clone()),
          f.bytes.clone(),
        )),
        vec![f.profile.clone()],
        artifact_policy.clone(),
      )
      .unwrap(),
    );
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let observations = Arc::new(IssueSnapshotObservations::new(
      reader.clone(),
      Arc::new(ReadClock(1000)),
    ));
    let handlers = Arc::new(
      IssueExecutionHandlers::new(issues.clone(), f.runs.clone(), policy.clone())
        .with_observations(observations.clone()),
    );
    let queries = AuthorizedQueryHandler::new(policy.clone(), handlers.clone());
    let query = ReviewIssueExecutionQuery {
      intent: RequestIssueExecutionIntent {
        project_id: issue.project_id,
        issue_id: issue.id,
        expected_version: issue.version,
        target,
        expected_snapshot_digest: None,
        classification: None,
      },
    };
    let before = issues.mutation_evidence_counts().await;
    let reviewed = queries
      .handle_query(
        &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
        query.clone(),
      )
      .await
      .unwrap();
    let snapshot = reviewed.snapshot.unwrap();
    assert_eq!(
      snapshot.values[&FactoryKey::new("signal").unwrap()],
      serde_json::json!("reproduced")
    );
    assert_eq!(
      snapshot.values[&FactoryKey::new("proof").unwrap()],
      serde_json::json!([reference.record])
    );
    assert_eq!(
      snapshot.evidence[&FactoryKey::new("proof").unwrap()],
      vec![reference.clone()]
    );
    let selected = f
      .runs
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: issue.project_id,
        configuration_id: query.intent.target.configuration_id,
        configuration_version: query.intent.target.configuration_version,
        repository_id: Some(query.intent.target.repository_id),
        repository_version: Some(query.intent.target.repository_version),
      })
      .await
      .unwrap();
    let original = f.runs.factory_run_snapshot(reference.run_id).await.unwrap();
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      selected.configuration.configuration.reference().clone(),
      ExternalWorkIdentity::new("independent-ci-consumer").unwrap(),
      reference.subject.exact().unwrap().clone(),
      original.work.artifacts().clone(),
      snapshot.admission_classification.clone().unwrap(),
    )
    .unwrap();
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    let flow = AdmittedFlow::from_configuration(&selected.configuration.configuration, &run).unwrap();
    let incoming = snapshot
      .incoming(&work, &flow, flow.data_schema(&snapshot.input.schema).unwrap())
      .unwrap();
    let mut admission = AdmitFactoryWork {
      probe: FactoryAdmissionProbe {
        source_scope: FactoryWorkSourceScope {
          source: FactoryKey::new("manual").unwrap(),
          security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
        },
        external_identity: work.external_identity().clone(),
        intent_digest: FactoryDigest::from_bytes([212; 32]),
        idempotency_key: "independent-ci-proof".parse().unwrap(),
      },
      repository_version: Some(query.intent.target.repository_version),
      work,
      run,
      flow,
      incoming: Some(incoming),
      admitted_at: at(1000),
    };
    assert!(
      snapshot
        .validate_admitted_input(&admission, admission.admitted_at)
        .is_ok(),
      "the reviewed exact subject and current proof qualify"
    );
    admission.admitted_at = reference.fresh_until;
    assert!(
      snapshot
        .validate_admitted_input(&admission, admission.admitted_at)
        .is_err(),
      "observation expiry between review and admission cannot mint immutable Work"
    );
    assert_eq!(issues.mutation_evidence_counts().await, before);
    let request_context =
      ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let request = AuthorizedCommandHandler::new(policy.clone(), handlers)
      .handle_command(
        &request_context,
        RequestIssueExecutionCommand {
          intent: query.intent.clone(),
          idempotency_key: "ci-consumer-request".parse().unwrap(),
          requested_at: at(1000),
        },
      )
      .await
      .unwrap()
      .request;
    let publish = PublishIssueExecutionCommand {
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: request.id,
      classification: snapshot.admission_classification.clone().unwrap(),
    };
    let publisher = |bytes| {
      AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(
          IssueExecutionPublisher::new(
            issues.clone(),
            f.runs.clone(),
            bytes,
            Arc::new(ExactRevisionResolver),
            policy.clone(),
            Arc::new(ReadClock(1000)),
          )
          .with_observations(observations.clone()),
        ),
      )
    };
    let late_revocation = Arc::new(RevokingBytes {
      bytes: f.bytes.clone(),
      policy: artifact_policy.clone(),
    });
    let error = publisher(late_revocation)
      .handle_command(&request_context, publish.clone())
      .await
      .expect_err("Artifact authority lost during byte publication must block admission");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("{error:?}")
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/evidence/proof");
    let frozen = f
      .runs
      .project_text_documents(
        issue.project_id,
        &request_context.security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    assert!(
      frozen
        .issue_snapshot()
        .unwrap()
        .evidence
        .values()
        .flatten()
        .any(|proof| proof == &reference)
    );
    artifact_policy.0.store(true, std::sync::atomic::Ordering::SeqCst);
    let late_clock = Arc::new(ObservationClock(std::sync::atomic::AtomicI64::new(1000)));
    let delayed = Arc::new(IssueSnapshotObservations::new(
      Arc::new(ExpireDuringRead {
        reader: reader.clone(),
        clock: late_clock.clone(),
      }),
      late_clock.clone(),
    ));
    let late_admission = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueExecutionPublisher::new(
          issues.clone(),
          f.runs.clone(),
          f.bytes.clone(),
          Arc::new(ExactRevisionResolver),
          policy.clone(),
          late_clock.clone(),
        )
        .with_observations(delayed),
      ),
    );
    let error = late_admission
      .handle_command(&request_context, publish.clone())
      .await
      .expect_err(
        "proof that expires after its final Artifact read cannot be admitted using the original reservation time",
      );
    assert!(matches!(
      error.application(),
      Some(ApplicationError::Store(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Issue,
      }))
    ));
    assert_eq!(late_clock.now().unwrap(), reference.fresh_until);
    let reserved = f
      .runs
      .project_text_preparation(
        issue.project_id,
        &request_context.security_scope().to_store(),
        &request.admission_key(),
      )
      .await
      .unwrap()
      .unwrap();
    assert_eq!(reserved.reserved_at, at(1000));
    assert!(
      reserved.admission.is_none(),
      "failed fresh admission must preserve a pending preparation"
    );
    // Retry uses the original retained proof, then an admitted replay needs no new observation.
    let recovery = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        IssueExecutionPublisher::new(
          issues.clone(),
          f.runs.clone(),
          f.bytes.clone(),
          Arc::new(ExactRevisionResolver),
          policy.clone(),
          Arc::new(ReadClock(1000)),
        )
        .with_observations(observations.clone()),
      ),
    );
    let published = recovery
      .handle_command(&request_context, publish.clone())
      .await
      .unwrap();
    artifact_policy.0.store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
      recovery.handle_command(&request_context, publish).await.unwrap(),
      published
    );
    assert_eq!(
      f.runs
        .project_text_documents(
          issue.project_id,
          &request_context.security_scope().to_store(),
          &request.admission_key()
        )
        .await
        .unwrap()
        .unwrap(),
      frozen
    );
    let before = issues.mutation_evidence_counts().await;
    artifact_policy.0.store(false, std::sync::atomic::Ordering::SeqCst);
    let error = queries
      .handle_query(
        &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
        query.clone(),
      )
      .await
      .err()
      .expect("current Artifact authority remains independent of Issue visibility");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("evidence diagnostics: {error:?}")
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/evidence/proof");
    artifact_policy.0.store(true, std::sync::atomic::Ordering::SeqCst);
    for case in ["environment", "criterion", "finding", "subject", "expired", "missing"] {
      let mut flow = consumer(catalog.version, &reference);
      let input = flow.issue_snapshot.as_mut().unwrap();
      let expected = input
        .evidence
        .get_mut(&FactoryKey::new("proof").unwrap())
        .unwrap()
        .observation
        .as_mut()
        .unwrap();
      let other = ImmutableReference::new(
        FactoryKey::new("other-contract").unwrap(),
        FactoryKey::new("1").unwrap(),
        FactoryDigest::from_bytes([211; 32]),
      );
      match case {
        "environment" => expected.environment = other,
        "criterion" => expected.criterion = other,
        "finding" => expected.finding = FactoryCommandFinding::NotMatched,
        _ => {}
      }
      let configuration = configuration_with_flow(issue.project_id, FactoryConfigurationId::generate(), Some(flow));
      let mut denied = query.clone();
      denied.intent.target.configuration_id = configuration.reference().id();
      if case == "subject" {
        denied.intent.target.source =
          RepositorySourceSelection::ExactRevision(ImmutableRevision::new("another-subject").unwrap());
      }
      if case == "missing" {
        let mut missing = issue.clone();
        missing.version = missing.version.next().unwrap();
        missing.classification.fields[0].evidence.clear();
        denied.intent.expected_version = missing.version;
        issues.seed_retained_issue_revision(missing).unwrap();
      }
      seed_permitted_configuration(
        &f.runs,
        PublishedFactoryConfiguration {
          configuration,
          published_at: at(5),
        },
      )
      .unwrap();
      let readers = Arc::new(IssueSnapshotObservations::new(
        reader.clone(),
        Arc::new(ReadClock(if case == "expired" {
          reference.fresh_until.unix_millis()
        } else {
          1000
        })),
      ));
      let owner = AuthorizedQueryHandler::new(
        policy.clone(),
        Arc::new(
          IssueExecutionHandlers::new(issues.clone(), f.runs.clone(), policy.clone()).with_observations(readers),
        ),
      );
      let error = owner
        .handle_query(
          &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
          denied,
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("{case} cannot satisfy a declared trusted observation"));
      let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
        panic!("{case}: {error:?}")
      };
      assert_eq!(report.errors[0].field, "/issue_snapshot/evidence/proof", "{case}");
    }
    assert_eq!(issues.mutation_evidence_counts().await, before);
  });
}

/// Revokes the independent read grant after preparation, while retaining the actual published bytes.
struct RevokingBytes {
  bytes: Arc<Bytes>,
  policy: Arc<ArtifactPolicy>,
}
#[async_trait]
impl ArtifactBytePublisher for RevokingBytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.bytes.publish_verified(object, bytes).await?;
    self.policy.0.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
  }
}
#[async_trait]
impl ArtifactByteReader for RevokingBytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    self.bytes.read_published(object, maximum).await
  }
}

struct ObservationClock(std::sync::atomic::AtomicI64);
impl FactoryExecutionClock for ObservationClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(self.0.load(std::sync::atomic::Ordering::SeqCst)))
  }
}
/// Actual qualification finishes while the original evidence deadline passes.
struct ExpireDuringRead {
  reader: Arc<dyn IssueClassificationEvidenceReader>,
  clock: Arc<ObservationClock>,
}
#[async_trait]
impl IssueClassificationEvidenceReader for ExpireDuringRead {
  async fn read(
    &self,
    context: &ManagementRequestContext,
    reference: &IssueClassificationEvidenceReference,
    checked_at: Timestamp,
  ) -> Result<Option<VerifiedIssueClassificationEvidence>, ApplicationError> {
    let result = self.reader.read(context, reference, checked_at).await?;
    self
      .clock
      .0
      .store(reference.fresh_until.unix_millis(), std::sync::atomic::Ordering::SeqCst);
    Ok(result)
  }
}
