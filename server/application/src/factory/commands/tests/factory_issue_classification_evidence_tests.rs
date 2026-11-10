//! Classification evidence uses the ordinary retained command and independently authorized body reader.
use super::*;

struct ArtifactPolicy(std::sync::atomic::AtomicBool);
#[async_trait]
impl ManagementAuthorizationPolicy for ArtifactPolicy {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if action == ManagementAction::Download
      && resource.kind() == ManagementResourceKind::Artifact
      && self.0.load(std::sync::atomic::Ordering::SeqCst)
    {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}

async fn retained_observation(primary: PrimaryReport) -> (Fixture, IssueClassificationEvidenceReference) {
  let f = observed_command_fixture(primary).await;
  let run = f.intent.input().factory_run_id();
  let initial = f.runs.factory_run_snapshot(run).await.unwrap();
  let claim = initial.current_claim.unwrap().id;
  let (executor, _) = f.executor();
  let executors = FactoryNodeExecutorRegistry::new(vec![(
    FlowNodeKind::BuildCommand,
    NodeExecutionIdentity::External(f.profile.executor.clone()),
    executor,
  )])
  .unwrap();
  let flow = FactoryFlowRunner::new(
    f.runs.clone(),
    Arc::new(executors),
    Arc::new(crate::factory_flow_tests::Inputs),
    f.clock.clone(),
  );
  for tick in 32..40 {
    flow.reconcile_flow(run, claim, at(tick)).await.unwrap();
    if !f
      .runs
      .factory_run_snapshot(run)
      .await
      .unwrap()
      .flow
      .data
      .records
      .is_empty()
    {
      break;
    }
  }
  let retained = f.runs.factory_run_snapshot(run).await.unwrap();
  let record = retained
    .flow
    .data
    .records
    .first()
    .expect("ordinary command's accepted observation");
  let output = record.primary_build_output().unwrap();
  let contract = f.profile.observation_contract.as_ref().unwrap();
  let reference = IssueClassificationEvidenceReference {
    work_id: retained.work.id(),
    run_id: run,
    record: record.digest().unwrap(),
    subject: FactoryTaskSubject::Exact(record.input().exact_resource().unwrap().clone()),
    environment: contract.environment.clone(),
    criterion: contract.criterion.clone(),
    observed_at: record.observed_at(),
    fresh_until: output.fresh_until,
  };
  (f, reference)
}

#[test]
fn issue_classification_evidence_uses_exact_retained_command_bytes_and_criterion() {
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
    let reader = FactoryIssueClassificationEvidenceReader::new(
      f.runs.clone(),
      Arc::new(FactoryArtifactReader::new(
        f.artifacts.clone(),
        Arc::new(f.query.clone()),
        f.bytes.clone(),
      )),
      vec![f.profile.clone()],
      Arc::new(TrustedNetworkManagementPolicy),
    )
    .unwrap();
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let proof = reader
      .read(&context, &reference, at(1000))
      .await
      .unwrap()
      .expect("qualified independently readable evidence");
    assert_eq!(proof.finding(), FactoryCommandFinding::Matched);
    assert!(proof.review_claim(&reference, "reproduced").is_empty());
    assert_eq!(
      proof.review_claim(&reference, "probably"),
      vec![IssueClassificationProblem::UnsupportedReproduction]
    );
    assert_eq!(
      proof.review_claim(&reference, "not_reproduced"),
      vec![IssueClassificationProblem::EvidenceClaimConflict {
        record: reference.record
      }]
    );
    let mut forged = reference.clone();
    forged.criterion = ImmutableReference::new(
      fixtures::key("another-criterion"),
      fixtures::key("v1"),
      FactoryDigest::from_bytes([92; 32]),
    );
    assert!(reader.read(&context, &forged, at(1000)).await.unwrap().is_none());
    let snapshot = f.runs.factory_run_snapshot(reference.run_id).await.unwrap();
    let artifact = snapshot.flow.data.records[0]
      .primary_build_output()
      .unwrap()
      .artifact
      .artifact_id();
    f.bytes.0.lock().unwrap().remove(&artifact);
    assert!(
      reader.read(&context, &reference, at(1000)).await.unwrap().is_none(),
      "the journal must not resurrect missing private evidence bytes"
    );
  });
}

