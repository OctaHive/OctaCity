//! Exact retained input ownership through the public ordinary Artifact/Build ports.
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_application::{
  ApplicationError, FactoryArtifactFlowInputs, FactoryArtifactReader, FactoryExecutionClock, FactoryFlowInputPreparer,
  FactoryNodeContextSource, FactoryNodeTarget, ReadFactoryArtifact,
};
use octacity_server_domain::*;
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest};
use octacity_server_store::testing::{
  ArtifactLeaseFixture, InMemoryArtifactRecordStore, authoritative_store_contract_fixture,
};
use octacity_server_store::*;
use std::{
  future::Future,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

struct Builds(BuildRecord);
#[async_trait]
impl BuildQueryStore for Builds {
  async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
    assert_eq!(id, self.0.build.id);
    Ok(self.0.clone())
  }
  async fn attempt(&self, _: AttemptId) -> Result<AttemptRecord, StoreError> {
    unreachable!("input reads do not reconstruct execution")
  }
  async fn job(&self, _: JobId) -> Result<JobRecord, StoreError> {
    unreachable!("publication already binds the producing Job")
  }
  async fn latest_attempt(&self, _: BuildId) -> Result<AttemptRecord, StoreError> {
    unreachable!("retries cannot change published bytes")
  }
}

struct Bytes(AtomicUsize, Vec<u8>, &'static str);
#[async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    assert_eq!(object.sha256(), FactoryDigest::content_sha256(&self.1).to_string());
    assert_eq!(object.size_bytes(), self.1.len() as u64);
    assert_eq!(
      object.content_type(),
      self.2,
      "object reads use actual upload transport metadata"
    );
    assert!((5..=octacity_artifact_store::MAX_ARTIFACT_READ_BYTES).contains(&maximum));
    self.0.fetch_add(1, Ordering::SeqCst);
    Ok(self.1.clone())
  }
}
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
fn ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory contract yielded"),
  }
}
#[test]
fn factory_reads_only_exact_published_project_owned_inputs_within_the_declared_ceiling() {
  ready(async {
    let (reader, request, bytes) = prepared_reader(None).await;
    let accepted = reader.read(request.clone()).await.unwrap();
    assert_eq!(accepted.bytes(), b"hello");
    assert_eq!(accepted.record().identity().artifact_id, request.artifact.artifact_id());
    for denied in [
      ReadFactoryArtifact {
        project_id: ProjectId::generate(),
        ..request.clone()
      },
      ReadFactoryArtifact {
        maximum_bytes: 4,
        ..request.clone()
      },
      ReadFactoryArtifact {
        observed_at: at(101),
        ..request.clone()
      },
      ReadFactoryArtifact {
        artifact: FactoryArtifactReference::new(request.artifact.artifact_id(), FactoryDigest::from_bytes([8; 32]), 5)
          .unwrap(),
        ..request.clone()
      },
    ] {
      assert!(reader.read(denied).await.is_err());
    }
    assert_eq!(
      bytes.0.load(Ordering::SeqCst),
      1,
      "rejected selection must not read protected bytes"
    );
  });
}

