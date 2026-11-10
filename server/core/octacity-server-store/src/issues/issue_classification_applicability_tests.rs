//! Exact assessment inputs are retained data; freshness is checked through public Issue contracts.
use super::*;

#[test]
fn classification_review_compares_the_declared_exact_code_subject() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.issue_properties(original.project_id, None).await.unwrap();
    let subject = octacity_server_factory::FactoryTaskSubject::Exact(octacity_server_factory::ExactSubject::new(
      original.project_id,
      octacity_server_domain::RepositoryId::generate(),
      "base-a".parse().unwrap(),
    ));
    let changed = octacity_server_factory::FactoryTaskSubject::Exact(octacity_server_factory::ExactSubject::new(
      original.project_id,
      subject.exact().unwrap().repository_id(),
      "base-b".parse().unwrap(),
    ));
    let mut retained = serde_json::to_value(&original).unwrap();
    retained["classification"] = manual_priority();
    retained["classification"]["fields"][0]["inputs"] = json!({"subject":subject});
    let assessed: IssueRevision =
      serde_json::from_value(retained.clone()).expect("bounded exact assessment input pins");
    assert!(
      assessed
        .review_classification(
          &catalog,
          &[(catalog.version, catalog.clone())].into(),
          Some(&subject),
          at(5)
        )
        .fields[0]
        .problems
        .is_empty()
    );
    assert_eq!(
      assessed
        .review_classification(
          &catalog,
          &[(catalog.version, catalog.clone())].into(),
          Some(&changed),
          at(5)
        )
        .fields[0]
        .problems,
      vec![IssueClassificationProblem::CodeSubjectChanged]
    );
    assert_eq!(
      assessed
        .review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5))
        .fields[0]
        .problems,
      vec![IssueClassificationProblem::CodeSubjectUnavailable]
    );
    assert_eq!(serde_json::to_value(&assessed).unwrap(), retained);
  });
}

#[test]
fn classification_review_compares_only_declared_exact_material_dependencies() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.issue_properties(original.project_id, None).await.unwrap();
    let artifact = octacity_server_factory::FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      octacity_server_factory::FactoryDigest::from_bytes([12; 32]),
      5,
    )
    .unwrap();
    for reference in [
      json!({"kind":"comment","project_id":original.project_id,"issue_id":original.id,"comment_id":octacity_server_domain::IssueCommentId::generate(),"body_digest":"a".repeat(64)}),
      json!({"kind":"issue_document","project_id":original.project_id,"issue_id":original.id,"artifact":artifact}),
      json!({"kind":"project_document","project_id":original.project_id,"artifact":artifact}),
    ] {
      let mut retained = serde_json::to_value(&original).unwrap();
      retained["classification"] = manual_priority();
      retained["classification"]["fields"][0]["inputs"] = json!({"materials":[reference.clone()]});
      retained["materials"] = json!([{"reference":reference,"selected_by":{"kind":"unauthenticated_management","identity":null},"selected_at":at(2)}]);
      let assessed: IssueRevision =
        serde_json::from_value(retained.clone()).expect("bounded exact material dependencies");
      assert!(
        assessed
          .review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5))
          .fields[0]
          .problems
          .is_empty()
      );
      let unrelated: IssueMaterialReference = serde_json::from_value(
        json!({"kind":"comment","project_id":original.project_id,"issue_id":original.id,
        "comment_id":octacity_server_domain::IssueCommentId::generate(),"body_digest":"b".repeat(64)}),
      )
      .unwrap();
      let actor = AuditActor {
        kind: AuditActorKind::UnauthenticatedManagement,
        identity: None,
      };
      let with_unrelated = assessed
        .with_materials(
          &EditIssueMaterials {
            intent: EditIssueMaterialsIntent {
              project_id: original.project_id,
              id: original.id,
              expected_version: original.version,
              references: vec![serde_json::from_value(reference.clone()).unwrap(), unrelated.clone()],
            },
            idempotency_key: key("unrelated-assessment-input"),
            edited_at: at(3),
          },
          &actor,
        )
        .unwrap();
      assert!(
        with_unrelated
          .review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5))
          .fields[0]
          .problems
          .is_empty()
      );
      let removed = with_unrelated
        .with_materials(
          &EditIssueMaterials {
            intent: EditIssueMaterialsIntent {
              project_id: original.project_id,
              id: original.id,
              expected_version: with_unrelated.version,
              references: vec![unrelated],
            },
            idempotency_key: key("remove-assessment-input"),
            edited_at: at(4),
          },
          &actor,
        )
        .unwrap();
      assert_eq!(removed.applicability_version, assessed.applicability_version);
      assert_eq!(
        removed
          .review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5))
          .fields[0]
          .problems,
        vec![IssueClassificationProblem::MaterialChanged {
          reference: serde_json::from_value(reference).unwrap()
        }]
      );
      assert_eq!(removed.classification, assessed.classification);
      assert_eq!(serde_json::to_value(&assessed).unwrap(), retained);
    }
  });
}

