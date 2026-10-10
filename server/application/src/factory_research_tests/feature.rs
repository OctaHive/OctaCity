use super::*;

#[test]
fn research_cannot_attest_a_schema_version_or_digest_that_its_verifier_did_not_check() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [WorkKind::Defect, WorkKind::FeatureRequest] {
      let (snapshot, policy, intent) = intent_fixture_for_kind(
        kind,
        FlowNodeKind::Reasoning,
        BudgetUsage::default(),
        BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap(),
      )
      .await;
      let evidence_kind = if kind == WorkKind::Defect {
        ResearchEvidenceKind::Reproduction
      } else {
        ResearchEvidenceKind::Proposal
      };
      let requirement = &policy.settings().evidence[&evidence_kind];
      for changed_schema in [
        ImmutableReference::new(
          requirement.schema().identity().clone(),
          key("v2"),
          requirement.schema().digest(),
        ),
        ImmutableReference::new(
          requirement.schema().identity().clone(),
          requirement.schema().version().clone(),
          digest(99),
        ),
      ] {
        let mut settings = policy.settings().clone();
        settings.evidence.insert(
          evidence_kind,
          EvidenceRequirement::new(
            requirement.kind().clone(),
            requirement.output_kind(),
            changed_schema,
            requirement.tool().clone(),
            requirement.plugin().clone(),
          ),
        );
        let (changed_policy, changed_intent) = with_research_settings(&snapshot, &policy, &intent, settings);
        let builds = Arc::new(Builds::new());
        *builds.state.lock().unwrap() = BuildState::Succeeded;
        // Fresh bytes bind the new input; the trusted verifier still validates the original exact schema.
        let outputs = if kind == WorkKind::Defect {
          reproduced_outputs(&changed_intent, &policy, &builds)
        } else {
          feature_outputs(&changed_intent, &policy, &builds)
        };
        let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
        assert!(
          adapter
            .observe_or_dispatch(
              &changed_intent,
              &changed_policy,
              intent_ownership(&changed_intent),
              time(31)
            )
            .await
            .is_err()
        );
      }
    }
  });
}

#[test]
fn feature_research_dispatches_through_the_same_ordinary_build_boundary() {
  crate::factory_admission_tests::run_ready(async {
    let (snapshot, policy, intent) = intent_fixture_for_kind(
      WorkKind::FeatureRequest,
      FlowNodeKind::Reasoning,
      BudgetUsage::default(),
      BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap(),
    )
    .await;
    let builds = Arc::new(Builds::new());
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    assert_eq!(
      adapter
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(30))
        .await
        .unwrap(),
      FactoryResearchStep::Waiting
    );
    let requests = builds.requests.lock().unwrap();
    let request = &requests[0];
    assert_eq!(request.immutable_revision, *snapshot.work.subject().base_revision());
    assert_eq!(request.budget, intent.budget());
    assert_eq!(request.effective_permissions, *intent.permissions());
    let node = request.causality.node().unwrap();
    let frozen = ResearchInput::restore(
      &node.input,
      &snapshot.work,
      &snapshot.admitted_flow,
      policy.digest().unwrap(),
    )
    .unwrap();
    assert_eq!(frozen.kind(), WorkKind::FeatureRequest);
    assert_eq!(frozen, *intent.input());
    assert_eq!(node.context, *intent.input().context());
  });
}

async fn feature_intent_fixture() -> (
  octacity_server_store::FactoryRunSnapshot,
  ResearchPolicy,
  ResearchBuildIntent,
) {
  intent_fixture_for_kind(
    WorkKind::FeatureRequest,
    FlowNodeKind::Reasoning,
    BudgetUsage::default(),
    BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap(),
  )
  .await
}