async fn retained_issue_assessment(
  issues: &InMemoryProjectStore,
  reference: &IssueClassificationEvidenceReference,
  claim: &str,
) -> IssueRevision {
  let project_id = reference.subject.project_id();
  issues
    .create_project(management_mutation(CreateProject {
      id: project_id,
      parent_id: None,
      name: ProjectName::new("Evidence review").unwrap(),
      idempotency_key: "project".parse().unwrap(),
      created_at: at(1),
    }))
    .await
    .unwrap();
  issues.publish_issue_properties(management_mutation(PublishIssueProperties {
      intent:PublishIssuePropertiesIntent {project_id,expected_version:0,definition:serde_json::from_value(serde_json::json!({"labels":[],"properties":[
        {"id":"reproduction","name":"Reproduction","retired":false,"kind":{"type":"choice","choices":[{"id":"reproduced","name":"Reproduced","retired":false},{"id":"unknown","name":"Unknown","retired":false}]}}
      ],"classification":{"reproducibility":"reproduction"}})).unwrap()},
      idempotency_key:"catalog".parse().unwrap(),published_at:at(2),
    })).await.unwrap();
  let created = issues
    .create_issue(management_mutation(CreateIssue {
      id: IssueId::generate(),
      assignees: Default::default(),
      intent: CreateIssueIntent {
        project_id,
        planning: Default::default(),
        content: IssueContent {
          title: "Defect".into(),
          description: String::new(),
          acceptance: None,
          priority: None,
        },
      },
      idempotency_key: "issue".parse().unwrap(),
      created_at: at(2),
    }))
    .await
    .unwrap()
    .issue;
  let mut restored = serde_json::to_value(&created).unwrap();
  restored["version"] = serde_json::json!(2);
  restored["planning"] =
    serde_json::json!({"definition_version":1,"labels":[],"values":{"reproduction":{"type":"choice","value":claim}}});
  restored["classification"] = serde_json::json!({"fields":[{"field":{"kind":"property","id":"reproduction","definition_version":1},
      "origin":{"kind":"manual","actor":{"kind":"unauthenticated_management","identity":null}},"assessed_version":1,"requirements_version":1,"assessed_at":at(2),"evidence":[reference.record]}],"evidence":[reference]});
  let restored: IssueRevision = serde_json::from_value(restored).unwrap();
  issues.seed_retained_issue_revision(restored.clone()).unwrap();
  restored
}