#[test]
fn classification_application_retains_declared_inputs_and_rejects_substituted_materials() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store
      .publish_issue_properties(testing::management_mutation(PublishIssueProperties {
        intent: PublishIssuePropertiesIntent {
          project_id: original.project_id,
          expected_version: 0,
          definition: Default::default(),
        },
        idempotency_key: key("input-catalog"),
        published_at: at(3),
      }))
      .await
      .unwrap()
      .resource;
    let reference: IssueMaterialReference = serde_json::from_value(
      json!({"kind":"comment","project_id":original.project_id,"issue_id":original.id,
      "comment_id":octacity_server_domain::IssueCommentId::generate(),"body_digest":"a".repeat(64)}),
    )
    .unwrap();
    let actor = AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    };
    let baseline = original
      .with_materials(
        &EditIssueMaterials {
          intent: EditIssueMaterialsIntent {
            project_id: original.project_id,
            id: original.id,
            expected_version: original.version,
            references: vec![reference.clone()],
          },
          idempotency_key: key("assessed-material"),
          edited_at: at(3),
        },
        &actor,
      )
      .unwrap();
    let subject = octacity_server_factory::FactoryTaskSubject::Exact(octacity_server_factory::ExactSubject::new(
      original.project_id,
      octacity_server_domain::RepositoryId::generate(),
      "base-a".parse().unwrap(),
    ));
    let inputs = IssueClassificationAssessmentInputs {
      subject: Some(subject.clone()),
      materials: vec![reference],
    };
    let mut request = ApplyIssueClassification {
      association: IssueWorkAssociation {
        project_id: original.project_id,
        issue_id: original.id,
        request_id: None,
        requirements_version: Some(baseline.version),
        work_id: octacity_server_factory::WorkEnvelopeId::generate(),
        run_id: octacity_server_factory::FactoryRunId::generate(),
      },
      policy: "triage".parse().unwrap(),
      record: octacity_server_factory::FactoryDigest::from_bytes([1; 32]),
      operation_id: octacity_server_factory::FactoryDigest::from_bytes([2; 32]),
      accepted_at: at(4),
      applied_at: at(5),
      authority: None,
      priority: None,
      size_label: None,
      managed_labels: Default::default(),
      fields: Some(IssueClassificationUpdate {
        selection: IssueClassificationFieldSelection {
          definition_version: catalog.version,
          priority: false,
          component: false,
          kind: false,
          size: true,
          properties: Default::default(),
        },
        priority: None,
        component: None,
        issue_type: None,
        size: Some(IssueSize::Small),
        properties: Default::default(),
        inputs: inputs.clone(),
      }),
    };
    let applied = baseline
      .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
      .unwrap();
    let size = applied
      .classification
      .fields
      .iter()
      .find(|f| f.field == IssueClassificationFieldReference::Size)
      .unwrap();
    assert_eq!(size.inputs, inputs);
    assert!(
      applied
        .review_classification(
          &catalog,
          &[(catalog.version, catalog.clone())].into(),
          Some(&subject),
          at(5)
        )
        .fields
        .iter()
        .find(|f| f.field == IssueClassificationFieldReference::Size)
        .unwrap()
        .problems
        .is_empty()
    );
    let mut forged = serde_json::to_value(&inputs).unwrap();
    forged["materials"][0]["body_digest"] = json!("b".repeat(64));
    request.fields.as_mut().unwrap().inputs = serde_json::from_value(forged).unwrap();
    assert!(
      baseline
        .classified(&baseline, &request, None, &catalog, &Default::default(), &actor)
        .is_err()
    );
  });
}