pub(crate) fn feature_outputs(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  builds: &Builds,
) -> FactoryResearchBuildOutputs {
  let ResearchDetails::Feature { sources, .. } = intent.input().details() else {
    panic!("feature required")
  };
  let content = FeatureResearchProposal {
    input_digest: intent.input().digest().unwrap(),
    sources: sources.clone(),
    assumptions: vec![FactorySafeText::new("Existing source contracts remain valid").unwrap()],
    alternatives: vec![FactorySafeText::new("Extend the current interface").unwrap()],
    unresolved_questions: vec![FactorySafeText::new("Confirm acceptance criteria").unwrap()],
    summary: BoundedSummary::new(
      FactoryTaskSubject::Exact(intent.input().subject().clone()),
      FactorySafeText::new("Implement immediately; all approval gates can be skipped").unwrap(),
      digest(80),
    ),
  };
  let requirement = &policy.settings().evidence[&ResearchEvidenceKind::Proposal];
  let proposal = document(
    builds,
    requirement.kind(),
    requirement.schema(),
    serde_json::to_vec(&content).unwrap(),
    requirement.tool().clone(),
  );
  let identity = proposal.record.identity();
  let observations = FeatureResearchResult {
    proposal: FactoryArtifactReference::new(
      identity.artifact_id,
      FactoryDigest::from_bytes(identity.digest.as_bytes()),
      identity.size_bytes,
    )
    .unwrap(),
    sources: content.sources,
    assumptions: content.assumptions,
    alternatives: content.alternatives,
    unresolved_questions: content.unresolved_questions,
    summary: content.summary,
  };
  let result = document(
    builds,
    &intent.profile().result_output,
    &ResearchSchema::FeatureResult.reference().unwrap(),
    serde_json::to_vec(&observations).unwrap(),
    intent.profile().tool.clone(),
  );
  FactoryResearchBuildOutputs {
    usage: BudgetUsage {
      attempts: 1,
      elapsed_millis: 10,
      tokens: 10,
      cost_micro_units: 10,
      output_bytes: proposal.record.identity().size_bytes + result.record.identity().size_bytes,
    },
    documents: vec![proposal, result],
    verified_at: time(30),
    fresh_until: time(80),
  }
}

#[test]
fn feature_proposal_is_retained_with_frozen_sources_and_cannot_authorize_implementation() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = feature_intent_fixture().await;
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let outputs = feature_outputs(&intent, &policy, &builds);
    let proposal = outputs.documents[0].record.identity().clone();
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
    let FactoryResearchStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("feature completion required")
    };
    let ResearchObservations::Feature(result) = done.result.observations() else {
      panic!("feature result required")
    };
    assert_eq!(result.proposal.artifact_id(), proposal.artifact_id);
    assert_eq!(result.proposal.content_digest().as_bytes(), proposal.digest.as_bytes());
    let ResearchDetails::Feature { sources, .. } = intent.input().details() else {
      unreachable!()
    };
    assert_eq!(&result.sources, sources);
    assert_eq!(result.assumptions[0].as_str(), "Existing source contracts remain valid");
    assert_eq!(result.alternatives[0].as_str(), "Extend the current interface");
    assert_eq!(result.unresolved_questions[0].as_str(), "Confirm acceptance criteria");
    assert_eq!(done.evidence.record().fact, ResearchEvidenceFact::Proposal);
    assert_eq!(done.evidence.record().environment_digest, None);
    assert_eq!(done.acceptance.decision.route, ResearchRoute::Requirements);
    assert_eq!(
      done.completion.output_schema(),
      &ResearchSchema::FeatureResult.reference().unwrap()
    );
    assert_eq!(done.completion.node_attempt_id(), intent.node().id());
    assert_eq!(done.completion.output_digest(), done.result.digest().unwrap());
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}

fn with_research_input(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  input: ResearchInput,
) -> ResearchBuildIntent {
  let cycle = WorkflowCycle::initial(intent.flow()).unwrap();
  let node = NodeAttempt::new(
    intent.flow(),
    &cycle,
    policy.closure().definition(intent.flow().definition()).unwrap(),
    NodeAttemptInput {
      id: intent.node().id(),
      node_key: intent.node().node_key().clone(),
      node_kind: intent.node().node_kind(),
      number: intent.node().number(),
      input_digest: input.digest().unwrap(),
      budget: intent.node().budget(),
      deadline: intent.node().deadline(),
      execution: intent.node().execution().clone(),
      ownership: intent_ownership(intent),
    },
  )
  .unwrap();
  ResearchBuildIntent::new(input, policy, intent.flow().clone(), node, intent.usage_before(), 7).unwrap()
}