type Reader = FactoryArtifactReader<InMemoryArtifactRecordStore, Builds, Bytes>;
async fn prepared_reader(project: Option<ProjectId>) -> (Reader, ReadFactoryArtifact, Arc<Bytes>) {
  prepared_reader_with_media(project, "text/plain", "text/plain", b"hello").await
}
async fn prepared_reader_with_media(
  project: Option<ProjectId>,
  media_type: &'static str,
  transport_media_type: &'static str,
  body: &[u8],
) -> (Reader, ReadFactoryArtifact, Arc<Bytes>) {
  let mut fixture = authoritative_store_contract_fixture();
  if let Some(project) = project {
    fixture.request.build.project_id = project;
  }
  let builds = Arc::new(Builds(BuildRecord {
    build: fixture.request.build.clone(),
    state: octacity_server_orchestrator::BuildState::Queued,
    version: BuildVersion::new(1).unwrap(),
    origin: BuildOrigin::Trigger(Box::new(TriggerBuildOrigin {
      occurrence: fixture.request.trigger.clone(),
      state: octacity_server_trigger::TriggerOccurrenceState::Accepted,
      created_at: at(100),
      updated_at: at(100),
    })),
    created_at: at(100),
    updated_at: at(100),
  }));
  let lease = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([7; 32]),
    agent_id: fixture.agent_id,
    registration_epoch: fixture.registration_epoch,
  };
  let job_id = fixture.request.jobs[0].id;
  let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access: lease,
    build_id: fixture.request.build.id,
    attempt_id: fixture.request.attempt_id,
    attempt: AttemptNumber::new(1).unwrap(),
    job_id,
    expires_at: at(10_000),
  }));
  let digest = ArtifactContentDigest::from_lower_hex(&FactoryDigest::content_sha256(body).to_string()).unwrap();
  let artifact_id = ArtifactId::generate();
  let uploaded = artifacts
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id,
      upload_id: ArtifactUploadId::generate(),
      idempotency_key: IdempotencyKey::new("immutable-input").unwrap(),
      lease,
      job_id,
      attempt: AttemptNumber::new(1).unwrap(),
      logical_name: ArtifactName::new("frozen-input").unwrap(),
      producer_run_id: 1,
      producer_task_id: 1,
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new(media_type).unwrap(),
      transport_media_type: ArtifactMediaType::new(transport_media_type).unwrap(),
      size_bytes: body.len() as u64,
      digest,
      reserved_at: at(101),
      capability_expires_at: at(1000),
    })
    .await
    .unwrap();
  let verification = VerifyArtifactUpload {
    upload_id: uploaded.upload.upload_id,
    lease,
    job_id,
    attempt: AttemptNumber::new(1).unwrap(),
    observed_at: at(102),
  };
  artifacts.begin_artifact_verification(verification).await.unwrap();
  artifacts
    .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
  let bytes = Arc::new(Bytes(AtomicUsize::new(0), body.to_vec(), transport_media_type));
  let reader = FactoryArtifactReader::new(artifacts, builds, bytes.clone());
  let request = ReadFactoryArtifact {
    project_id: fixture.request.build.project_id,
    artifact: FactoryArtifactReference::new(
      artifact_id,
      FactoryDigest::from_bytes(digest.as_bytes()),
      body.len() as u64,
    )
    .unwrap(),
    maximum_bytes: 8,
    observed_at: at(103),
    required_until: at(1000),
  };
  (reader, request, bytes)
}