#[test]
fn issue_classification_review_qualifies_evidence_without_private_producer_access_or_mutations() {
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
    let created = retained_issue_assessment(&issues, &reference, "reproduced").await;
    let project_id = created.project_id;
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let query = ReviewIssueClassificationQuery {
      project_id,
      id: created.id,
      version: None,
      subject: Some(reference.subject.clone()),
      checked_at: at(1000),
    };
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let unavailable = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(issues.clone(), policy.clone())),
    )
    .handle_query(&context, query.clone())
    .await
    .unwrap();
    assert_eq!(
      unavailable.fields[0].problems,
      vec![IssueClassificationProblem::EvidenceUnavailable {
        record: reference.record
      }]
    );
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
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(issues.clone(), policy).with_classification_evidence_reader(reader)),
    );
    let before = issues.mutation_evidence_counts().await;
    let reviewed = queries.handle_query(&context, query.clone()).await.unwrap();
    assert!(
      reviewed.fields[0].problems.is_empty(),
      "qualified current evidence supports this exact claim"
    );
    artifact_policy.0.store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
      queries.handle_query(&context, query.clone()).await.unwrap().fields[0].problems,
      vec![IssueClassificationProblem::EvidenceUnavailable {
        record: reference.record
      }]
    );
    artifact_policy.0.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut expired = query;
    expired.checked_at = reference.fresh_until;
    assert!(
      queries.handle_query(&context, expired).await.unwrap().fields[0]
        .problems
        .contains(&IssueClassificationProblem::EvidenceExpired {
          record: reference.record
        })
    );
    let mut unresolved = issues.issue(project_id, created.id, None).await.unwrap();
    unresolved.version = unresolved.version.next().unwrap();
    unresolved
      .planning
      .values
      .insert("reproduction".into(), IssuePropertyValue::Choice("unknown".into()));
    issues.seed_retained_issue_revision(unresolved).unwrap();
    artifact_policy.0.store(false, std::sync::atomic::Ordering::SeqCst);
    let unresolved = queries
      .handle_query(
        &context,
        ReviewIssueClassificationQuery {
          project_id,
          id: created.id,
          version: None,
          subject: Some(reference.subject.clone()),
          checked_at: at(1000),
        },
      )
      .await
      .unwrap();
    assert!(unresolved.fields[0].problems.is_empty());
    assert_eq!(
      unresolved.fields[0].evidence[0].problems,
      vec![IssueClassificationProblem::EvidenceUnavailable {
        record: reference.record
      }]
    );
    assert_eq!(issues.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_classification_review_preserves_historical_reproduction_after_catalog_rebinding() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let (f, reference) = retained_observation(|intent, profile| {
      let mut report = crate::factory::commands::tests::factory_command_observation_tests::report(
        intent,
        profile.observation_contract.as_ref().unwrap(),
      );
      report["observations"]["unexpected_statuses"] = serde_json::json!(0);
      report["finding"] = serde_json::json!("not_matched");
      Some(report)
    })
    .await;
    let issues = Arc::new(InMemoryProjectStore::new());
    let assessed = retained_issue_assessment(&issues, &reference, "reproduced").await;
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let reader = FactoryIssueClassificationEvidenceReader::new(
      f.runs.clone(),
      Arc::new(FactoryArtifactReader::new(
        f.artifacts.clone(),
        Arc::new(f.query.clone()),
        f.bytes.clone(),
      )),
      vec![f.profile.clone()],
      policy.clone(),
    )
    .unwrap();
    let queries = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueHandlers::new(issues.clone(), policy).with_classification_evidence_reader(Arc::new(reader))),
    );
    let query = ReviewIssueClassificationQuery {
      project_id: assessed.project_id,
      id: assessed.id,
      version: Some(assessed.version),
      subject: Some(reference.subject.clone()),
      checked_at: at(1000),
    };
    let expected = vec![IssueClassificationProblem::EvidenceClaimConflict {
      record: reference.record,
    }];
    assert_eq!(
      queries.handle_query(&context, query.clone()).await.unwrap().fields[0].problems,
      expected
    );
    let mut catalog = issues.issue_properties(assessed.project_id, None).await.unwrap();
    let mut replacement = catalog.definition.properties[0].clone();
    replacement.id = "replacement_reproduction".into();
    catalog.definition.properties.push(replacement);
    catalog.definition.classification.reproducibility = Some("replacement_reproduction".into());
    issues
      .publish_issue_properties(management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: assessed.project_id,
          expected_version: catalog.version,
          definition: catalog.definition,
        },
        idempotency_key: "move-reproduction-binding".parse().unwrap(),
        published_at: at(4),
      }))
      .await
      .unwrap();
    let before = issues.mutation_evidence_counts().await;
    let review = queries.handle_query(&context, query).await.unwrap();
    assert_eq!(
      review.fields[0].problems, expected,
      "a new binding must not reinterpret the same retained reproduction claim as unknown"
    );
    assert_eq!(
      issues
        .issue(assessed.project_id, assessed.id, Some(assessed.version))
        .await
        .unwrap(),
      assessed
    );
    assert_eq!(issues.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_classification_evidence_distinguishes_negative_observations_from_technical_conditions() {
  crate::factory::admission::tests::factory_admission_tests::run_ready(async {
    let cases: [(FactoryCommandFinding, PrimaryReport); 2] = [
      (FactoryCommandFinding::NotMatched, |intent, profile| {
        let mut report = crate::factory::commands::tests::factory_command_observation_tests::report(
          intent,
          profile.observation_contract.as_ref().unwrap(),
        );
        report["observations"]["unexpected_statuses"] = serde_json::json!(0);
        report["finding"] = serde_json::json!("not_matched");
        Some(report)
      }),
      (FactoryCommandFinding::Indeterminate, |intent, profile| {
        let mut report = crate::factory::commands::tests::factory_command_observation_tests::report(
          intent,
          profile.observation_contract.as_ref().unwrap(),
        );
        report["status"] = serde_json::json!("environment_unavailable");
        report["finding"] = serde_json::json!("indeterminate");
        report.as_object_mut().unwrap().remove("observations");
        Some(report)
      }),
    ];
    for (expected, report) in cases {
      let (f, reference) = retained_observation(report).await;
      let reader = FactoryIssueClassificationEvidenceReader::new(
        f.runs.clone(),
        Arc::new(FactoryArtifactReader::new(
          f.artifacts.clone(),
          Arc::new(f.query.clone()),
          f.bytes.clone(),
        )),
        vec![f.profile.clone()],
        Arc::new(TrustedNetworkManagementPolicy),
      )
      .unwrap();
      let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
      let proof = reader.read(&context, &reference, at(1000)).await.unwrap().unwrap();
      assert_eq!(proof.finding(), expected);
      let problems = proof.review_claim(&reference, "not_reproduced");
      if expected == FactoryCommandFinding::NotMatched {
        assert!(problems.is_empty());
      } else {
        assert_eq!(
          problems,
          vec![IssueClassificationProblem::EvidenceInconclusive {
            record: reference.record
          }]
        );
      }
    }
  });
}