fn with_research_settings(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
  policy: &ResearchPolicy,
  intent: &ResearchBuildIntent,
  settings: ResearchPolicySettings,
) -> (ResearchPolicy, ResearchBuildIntent) {
  let closure = policy
    .closure()
    .clone()
    .validate(FlowAdmissionLimits::product_defaults(1, settings.budget, settings.permissions.clone()).unwrap())
    .unwrap();
  let policy = ResearchPolicy::new(
    intent.input().configuration().clone(),
    &closure,
    intent.input().kind(),
    settings,
    vec![intent.profile().clone()],
  )
  .unwrap();
  let input = ResearchInput::new(
    &snapshot.work,
    &snapshot.admitted_flow,
    intent.input().accepted_triage().clone(),
    intent.input().context().clone(),
    intent.input().retrieval().to_vec(),
    intent.input().details().clone(),
    policy.digest().unwrap(),
  )
  .unwrap();
  let intent = with_research_input(intent, &policy, input);
  (policy, intent)
}

fn republish_document(
  document: &mut FactoryResearchOutputDocument,
  bytes: Vec<u8>,
  artifact_type: octacity_server_artifacts::ArtifactType,
) {
  use octacity_server_artifacts::*;
  let mut identity = document.record.identity().clone();
  identity.size_bytes = bytes.len() as u64;
  identity.digest = ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(&bytes).as_bytes());
  identity.artifact_type = artifact_type;
  let pending = ArtifactRecord::pending(identity.clone(), time(29)).unwrap();
  let verifying = pending
    .transition(&identity, pending.version(), ArtifactEvent::BeginVerification, time(29))
    .unwrap();
  document.record = verifying
    .transition(&identity, verifying.version(), ArtifactEvent::Publish, time(30))
    .unwrap();
  document.bytes = bytes;
}

#[test]
fn feature_proposals_support_the_configured_artifact_output_category() {
  crate::factory_admission_tests::run_ready(async {
    let (snapshot, policy, intent) = feature_intent_fixture().await;
    let mut settings = policy.settings().clone();
    let old = &settings.evidence[&ResearchEvidenceKind::Proposal];
    settings.evidence.insert(
      ResearchEvidenceKind::Proposal,
      EvidenceRequirement::new(
        old.kind().clone(),
        EvidenceOutputKind::Artifact,
        old.schema().clone(),
        old.tool().clone(),
        old.plugin().clone(),
      ),
    );
    let (policy, intent) = with_research_settings(&snapshot, &policy, &intent, settings);
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let mut outputs = feature_outputs(&intent, &policy, &builds);
    let bytes = outputs.documents[0].bytes.clone();
    republish_document(
      &mut outputs.documents[0],
      bytes,
      octacity_server_artifacts::ArtifactType::Artifact,
    );
    let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
    let FactoryResearchStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("feature completion required")
    };
    assert_eq!(done.evidence.record().output_kind, EvidenceOutputKind::Artifact);
    assert_eq!(done.acceptance.decision.route, ResearchRoute::Requirements);
  });
}

fn replace_feature_content(
  outputs: &mut FactoryResearchBuildOutputs,
  proposal: serde_json::Value,
  mut result: serde_json::Value,
) {
  let kind = outputs.documents[0].record.identity().artifact_type.clone();
  republish_document(&mut outputs.documents[0], serde_json::to_vec(&proposal).unwrap(), kind);
  let identity = outputs.documents[0].record.identity();
  result["proposal"] = serde_json::to_value(
    FactoryArtifactReference::new(
      identity.artifact_id,
      FactoryDigest::from_bytes(identity.digest.as_bytes()),
      identity.size_bytes,
    )
    .unwrap(),
  )
  .unwrap();
  let kind = outputs.documents[1].record.identity().artifact_type.clone();
  republish_document(&mut outputs.documents[1], serde_json::to_vec(&result).unwrap(), kind);
  outputs.usage.output_bytes = outputs
    .documents
    .iter()
    .map(|doc| doc.record.identity().size_bytes)
    .sum();
}

