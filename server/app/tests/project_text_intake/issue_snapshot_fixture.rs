//! Admission is the commit boundary, including edits after immutable byte publication.
use super::*;
use tokio::sync::Notify;

pub(super) struct PausedBytes {
  pub(super) bytes: Bytes,
  pub(super) reached: Notify,
  pub(super) resume: Notify,
}
impl PausedBytes {
  pub(super) fn new() -> Self {
    Self {
      bytes: Bytes::new(),
      reached: Notify::new(),
      resume: Notify::new(),
    }
  }
}
#[async_trait::async_trait]
impl ArtifactBytePublisher for PausedBytes {
  async fn publish_verified(&self, object: &ArtifactObject, bytes: Vec<u8>) -> Result<(), ArtifactStoreError> {
    self.bytes.publish_verified(object, bytes).await?;
    if self.bytes.writes.load(Ordering::SeqCst) == 2 {
      self.reached.notify_one();
      self.resume.notified().await;
    }
    Ok(())
  }
}
#[async_trait::async_trait]
impl ArtifactByteReader for PausedBytes {
  async fn read_published(&self, object: &ArtifactObject, limit: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    self.bytes.read_published(object, limit).await
  }
}

pub(super) fn snapshot_flow() -> FactoryFlowConfiguration {
  snapshot_flow_with_fields([("title", FlowIssueFieldSource::Title)])
}

/// These replay fixtures consume built-in string/choice fields only.
pub(super) fn snapshot_flow_with_fields<const N: usize>(
  fields: [(&str, FlowIssueFieldSource); N],
) -> FactoryFlowConfiguration {
  let schema = FlowDataSchema::new(
    "issue.snapshot".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::Object {
      fields: fields
        .iter()
        .map(|(name, _)| {
          (
            name.parse().unwrap(),
            FlowFieldSchema::required(FlowValueSchema::String {
              min_bytes: 1,
              max_bytes: 256,
            }),
          )
        })
        .collect(),
    },
  )
  .unwrap();
  let input = FlowIssueSnapshotInput {
    admission: None,
    version: 1,
    schema: schema.reference().clone(),
    definition_version: 0,
    fields: fields
      .into_iter()
      .map(|(name, source)| {
        (
          name.parse().unwrap(),
          FlowIssueFieldSelection { source, required: true },
        )
      })
      .collect(),
    materials: Default::default(),
    evidence: Default::default(),
  };
  snapshot_flow_with_input(input, schema)
}

/// Shares graph construction for scalar-only and explicit material input contracts.
pub(super) fn snapshot_flow_with_input(
  input: FlowIssueSnapshotInput,
  schema: FlowDataSchema,
) -> FactoryFlowConfiguration {
  let fixture = postgres::factory_configuration::fixture(ProjectId::generate());
  let flow = fixture.draft.flow.unwrap();
  let mut schemas = flow.data_schemas.clone();
  schemas.push(schema.clone());
  let root = flow.closure.definition(flow.closure.root()).unwrap();
  let mut nodes = root.nodes().to_vec();
  let entry = nodes.iter().position(|node| node.key() == root.entry()).unwrap();
  nodes[entry] = nodes[entry]
    .clone()
    .with_input_binding(
      FlowInputBinding::new(
        [(
          "issue".parse().unwrap(),
          FlowInputSource::Incoming {
            schema: schema.reference().clone(),
            path: String::new(),
          },
        )]
        .into(),
        FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!(true),
        })
        .unwrap(),
      )
      .unwrap(),
    )
    .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes,
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let closure = PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap();
  FactoryFlowConfiguration::new(closure, flow.limits, schemas, flow.pools)
    .unwrap()
    .with_issue_snapshot(input)
    .unwrap()
}

pub(super) async fn snapshot_request(
  store: Arc<PostgresStore>,
  issue: &IssueRevision,
  submission: &ProjectTextSubmission,
) -> IssueExecutionRequest {
  snapshot_request_with_materials(store, issue, submission, None).await
}

pub(super) async fn snapshot_request_with_materials(
  store: Arc<PostgresStore>,
  issue: &IssueRevision,
  submission: &ProjectTextSubmission,
  readers: Option<Arc<IssueSnapshotMaterialReaders>>,
) -> IssueExecutionRequest {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let handler = IssueExecutionHandlers::new(store.clone(), store, policy.clone());
  let handler = match readers {
    Some(readers) => handler.with_materials(readers),
    None => handler,
  };
  let requests = AuthorizedCommandHandler::new(policy, Arc::new(handler));
  requests
    .handle_command(
      &context(),
      RequestIssueExecutionCommand {
        intent: RequestIssueExecutionIntent {
          expected_snapshot_digest: None,
          project_id: issue.project_id,
          issue_id: issue.id,
          expected_version: issue.version,
          target: IssueExecutionTarget {
            predecessor_run: None,
            configuration_id: submission.intent.target.configuration_id,
            configuration_version: submission.intent.target.configuration_version,
            repository_id: submission.intent.target.repository_id.unwrap(),
            repository_version: submission.intent.target.repository_version.unwrap(),
            source: submission.intent.source.clone(),
            mapping: IssueExecutionMapping::TitleDescriptionAcceptanceV1,
          },
          classification: Some(submission.intent.classification.clone()),
        },
        idempotency_key: key(&format!("snapshot-review.{}", issue.id)),
        requested_at: at(10),
      },
    )
    .await
    .unwrap()
    .request
}

pub(super) async fn prepared_admission(
  store: &PostgresStore,
  preparation: &ProjectTextPreparation,
  documents: &ProjectTextDocuments,
) -> AdmitFactoryWork {
  let intent = preparation.admission_intent().unwrap();
  let selected = store
    .factory_admission_context(preparation.intent.target)
    .await
    .unwrap();
  let configuration = &selected.configuration.configuration;
  let work = WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    configuration.reference().clone(),
    intent.external_identity.clone(),
    ExactSubject::new(
      intent.project_id,
      intent.repository_id.unwrap(),
      preparation.frozen_revision.clone().unwrap(),
    ),
    intent.artifacts.clone(),
    intent.classification.clone(),
  )
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = AdmittedFlow::from_configuration(configuration, &run)
    .unwrap()
    .with_access_profile(
      configuration,
      selected.project_policies.as_ref().unwrap().access_profile.clone(),
    )
    .unwrap();
  let snapshot = documents.issue_snapshot().unwrap();
  let incoming = snapshot
    .incoming(&work, &flow, flow.data_schema(&snapshot.input.schema).unwrap())
    .unwrap();
  AdmitFactoryWork {
    probe: FactoryAdmissionProbe {
      source_scope: FactoryWorkSourceScope {
        source: "manual".parse().unwrap(),
        security_scope: ManagementSecurityScope::trusted_network(),
      },
      external_identity: intent.external_identity.clone(),
      idempotency_key: intent.idempotency_key.clone(),
      intent_digest: factory_admission_intent_digest(&intent, "octacity.factory.manual-admission.v1"),
    },
    repository_version: intent.repository_version,
    work,
    run,
    flow,
    incoming: Some(incoming),
    admitted_at: intent.admitted_at,
  }
}