#[test]
fn configured_context_uses_only_declared_verified_artifacts_for_arbitrary_nodes() {
  use octacity_server_factory::*;
  use octacity_server_store::testing::{
    InMemoryFactoryConfigurationStore, factory_node_journal_contract_fixture, management_mutation_with_request,
  };
  use std::collections::BTreeMap;
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(103))
    }
  }
  ready(async {
    let fixture = factory_node_journal_contract_fixture();
    let (reader, request, bytes) = prepared_reader(Some(fixture.admission.work.subject().project_id())).await;
    let original = fixture
      .admission
      .flow
      .closure()
      .definition(fixture.admission.flow.closure().root())
      .unwrap();
    let alias = FactoryKey::new("selected-document-42").unwrap();
    let context = FlowContextBinding::new(
      None,
      BTreeMap::from([(
        alias.clone(),
        FlowContextSelection {
          when_present: None,
          source_kind: ContextSourceKind::Task,
          source: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::to_value(FactoryContextReference::Artifact(request.artifact.clone())).unwrap(),
          })
          .unwrap(),
        },
      )]),
    )
    .unwrap();
    let node = original.nodes()[0]
      .clone()
      .with_input_binding(
        FlowInputBinding::new(
          BTreeMap::new(),
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!({"count":3}),
          })
          .unwrap(),
        )
        .unwrap(),
      )
      .unwrap()
      .with_context_binding(context)
      .unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: original.reference().id(),
      version: original.reference().version(),
      input_schema: original.input_schema().cloned(),
      entry: node.key().clone(),
      nodes: vec![node],
      transitions: original.transitions().to_vec(),
      terminals: original.terminals().to_vec(),
      context_projections: vec![],
      data_projections: vec![],
      execution: original.execution().clone(),
    })
    .unwrap();
    let flow = FlowRun::root(&fixture.admission.run, definition.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let admitted = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      fixture.admission.flow.limits().clone(),
      flow,
      cycle,
      fixture.admission.flow.wip_limits(),
    )
    .unwrap()
    .with_data_schemas(vec![fixture.schema.clone()])
    .unwrap();
    let store = InMemoryFactoryConfigurationStore::new();
    store
      .seed_factory_run(
        PublishedFactoryAdmission {
          flow: admitted,
          ..fixture.admission.clone()
        },
        FactoryDigest::from_bytes([1; 32]),
        management_mutation_with_request((), "context-owner").audit(),
      )
      .unwrap();
    let snapshot = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    let reader = Arc::new(reader);
    let inputs = FactoryArtifactFlowInputs::new(reader.clone(), Arc::new(Clock));
    let target = FactoryNodeTarget {
      flow_run_id: snapshot.admitted_flow.root_run().id(),
      cycle_id: snapshot.admitted_flow.initial_cycle().id(),
      node: original.entry().clone(),
    };
    let prepared = inputs.prepare(&snapshot, &target).await.unwrap();
    assert_eq!(
      prepared,
      inputs.prepare(&snapshot, &target).await.unwrap(),
      "the same immutable selection and construction policy must produce identical manifest bytes"
    );
    assert_eq!(prepared.payload().value(), &serde_json::json!({"count":3}));
    let manifest = prepared.context().unwrap();
    assert_eq!(
      manifest.subject(),
      &FactoryTaskSubject::Exact(snapshot.work.subject().clone())
    );
    assert_eq!(
      manifest.entries().len(),
      1,
      "ordering and Work contents must not add implicit context"
    );
    assert_eq!(manifest.entries()[0].logical_identity(), &alias);
    assert_eq!(
      manifest.entries()[0].source(),
      &FactoryContextReference::Artifact(request.artifact.clone())
    );
    assert_eq!(bytes.0.load(Ordering::SeqCst), 2);
    let context_bytes = reader.read_context(&prepared, at(103), at(1103)).await.unwrap();
    assert_eq!(context_bytes.len(), 1);
    assert_eq!(
      context_bytes.get(&request.artifact.artifact_id()),
      Some(&b"hello".to_vec())
    );
    assert_eq!(bytes.0.load(Ordering::SeqCst), 3);
    assert!(reader.read_context(&prepared, at(103), at(103)).await.is_err());
    assert_eq!(bytes.0.load(Ordering::SeqCst), 3, "expired context must not read bytes");
  });
}