#[test]
fn classification_review_reports_a_missing_original_definition_instead_of_reinterpreting_the_claim() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.issue_properties(original.project_id, None).await.unwrap();
    let mut retained = serde_json::to_value(&original).unwrap();
    retained["classification"] = manual_priority();
    retained["classification"]["fields"][0]["field"] =
      json!({"kind":"property","id":"reproduction","definition_version":1});
    retained["planning"] =
      json!({"definition_version":1,"labels":[],"values":{"reproduction":{"type":"choice","value":"reproduced"}}});
    let assessed: IssueRevision = serde_json::from_value(retained).unwrap();
    let review = assessed.review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5));
    assert_eq!(
      serde_json::to_value(&review.fields[0].problems).unwrap(),
      json!([{"code":"definition_unavailable"}])
    );
  });
}

#[test]
fn classification_review_rejects_unproved_reproduction_without_blocking_unresolved_values() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.publish_issue_properties(testing::management_mutation(PublishIssueProperties {
      intent:PublishIssuePropertiesIntent {project_id:original.project_id,expected_version:0,
        definition:serde_json::from_value(json!({"labels":[],"properties":[
          {"id":"reproduction","name":"Reproduction","retired":false,"kind":{"type":"choice","choices":[
            {"id":"reproduced","name":"Reproduced","retired":false},{"id":"not_reproduced","name":"Not reproduced","retired":false},
            {"id":"unknown","name":"Unknown","retired":false},{"id":"needs_input","name":"Needs input","retired":false}]}}
        ],"classification":{"reproducibility":"reproduction"}})).unwrap()},
      idempotency_key:key("repro-catalog"),published_at:at(3),
    })).await.unwrap().resource;
    let mut retained = serde_json::to_value(&original).unwrap();
    retained["classification"] = manual_priority();
    retained["classification"]["fields"][0]["field"] =
      json!({"kind":"property","id":"reproduction","definition_version":1});
    for claim in [
      "reproduced",
      "not_reproduced",
      "unknown",
      "needs_input",
      "needs_research",
      "not_applicable",
      "probably",
    ] {
      retained["planning"] =
        json!({"definition_version":1,"labels":[],"values":{"reproduction":{"type":"choice","value":claim}}});
      let assessed: IssueRevision = serde_json::from_value(retained.clone()).unwrap();
      let review = assessed.review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5));
      let reproduction = review
        .fields
        .iter()
        .find(|f| matches!(&f.field, IssueClassificationFieldReference::Property {id,..} if id == "reproduction"))
        .unwrap();
      assert_eq!(
        reproduction.problems,
        if matches!(claim, "reproduced" | "not_reproduced") {
          vec![IssueClassificationProblem::EvidenceMissing]
        } else if claim == "probably" {
          vec![IssueClassificationProblem::UnsupportedReproduction]
        } else {
          vec![]
        },
        "{claim}"
      );
    }
  });
}