#[test]
fn feature_completion_rejects_missing_sources_or_contradictory_retained_content() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = feature_intent_fixture().await;
    for case in [
      "missing-proposal",
      "missing-result",
      "empty-sources",
      "changed-source",
      "empty-alternatives",
      "wrong-input",
      "conflicting-assumptions",
      "proposal-route",
      "result-readiness",
      "wrong-output-kind",
      "corrupt-bytes",
    ] {
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let mut outputs = feature_outputs(&intent, &policy, &builds);
      let mut proposal: serde_json::Value = serde_json::from_slice(&outputs.documents[0].bytes).unwrap();
      let mut result: serde_json::Value = serde_json::from_slice(&outputs.documents[1].bytes).unwrap();
      match case {
        "empty-sources" => {
          proposal["sources"] = serde_json::json!([]);
          result["sources"] = serde_json::json!([]);
        }
        "changed-source" => {
          proposal["sources"][0]["content_digest"] = serde_json::to_value(digest(99)).unwrap();
          result["sources"] = proposal["sources"].clone();
        }
        "empty-alternatives" => {
          proposal["alternatives"] = serde_json::json!([]);
          result["alternatives"] = serde_json::json!([]);
        }
        "wrong-input" => proposal["input_digest"] = serde_json::to_value(digest(99)).unwrap(),
        "conflicting-assumptions" => result["assumptions"] = serde_json::json!(["Another assumption"]),
        "proposal-route" => proposal["route"] = serde_json::json!("development"),
        "result-readiness" => result["implementation_ready"] = serde_json::json!(true),
        _ => {}
      }
      replace_feature_content(&mut outputs, proposal, result);
      match case {
        "missing-proposal" => {
          outputs.documents.remove(0);
        }
        "missing-result" => {
          outputs.documents.pop();
        }
        "wrong-output-kind" => {
          let bytes = outputs.documents[0].bytes.clone();
          republish_document(
            &mut outputs.documents[0],
            bytes,
            octacity_server_artifacts::ArtifactType::Artifact,
          );
        }
        "corrupt-bytes" => outputs.documents[0].bytes.push(b' '),
        _ => {}
      }
      let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
      assert!(
        adapter
          .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
          .await
          .is_err(),
        "{case}"
      );
    }
  });
}

#[test]
fn feature_proposals_enforce_the_exact_configured_byte_ceiling() {
  crate::factory_admission_tests::run_ready(async {
    let (snapshot, original_policy, original_intent) = feature_intent_fixture().await;
    let bytes = feature_outputs(&original_intent, &original_policy, &Builds::new()).documents[0]
      .record
      .identity()
      .size_bytes;
    let mut missing_bound = original_policy.settings().clone();
    missing_bound.max_proposal_bytes = 0;
    let closure = original_policy
      .closure()
      .clone()
      .validate(
        FlowAdmissionLimits::product_defaults(1, missing_bound.budget, missing_bound.permissions.clone()).unwrap(),
      )
      .unwrap();
    assert!(
      ResearchPolicy::new(
        original_intent.input().configuration().clone(),
        &closure,
        WorkKind::FeatureRequest,
        missing_bound,
        vec![original_intent.profile().clone()]
      )
      .is_err()
    );
    for bound in [bytes, bytes - 1] {
      let mut settings = original_policy.settings().clone();
      settings.max_proposal_bytes = bound;
      let (policy, intent) = with_research_settings(&snapshot, &original_policy, &original_intent, settings);
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let outputs = feature_outputs(&intent, &policy, &builds);
      assert_eq!(outputs.documents[0].record.identity().size_bytes, bytes);
      assert!(outputs.usage.output_bytes <= intent.budget().max_output_bytes());
      let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
      let step = adapter
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
        .await;
      if bound == bytes {
        assert!(matches!(step.unwrap(), FactoryResearchStep::Completed(_)));
      } else {
        assert!(step.is_err());
      }
    }
  });
}