#[test]
fn candidate_context_requires_selected_verified_publication_ancestry() {
  use octacity_server_application::*;
  use octacity_server_factory::*;
  use octacity_server_store::testing::{
    InMemoryFactoryConfigurationStore, factory_node_journal_contract_fixture, management_mutation_with_request,
  };
  use std::collections::BTreeMap;
  struct Clock;
  impl FactoryExecutionClock for Clock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(103))
    }
  }
  struct Publication(CandidateSubject);
  #[async_trait]
  impl FactoryNodeExecutor for Publication {
    async fn cancel(
      &self,
      _: FactoryNodeExecutionRequest<'_>,
      _: Timestamp,
    ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
      unreachable!()
    }
    async fn observe(
      &self,
      request: FactoryNodeExecutionRequest<'_>,
    ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
      let definition = request
        .admitted
        .closure()
        .definition(request.input.definition())
        .unwrap();
      let outcome = &definition.node(request.input.node()).unwrap().outcomes()[0];
      let record = FlowNodeRecord::new(
        request.input,
        request.attempt,
        definition,
        FlowRecordObservation {
          outcome: outcome.key().clone(),
          payload: FlowPayload::new(
            request.admitted.data_schema(outcome.schema()).unwrap(),
            serde_json::json!({"count":3}),
          )
          .unwrap(),
          producer: request.attempt.execution().clone(),
          observed_at: request.observed_at,
        },
      )
      .unwrap()
      .with_discovery(FlowDiscoveryFacts::candidate(self.0.clone()))
      .unwrap();
      Ok(FactoryNodeExecutionStep::Completed {
        execution: None,
        record: Box::new(record),
        usage: BudgetUsage::default(),
        outbox: vec![],
      })
    }
  }
  ready(async {
    for (selected_ancestry, through_gate, through_call) in [
      (true, false, false),
      (true, true, false),
      (true, false, true),
      (false, false, false),
    ] {
      let fixture = factory_node_journal_contract_fixture();
      let (reader, request, bytes) = prepared_reader(Some(fixture.admission.work.subject().project_id())).await;
      let original = fixture
        .admission
        .flow
        .closure()
        .definition(fixture.admission.flow.closure().root())
        .unwrap();
      let candidate = CandidateSubject::new(
        fixture.admission.work.subject().clone(),
        ImmutableRevision::new("exact-candidate-r7").unwrap(),
        FactoryDigest::from_bytes([29; 32]),
      );
      let capability = ImmutableReference::new(
        FactoryKey::new("installed.publisher").unwrap(),
        FactoryKey::new("v7").unwrap(),
        FactoryDigest::from_bytes([3; 32]),
      );
      let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
        plugins: vec![capability.clone()],
        ..Default::default()
      })
      .unwrap();
      let mapping = FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!({"count":3}),
      })
      .unwrap();
      let context_binding = |candidate_revision| {
        FlowContextBinding::new(
          candidate_revision,
          BTreeMap::from([(
            FactoryKey::new("closed-input").unwrap(),
            FlowContextSelection {
              when_present: None,
              source_kind: ContextSourceKind::Task,
              source: FlowDataMapping::new(FlowValueMapping::Constant {
                value: serde_json::to_value(FactoryContextReference::Artifact(request.artifact.clone())).unwrap(),
              })
              .unwrap(),
            },
          )]),
        )
        .unwrap()
      };
      let producer = FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: FactoryKey::new("unfamiliar-step").unwrap(),
        kind: FlowNodeKind::TrustedAction,
        input_schema: Some(fixture.schema.reference().clone()),
        outcomes: original.nodes()[0].outcomes().to_vec(),
        budget: original.nodes()[0].budget(),
        permissions: permissions.clone(),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_action(
        FlowActionBinding::new(
          capability.clone(),
          FactoryKey::new("publish").unwrap(),
          serde_json::json!({}),
        )
        .unwrap(),
      )
      .unwrap()
      .with_input_binding(FlowInputBinding::new(BTreeMap::new(), mapping.clone()).unwrap())
      .unwrap()
      .with_context_binding(context_binding(None))
      .unwrap();
      let binding = if selected_ancestry {
        FlowInputBinding::new(
          BTreeMap::from([(
            FactoryKey::new("prior").unwrap(),
            FlowInputSource::Predecessor {
              view: FlowRecordView::Payload,
              path: String::new(),
            },
          )]),
          FlowDataMapping::new(FlowValueMapping::Pointer {
            path: "/prior".to_owned(),
          })
          .unwrap(),
        )
        .unwrap()
      } else {
        FlowInputBinding::new(BTreeMap::new(), mapping).unwrap()
      };
      let consumer = original.nodes()[0]
        .clone()
        .with_input_binding(binding)
        .unwrap()
        .with_context_binding(context_binding(Some(
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!(candidate.candidate_revision()),
          })
          .unwrap(),
        )))
        .unwrap();
      let gate = FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: FactoryKey::new("policy-check-91").unwrap(),
        kind: FlowNodeKind::DeterministicGate,
        input_schema: Some(fixture.schema.reference().clone()),
        outcomes: original.nodes()[0].outcomes().to_vec(),
        budget: original.nodes()[0].budget(),
        permissions: FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_input_binding(consumer.input_binding().unwrap().clone())
      .unwrap()
      .with_gate(
        FlowGateProgram::new(vec![], original.nodes()[0].outcomes()[0].key().clone())
          .unwrap()
          .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
          .unwrap()
          .binding()
          .unwrap(),
      )
      .unwrap();
      let mut nodes = vec![producer.clone(), consumer.clone()];
      let mut transitions = vec![];
      if through_gate {
        nodes.push(gate.clone());
        transitions.push(FlowTransition::new(
          gate.key().clone(),
          gate.outcomes()[0].key().clone(),
          FlowTransitionTarget::Node(consumer.key().clone()),
        ));
      }
      transitions.extend([
        FlowTransition::new(
          producer.key().clone(),
          producer.outcomes()[0].key().clone(),
          FlowTransitionTarget::Node(if through_gate {
            gate.key().clone()
          } else {
            consumer.key().clone()
          }),
        ),
        FlowTransition::new(
          consumer.key().clone(),
          consumer.outcomes()[0].key().clone(),
          FlowTransitionTarget::Terminal(original.terminals()[0].key().clone()),
        ),
      ]);
      let child = if through_call {
        let child_node = consumer
          .clone()
          .with_input_binding(
            FlowInputBinding::new(
              BTreeMap::from([(
                FactoryKey::new("caller").unwrap(),
                FlowInputSource::CallerInput { path: String::new() },
              )]),
              FlowDataMapping::new(FlowValueMapping::Pointer {
                path: "/caller".to_owned(),
              })
              .unwrap(),
            )
            .unwrap(),
          )
          .unwrap();
        let child = FlowDefinition::new(FlowDefinitionInput {
          id: FlowDefinitionId::generate(),
          version: FlowDefinitionVersion::INITIAL,
          input_schema: Some(fixture.schema.reference().clone()),
          entry: child_node.key().clone(),
          nodes: vec![child_node.clone()],
          transitions: vec![FlowTransition::new(
            child_node.key().clone(),
            child_node.outcomes()[0].key().clone(),
            FlowTransitionTarget::Terminal(child_node.outcomes()[0].key().clone()),
          )],
          terminals: vec![FlowTerminalDefinition::new(
            child_node.outcomes()[0].key().clone(),
            child_node.outcomes()[0].schema().clone(),
          )],
          context_projections: vec![],
          data_projections: vec![],
          execution: FlowExecutionPolicy::new(consumer.budget(), consumer.permissions().clone(), 1).unwrap(),
        })
        .unwrap();
        let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
          key: FactoryKey::new("nested-operation-73").unwrap(),
          kind: FlowNodeKind::SubflowCall,
          input_schema: Some(fixture.schema.reference().clone()),
          outcomes: consumer.outcomes().to_vec(),
          budget: consumer.budget(),
          permissions: consumer.permissions().clone(),
          required: true,
          subflow: Some(child.reference()),
        })
        .unwrap()
        .with_input_binding(consumer.input_binding().unwrap().clone())
        .unwrap();
        nodes = vec![producer.clone(), call.clone()];
        transitions = vec![
          FlowTransition::new(
            producer.key().clone(),
            producer.outcomes()[0].key().clone(),
            FlowTransitionTarget::Node(call.key().clone()),
          ),
          FlowTransition::new(
            call.key().clone(),
            call.outcomes()[0].key().clone(),
            FlowTransitionTarget::Terminal(original.terminals()[0].key().clone()),
          ),
        ];
        Some(child)
      } else {
        None
      };
      let mut limits = serde_json::to_value(fixture.admission.flow.limits()).unwrap();
      limits["permissions"] = serde_json::to_value(&permissions).unwrap();
      let definition = FlowDefinition::new(FlowDefinitionInput {
        id: original.reference().id(),
        version: original.reference().version(),
        input_schema: original.input_schema().cloned(),
        entry: producer.key().clone(),
        nodes,
        transitions,
        terminals: original.terminals().to_vec(),
        context_projections: vec![],
        data_projections: vec![],
        execution: FlowExecutionPolicy::new(fixture.admission.flow.limits().budget(), permissions, 1).unwrap(),
      })
      .unwrap();
      let flow = FlowRun::root(&fixture.admission.run, definition.reference()).unwrap();
      let cycle = WorkflowCycle::initial(&flow).unwrap();
      let admitted = AdmittedFlow::new(
        PinnedFlowDefinitionClosure::new(
          definition.reference(),
          std::iter::once(definition).chain(child).collect(),
        )
        .unwrap(),
        serde_json::from_value(limits).unwrap(),
        flow,
        cycle,
        fixture.admission.flow.wip_limits(),
      )
      .unwrap()
      .with_data_schemas(vec![fixture.schema.clone()])
      .unwrap();
      let store = Arc::new(InMemoryFactoryConfigurationStore::new());
      store
        .seed_factory_run(
          PublishedFactoryAdmission {
            flow: admitted,
            ..fixture.admission.clone()
          },
          FactoryDigest::from_bytes([1; 32]),
          management_mutation_with_request((), "candidate-context").audit(),
        )
        .unwrap();
      let run = fixture.admission.run.id();
      let claim = FactoryRunClaimRecord::new(
        run,
        FactoryKey::new("context-owner").unwrap(),
        FactoryClaim::new(
          FactoryClaimFence::new(FactoryDigest::from_bytes([4; 32])),
          at(100),
          at(3000),
        )
        .unwrap(),
      );
      store
        .claim_factory_run(ClaimFactoryRun {
          run_id: run,
          expected_version: fixture.admission.run.version(),
          record: claim.clone(),
          audit: FactoryAuditFact::new(
            run,
            AuditActorKind::Worker,
            None,
            FactoryKey::new("claimed").unwrap(),
            FactoryDigest::from_bytes([4; 32]),
            FactoryKey::new("accepted").unwrap(),
            at(100),
          ),
        })
        .await
        .unwrap();
      let inputs = FactoryArtifactFlowInputs::new(Arc::new(reader), Arc::new(Clock));
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      let target = FactoryNodeTarget {
        flow_run_id: snapshot.admitted_flow.root_run().id(),
        cycle_id: snapshot.admitted_flow.initial_cycle().id(),
        node: producer.key().clone(),
      };
      let prepared = inputs.prepare(&snapshot, &target).await.unwrap();
      let runner = FactoryNodeRunner::new(store.clone(), Arc::new(Publication(candidate.clone())), Arc::new(Clock));
      runner
        .run_once(run, claim.id, target.clone(), Some(prepared), at(103))
        .await
        .unwrap();
      runner.run_once(run, claim.id, target, None, at(104)).await.unwrap();
      if through_call {
        let flow = FactoryFlowRunner::new(
          store.clone(),
          Arc::new(Publication(candidate.clone())),
          Arc::new(inputs),
          Arc::new(Clock),
        );
        let mut prepared = None;
        for observed in 105..115 {
          flow.reconcile_flow(run, claim.id, at(observed)).await.unwrap();
          let snapshot = store.factory_run_snapshot(run).await.unwrap();
          prepared = snapshot
            .flow
            .data
            .inputs
            .iter()
            .find(|input| input.node() == consumer.key())
            .cloned();
          if prepared.is_some() {
            break;
          }
        }
        let prepared = prepared.expect("nested selected input must be prepared within bounded passes");
        assert!(prepared.sources().is_empty());
        assert!(prepared.caller_input_digest().is_some());
        assert_eq!(
          prepared.context().unwrap().subject(),
          &FactoryTaskSubject::Candidate(candidate)
        );
        assert_eq!(bytes.0.load(Ordering::SeqCst), 2);
        continue;
      }
      if through_gate {
        let snapshot = store.factory_run_snapshot(run).await.unwrap();
        let target = FactoryNodeTarget {
          flow_run_id: snapshot.admitted_flow.root_run().id(),
          cycle_id: snapshot.admitted_flow.initial_cycle().id(),
          node: gate.key().clone(),
        };
        let prepared = inputs.prepare(&snapshot, &target).await.unwrap();
        runner
          .run_once(run, claim.id, target.clone(), Some(prepared), at(105))
          .await
          .unwrap();
        runner.run_once(run, claim.id, target, None, at(106)).await.unwrap();
      }
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      let target = FactoryNodeTarget {
        flow_run_id: snapshot.admitted_flow.root_run().id(),
        cycle_id: snapshot.admitted_flow.initial_cycle().id(),
        node: consumer.key().clone(),
      };
      let prepared = inputs.prepare(&snapshot, &target).await;
      if selected_ancestry {
        assert_eq!(
          prepared.unwrap().context().unwrap().subject(),
          &FactoryTaskSubject::Candidate(candidate)
        );
        assert_eq!(bytes.0.load(Ordering::SeqCst), 2);
      } else {
        assert!(
          prepared.is_err(),
          "a control edge cannot grant unselected candidate ancestry"
        );
        assert_eq!(bytes.0.load(Ordering::SeqCst), 1);
      }
    }
  });
}