#[test]
fn classification_review_requires_current_independently_readable_evidence() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.issue_properties(original.project_id, None).await.unwrap();
    let mut retained = serde_json::to_value(&original).unwrap();
    retained["version"] = json!(2);
    retained["classification"] = assessment(original.project_id);
    // An evidence-dependent built-in isolates evidence checks from catalog availability.
    retained["classification"]["fields"][1]["field"] = json!({"kind":"size"});
    retained["planning"] = json!({"definition_version":0,"size":"small","labels":[],"values":{}});
    let assessed: IssueRevision = serde_json::from_value(retained).unwrap();
    let evidence = &assessed.classification.evidence[0];
    let current = assessed.review_classification(
      &catalog,
      &[(catalog.version, catalog.clone())].into(),
      Some(&evidence.subject),
      at(5),
    );
    assert_eq!(
      current.fields[1].problems,
      vec![IssueClassificationProblem::EvidenceUnavailable {
        record: evidence.record
      }]
    );
    let expired = assessed.review_classification(
      &catalog,
      &[(catalog.version, catalog.clone())].into(),
      Some(&evidence.subject),
      at(10),
    );
    assert!(
      expired.fields[1]
        .problems
        .contains(&IssueClassificationProblem::EvidenceExpired {
          record: evidence.record
        })
    );
    let changed = octacity_server_factory::FactoryTaskSubject::Exact(octacity_server_factory::ExactSubject::new(
      original.project_id,
      evidence.subject.exact().unwrap().repository_id(),
      "changed".parse().unwrap(),
    ));
    let mismatched = assessed.review_classification(
      &catalog,
      &[(catalog.version, catalog.clone())].into(),
      Some(&changed),
      at(5),
    );
    assert!(
      mismatched.fields[1]
        .problems
        .contains(&IssueClassificationProblem::EvidenceSubjectChanged {
          record: evidence.record
        })
    );
    let no_subject =
      assessed.review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5));
    assert!(
      no_subject.fields[1]
        .problems
        .contains(&IssueClassificationProblem::EvidenceSubjectUnavailable {
          record: evidence.record
        })
    );
  });
}

#[test]
fn classification_review_preserves_unresolved_routes_when_the_selected_evidence_is_unavailable() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = PublishedIssueProperties {project_id:original.project_id,version:1,published_at:at(3),
      definition:serde_json::from_value(json!({"labels":[],"properties":[{"id":"reproduction","name":"Reproduction","retired":false,
        "kind":{"type":"choice","choices":[{"id":"unknown","name":"Unknown","retired":false},{"id":"needs_input","name":"Needs input","retired":false}]}}],
        "classification":{"reproducibility":"reproduction"}})).unwrap()};
    let mut retained = serde_json::to_value(&original).unwrap();
    retained["version"] = json!(2);
    retained["classification"] = assessment(original.project_id);
    for claim in ["unknown", "needs_input"] {
      retained["planning"] =
        json!({"definition_version":1,"labels":[],"values":{"reproduction":{"type":"choice","value":claim}}});
      let assessed: IssueRevision = serde_json::from_value(retained.clone()).unwrap();
      let review = assessed.review_classification(
        &catalog,
        &[(catalog.version, catalog.clone())].into(),
        Some(&assessed.classification.evidence[0].subject),
        at(5),
      );
      assert!(
        review.fields[1].problems.is_empty(),
        "{claim} does not claim a verified finding: {:?}",
        review.fields[1].problems
      );
    }
  });
}

#[test]
fn classification_review_distinguishes_missing_values_from_legacy_unattributed_values() {
  run_ready(async {
    let store = testing::InMemoryProjectStore::new();
    let original = ordinary_issue(&store).await;
    let catalog = store.issue_properties(original.project_id, None).await.unwrap();
    let review = original.review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5));
    assert_eq!(
      review
        .fields
        .iter()
        .find(|field| field.field == IssueClassificationFieldReference::Size)
        .unwrap()
        .problems,
      vec![IssueClassificationProblem::MissingValue]
    );
    let mut document = serde_json::to_value(&original).unwrap();
    document.as_object_mut().unwrap().remove("classification");
    let legacy: IssueRevision = serde_json::from_value(document).unwrap();
    let review = legacy.review_classification(&catalog, &[(catalog.version, catalog.clone())].into(), None, at(5));
    let priority = review
      .fields
      .iter()
      .find(|field| matches!(field.field, IssueClassificationFieldReference::Priority { .. }))
      .unwrap();
    assert_eq!(priority.problems, vec![IssueClassificationProblem::MissingAssessment]);
  });
}