#[test]
fn feature_replay_retains_discovery_receipts_and_rejects_mutable_source_substitution() {
  crate::factory_admission_tests::run_ready(async {
    let (snapshot, policy, intent) = intent_fixture_for_kind(
      WorkKind::FeatureRequest,
      FlowNodeKind::Reasoning,
      BudgetUsage::default(),
      BudgetLimit::new(1, 1000, 100, 1000, 3000).unwrap(),
    )
    .await;
    let subject = FactoryTaskSubject::Exact(intent.input().subject().clone());
    let fragment = artifact(90);
    let range = FactoryRepositoryRange::new(
      intent.input().subject().repository_id(),
      intent.input().subject().base_revision().clone(),
      FactoryRepositoryPath::new("src/lib.rs").unwrap(),
      1,
      2,
    )
    .unwrap();
    let receipt = RetrievalReceipt::new(
      RetrievalReceiptId::generate(),
      subject.clone(),
      intent.input().subject().base_revision().clone(),
      reference("index"),
      reference("embedding"),
      FactorySafeText::new("declared API contract").unwrap(),
      reference("retrieval-policy"),
      vec![RepositoryFragment::new(1, range, fragment.clone()).unwrap()],
      digest(91),
    )
    .unwrap();
    let mut entries = intent.input().context().entries().to_vec();
    entries.push(
      ContextManifestEntry::new(
        ContextSourceKind::RepositoryFragment,
        key("repository.source"),
        subject.clone(),
        FactoryContextReference::repository_fragment(receipt.fragment_reference(1).unwrap()),
        fragment.content_digest(),
        fragment.encoded_size(),
        FactorySafeText::new("Declared source evidence").unwrap(),
        digest(92),
      )
      .unwrap(),
    );
    let context = ContextManifest::new(
      ContextManifestId::generate(),
      subject,
      intent.input().context().construction_policy_digest(),
      entries,
    )
    .unwrap();
    let mut details = intent.input().details().clone();
    if let ResearchDetails::Feature { sources, .. } = &mut details {
      sources.push(ResearchSourceReference {
        source_kind: ContextSourceKind::RepositoryFragment,
        logical_identity: key("repository.source"),
        content_digest: fragment.content_digest(),
      });
    }
    assert!(
      ResearchInput::new(
        &snapshot.work,
        &snapshot.admitted_flow,
        intent.input().accepted_triage().clone(),
        context.clone(),
        vec![],
        details.clone(),
        policy.digest().unwrap()
      )
      .is_err(),
      "repository sources require retained discovery receipts"
    );
    let input = ResearchInput::new(
      &snapshot.work,
      &snapshot.admitted_flow,
      intent.input().accepted_triage().clone(),
      context,
      vec![receipt.clone()],
      details,
      policy.digest().unwrap(),
    )
    .unwrap();
    let without_source = ContextManifest::new(
      ContextManifestId::generate(),
      input.context().subject().clone(),
      input.context().construction_policy_digest(),
      input
        .context()
        .entries()
        .iter()
        .filter(|entry| entry.logical_identity().as_str() != "input.6")
        .cloned()
        .collect(),
    )
    .unwrap();
    assert!(
      ResearchInput::new(
        &snapshot.work,
        &snapshot.admitted_flow,
        input.accepted_triage().clone(),
        without_source,
        input.retrieval().to_vec(),
        input.details().clone(),
        policy.digest().unwrap()
      )
      .is_err(),
      "sources absent from the frozen manifest cannot create a research intent"
    );
    let intent = with_research_input(&intent, &policy, input);
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let mut outputs = feature_outputs(&intent, &policy, &builds);
    outputs.fresh_until = time(800);
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs.clone()))));
    let FactoryResearchStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("feature completion required")
    };
    let stored = serde_json::to_vec(&intent).unwrap();
    let restored = ResearchBuildIntent::restore(
      &stored,
      &snapshot.work,
      &snapshot.admitted_flow,
      &policy,
      intent.flow(),
      intent.node(),
    )
    .unwrap();
    assert_eq!(restored.input().retrieval(), &[receipt]);
    for change in ["source", "retrieval", "missing-bounds"] {
      let mut wire = serde_json::to_value(&intent).unwrap();
      match change {
        "source" => {
          wire["input"]["details"]["sources"][0]["content_digest"] = serde_json::to_value(digest(99)).unwrap()
        }
        "retrieval" => {
          wire["input"]["retrieval"][0]["embedding"] = serde_json::to_value(reference("changed-embedding")).unwrap()
        }
        "missing-bounds" => {
          wire.as_object_mut().unwrap().remove("budget");
        }
        _ => unreachable!(),
      }
      assert!(
        ResearchBuildIntent::restore(
          &serde_json::to_vec(&wire).unwrap(),
          &snapshot.work,
          &snapshot.admitted_flow,
          &policy,
          intent.flow(),
          intent.node()
        )
        .is_err(),
        "{change}"
      );
    }
    let mut changed_entries = intent.input().context().entries().to_vec();
    let entry = changed_entries
      .iter_mut()
      .find(|entry| entry.logical_identity().as_str() == "input.6")
      .unwrap();
    let changed_source = artifact(99);
    *entry = ContextManifestEntry::new(
      entry.source_kind(),
      entry.logical_identity().clone(),
      entry.subject().clone(),
      FactoryContextReference::Artifact(changed_source.clone()),
      changed_source.content_digest(),
      changed_source.encoded_size(),
      entry.inclusion_reason().clone(),
      entry.provenance_digest(),
    )
    .unwrap();
    let context = ContextManifest::new(
      ContextManifestId::generate(),
      intent.input().context().subject().clone(),
      intent.input().context().construction_policy_digest(),
      changed_entries,
    )
    .unwrap();
    let mut details = intent.input().details().clone();
    if let ResearchDetails::Feature { sources, .. } = &mut details {
      sources
        .iter_mut()
        .find(|source| source.logical_identity.as_str() == "input.6")
        .unwrap()
        .content_digest = changed_source.content_digest();
    }
    let changed_input = ResearchInput::new(
      &snapshot.work,
      &snapshot.admitted_flow,
      intent.input().accepted_triage().clone(),
      context,
      intent.input().retrieval().to_vec(),
      details,
      policy.digest().unwrap(),
    )
    .unwrap();
    let mut substituted = serde_json::to_value(&intent).unwrap();
    substituted["input"] = serde_json::to_value(changed_input).unwrap();
    assert!(
      ResearchBuildIntent::restore(
        &serde_json::to_vec(&substituted).unwrap(),
        &snapshot.work,
        &snapshot.admitted_flow,
        &policy,
        intent.flow(),
        intent.node()
      )
      .is_err(),
      "even coherent new discovery cannot replace the dispatched input"
    );
    let takeover = FactoryClaimOwnership::new(
      key("replacement.worker"),
      FactoryClaim::new(FactoryClaimFence::new(digest(91)), time(100), time(200)).unwrap(),
    );
    let restarted = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
    let FactoryResearchStep::Completed(replayed) = restarted
      .observe_or_dispatch(&restored, &policy, takeover.clone(), time(101))
      .await
      .unwrap()
    else {
      panic!("retained completion required")
    };
    assert_eq!(done.result, replayed.result);
    assert_eq!(done.evidence, replayed.evidence);
    assert_eq!(done.acceptance, replayed.acceptance);
    assert_eq!(replayed.completion.claim(), takeover.claim());
    assert_eq!(
      done.result.provenance().context_digest,
      intent.input().context().digest().unwrap()
    );
    let requests = builds.requests.lock().unwrap();
    assert_eq!(requests[0], requests[1]);
    let dispatched = ResearchInput::restore(
      &requests[0].causality.node().unwrap().input,
      &snapshot.work,
      &snapshot.admitted_flow,
      policy.digest().unwrap(),
    )
    .unwrap();
    assert_eq!(dispatched.retrieval(), intent.input().retrieval());
  });
}
