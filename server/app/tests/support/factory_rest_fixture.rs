//! Deterministic installed execution boundaries; management and the graph owner stay real.
use async_trait::async_trait;
use octacity_server_application::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_store_postgres::{FactoryCapabilitySource, PostgresStore};
use serde_json::{Value, json};
use std::sync::Arc;

fn key(text: &str) -> FactoryKey {
  text.parse().unwrap()
}
fn reference(name: &str, byte: u8) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([byte; 32]))
}
struct Inputs;
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    FactoryNodeRunner::<PostgresStore, Observer>::prepare_input(snapshot, target)
  }
}
struct Observer {
  evidence: bool,
  value: Value,
}
#[async_trait]
impl FactoryNodeExecutor for Observer {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    Ok(FactoryNodeCancellationStep::Stopped {
      execution: None,
      usage: BudgetUsage::default(),
    })
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
    let node = definition.node(request.input.node()).unwrap();
    assert_eq!(node.outcomes().len(), 1);
    let outcome = &node.outcomes()[0];
    let mut value = self.value.clone();
    if let Some(input_digest) = value.get_mut("input_digest") {
      *input_digest = json!(request.input.digest().unwrap());
    }
    let payload = FlowPayload::new(request.admitted.data_schema(outcome.schema()).unwrap(), value).unwrap();
    let record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: outcome.key().clone(),
        payload,
        producer: request.attempt.execution().clone(),
        observed_at: request.observed_at,
      },
    )
    .unwrap();
    let record = if self.evidence {
      record
        .with_discovery(verified_evidence(&request, outcome.schema()))
        .unwrap()
    } else {
      record
    };
    Ok(FactoryNodeExecutionStep::Completed {
      execution: None,
      record: Box::new(record),
      usage: BudgetUsage::default(),
      outbox: vec![],
    })
  }
}
pub struct Installation;
#[async_trait]
impl FactoryCapabilitySource for Installation {
  async fn availability(
    &self,
    _: octacity_server_domain::ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    let references = [
      (
        FactoryChoiceKind::AdmissionPolicy,
        "admission",
        reference("fixture.admission", 1),
      ),
      (
        FactoryChoiceKind::PermissionCeiling,
        "permissions",
        reference("fixture.permissions", 2),
      ),
      (
        FactoryChoiceKind::NodeCapability,
        "observer",
        reference("fixture.observer", 3),
      ),
      (
        FactoryChoiceKind::NodeCapability,
        "evidence",
        reference("fixture.evidence", 4),
      ),
      (
        FactoryChoiceKind::NodeCapability,
        "classifier",
        reference("fixture.classifier", 6),
      ),
    ]
    .into_iter()
    .map(|(kind, alias, reference)| FactoryReferenceChoice {
      kind,
      alias: key(alias),
      reference,
    })
    .collect();
    Ok(FactoryConfigurationAvailability::Available(
      FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
        references,
        build_configurations: vec![],
      })
      .unwrap(),
    ))
  }
}
#[async_trait]
impl octacity_server::FactoryRuntimeReadiness for Installation {
  async fn available(&self) -> bool {
    true
  }
}
#[async_trait]
impl LeaseBoundToolActionAuthorizer for Installation {
  async fn authorize(
    &self,
    input: LeaseBoundToolActionInput,
  ) -> Result<octacity_protocol::FactoryToolActionDecisionV3, AgentToolActionError> {
    octacity_protocol::FactoryToolActionDecisionV3::new(
      input.proposal_sha256,
      octacity_protocol::FactoryToolActionDispositionV3::Deny,
      octacity_protocol::FactoryToolActionDecisionSourceV3::HardPolicy,
      None,
    )
    .map_err(|_| AgentToolActionError::Unavailable)
  }
}
#[async_trait]
impl octacity_server::FactoryRuntimeInstaller for Installation {
  async fn install(
    &self,
    _: octacity_server::FactoryRuntimeResources,
  ) -> Result<octacity_server::FactoryRuntimeInstallation, octacity_server::FactoryRuntimeInstallationError> {
    Ok(octacity_server::FactoryRuntimeInstallation {
      capabilities: Arc::new(Installation),
      inputs: Arc::new(Inputs),
      readiness: Arc::new(Installation),
      outbox_dispatchers: vec![],
      tool_authorizer: Some(Arc::new(Installation)),
      output_finalizer: None,
      work_sources: None,
      profile_selectors: None,
      executors: vec![
        (
          FlowNodeKind::TrustedAction,
          NodeExecutionIdentity::External(reference("fixture.observer", 3)),
          Arc::new(Observer {
            evidence: false,
            value: json!({"fit":"fits","duplicate":false,"input_digest":""}),
          }),
        ),
        (
          FlowNodeKind::TrustedAction,
          NodeExecutionIdentity::External(reference("fixture.evidence", 4)),
          Arc::new(Observer {
            evidence: true,
            value: json!(true),
          }),
        ),
        (
          FlowNodeKind::TrustedAction,
          NodeExecutionIdentity::External(reference("fixture.classifier", 6)),
          Arc::new(Observer {
            evidence: false,
            value: json!({"work_kind":"feature_request","component":"api",
            "severity":"low","size":"small","risk":"low","dependencies":[],
            "reproducibility":"not_applicable","recommended_route":"review"}),
          }),
        ),
      ],
    })
  }
}
// Represents a trusted external producer's verified receipt. Artifact bytes and
// ordinary producer execution are fixture boundaries, never management inputs.
fn verified_evidence(request: &FactoryNodeExecutionRequest<'_>, schema: &ImmutableReference) -> FlowDiscoveryFacts {
  use octacity_server_domain::{ArtifactId, AttemptId, BuildId, JobId, Timestamp};
  let subject = CandidateSubject::new(
    request.input.exact_resource().unwrap().clone(),
    request.input.exact_resource().unwrap().base_revision().clone(),
    FactoryDigest::content_sha256(b"fixture-candidate"),
  );
  let changeset = ChangeSet::new(
    ChangeSetId::generate(),
    request.input,
    request.attempt,
    subject.clone(),
    ArtifactId::generate(),
    ArtifactId::generate(),
  )
  .unwrap();
  let plugin = reference("fixture.evidence", 4);
  let tool = reference("fixture.validation", 5);
  let kind = key("validation");
  let item = EvidenceItem::new(EvidenceItemInput {
    subject: subject.clone(),
    kind: kind.clone(),
    output_kind: EvidenceOutputKind::Report,
    artifact: FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::content_sha256(b"true"), 4).unwrap(),
    schema: schema.clone(),
    producer: EvidenceProducer::new(
      BuildId::generate(),
      AttemptId::generate(),
      JobId::generate(),
      tool.clone(),
      plugin.clone(),
    ),
    outcome: DeterministicGateOutcome::Passed,
    published_at: request.observed_at,
    fresh_until: Timestamp::from_unix_millis(request.observed_at.unix_millis() + 60000).unwrap(),
  });
  let manifest = EvidenceManifest::new(
    EvidenceManifestId::generate(),
    &changeset,
    subject,
    request.observed_at,
    vec![EvidenceRequirement::new(
      kind,
      EvidenceOutputKind::Report,
      schema.clone(),
      tool,
      plugin,
    )],
    vec![item],
  )
  .unwrap();
  FlowDiscoveryFacts::evidence(&manifest).unwrap()
}
// Authored contracts supplied to the client as JSON, like an operator's input
// document. The transport client does not compute server identities or call Rust services.
pub fn contracts() -> (Value, Value) {
  let boolean = FlowDataSchema::new(key("fixture.boolean"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let binding = FlowHumanGateBinding::new(
    FlowHumanOperation::Delivery,
    std::collections::BTreeMap::from([(FlowHumanDisposition::Confirm, key("requested"))]),
  )
  .unwrap();
  let operation = FlowDataSchema::new(
    key("fixture.operator"),
    key("v1"),
    binding.response_shape(FlowHumanDisposition::Confirm).unwrap(),
  )
  .unwrap();
  (
    serde_json::to_value(boolean).unwrap(),
    serde_json::to_value(operation).unwrap(),
  )
}

/// Authored typed contracts shared with the supplied two-phase triage configuration.
pub fn triage_contracts() -> (Value, Value) {
  fn publish(document: &str) -> Value {
    let document: Value = serde_json::from_str(document).unwrap();
    let schemas = document["schemas"].as_object().unwrap();
    assert_eq!(schemas.len(), 1);
    let schema = schemas.values().next().unwrap();
    serde_json::to_value(
      FlowDataSchema::new(
        key(schema["identity"].as_str().unwrap()),
        key(schema["version"].as_str().unwrap()),
        serde_json::from_value(schema["shape"].clone()).unwrap(),
      )
      .unwrap(),
    )
    .unwrap()
  }
  (
    publish(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../config/factory/flows/base-flow/schemas/eligibility_facts.json"
    ))),
    publish(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../config/factory/flows/base-flow/schemas/classification_observations.json"
    ))),
  )
}
pub fn budget() -> Value {
  json!({"max_attempts":32,"max_elapsed_millis":60000,"max_tokens":0,"max_cost_micro_units":0,"max_output_bytes":131072})
}
pub fn permissions() -> Value {
  serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap()
}
pub fn node(name: &str, kind: &str, input: &Value, output: &Value, outcome: &str) -> Value {
  json!({"key":name,"kind":kind,"input_schema":input,"outcomes":[{"key":outcome,"kind":"success","schema":output}],
    "budget":budget(),"permissions":permissions(),"required":true,"subflow":null,
    "input_binding":{"sources":{},"mapping":{"value":{"mapping":"constant","value":true}}}})
}
pub fn draft(
  entry: &str,
  nodes: Vec<Value>,
  transitions: Value,
  terminal: &str,
  terminal_schema: &Value,
  schemas: Vec<Value>,
) -> Value {
  json!({"root":{"input_schema":schemas[0]["reference"],"entry":entry,"nodes":nodes,"transitions":transitions,
    "terminals":[{"key":terminal,"schema":terminal_schema}],"context_projections":[],"data_projections":[],
    "execution":{"budget":budget(),"permissions":permissions(),"max_active_nodes":4}},
    "limits":{"max_depth":4,"max_expanded_nodes":16,"max_repeat_count":1,"max_fan_out":4,"max_wip":4,"budget":budget(),"permissions":permissions()},
    "data_schemas":schemas,"pools":{}})
}
pub fn transition(predecessor: &str, outcome: &str, kind: &str, target: &str) -> Value {
  json!({"predecessor":predecessor,"outcome":outcome,"target":{"kind":kind,"key":target},"max_repeats":0,"exhausted_terminal":null,"starts_cycle":false})
}

/// Authored deterministic routes over configured fields, independent of node names.
pub fn gate(name: &str, input: &Value, output: &Value, accepted: &str, comparisons: Vec<(&str, Value)>) -> Value {
  let program: FlowGateProgram = serde_json::from_value(json!({"rules":[{
  "when":{"operation":"all","predicates":comparisons.into_iter().map(|(path,value)| json!({
    "operation":"compare","left":{"source":"path","path":path},"comparison":"equal",
    "right":{"source":"literal","value":value}
  })).collect::<Vec<_>>()},"outcome":accepted}],"fallback":"rejected",
  "outputs":{
    accepted:{"value":{"mapping":"constant","value":true}},
    "rejected":{"value":{"mapping":"constant","value":false}}
  }}))
  .unwrap();
  let mut result = node(name, "deterministic_gate", input, output, accepted);
  result["outcomes"]
    .as_array_mut()
    .unwrap()
    .push(json!({"key":"rejected","kind":"failure","schema":output}));
  result["input_binding"] = json!({"sources":{"previous":{"source":"predecessor","view":"payload","path":""}},
    "mapping":{"value":{"mapping":"pointer","path":"/previous"}}});
  result["gate"] = serde_json::to_value(program.binding().unwrap()).unwrap();
  result
}