#[test]
fn factory_reads_published_text_with_its_logical_media_type_and_binary_upload_transport() {
  ready(async {
    // The released Agent sends regular files unchanged with binary transport,
    // independently of their logical JSON or trace content type.
    for media in ["application/json", "application/x-ndjson"] {
      let (reader, request, bytes) =
        prepared_reader_with_media(None, media, "application/octet-stream", b"true\n").await;
      let accepted = reader
        .read(request.clone())
        .await
        .expect("published raw file bytes retain their declared logical media type");
      assert_eq!(accepted.bytes(), b"true\n");
      assert_eq!(accepted.record().identity().media_type.as_str(), media);
      let record = reader.read_record(request.clone()).await.unwrap();
      assert_eq!(record, *accepted.record());
      for denied in [
        ReadFactoryArtifact {
          project_id: ProjectId::generate(),
          ..request.clone()
        },
        ReadFactoryArtifact {
          maximum_bytes: 4,
          ..request.clone()
        },
        ReadFactoryArtifact {
          observed_at: at(101),
          ..request.clone()
        },
        ReadFactoryArtifact {
          required_until: request.observed_at,
          ..request.clone()
        },
        ReadFactoryArtifact {
          artifact: FactoryArtifactReference::new(
            request.artifact.artifact_id(),
            FactoryDigest::from_bytes([8; 32]),
            5,
          )
          .unwrap(),
          ..request.clone()
        },
      ] {
        assert!(reader.read(denied).await.is_err());
      }
      assert_eq!(
        bytes.0.load(Ordering::SeqCst),
        1,
        "binary transport does not bypass the immutable selection and ownership gates"
      );
    }
  });
}

#[test]
fn factory_rejects_archive_transport_conversions_before_reading_selected_text() {
  ready(async {
    for transport in ["application/vnd.octacity.directory.tar.v1", "application/gzip"] {
      let (reader, request, bytes) = prepared_reader_with_media(None, "application/json", transport, b"true\n").await;
      assert!(reader.read(request.clone()).await.is_err());
      assert!(reader.read_record(request).await.is_err());
      assert_eq!(
        bytes.0.load(Ordering::SeqCst),
        0,
        "unsupported transport conversions never read or unpack protected bytes"
      );
    }
  });
}
