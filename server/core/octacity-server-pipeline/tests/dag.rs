use octacity_server_domain::{JobName, PipelineNodeId};
use octacity_server_pipeline::{
  CapabilityCatalog, DependencyDecision, DependencyOutcome, DependencyPolicy, ExecutionCapability, MAX_PIPELINE_FAN_IN,
  MAX_PIPELINE_FAN_OUT, PipelineDag, PipelineEdge, PipelineError, PipelineNode, PublishablePipelineDag,
};
use serde_json::json;

#[test]
fn canonical_dag_validates_nodes_edges_and_stable_ordering() {
  let capabilities = catalog(&["native", "shell"]);
  let first = PublishablePipelineDag::new(
    vec![node("test", &["shell"]), node("build", &["native"])],
    vec![edge("build", "test")],
    &capabilities,
  )
  .unwrap();
  let second = PublishablePipelineDag::new(
    vec![node("build", &["native"]), node("test", &["shell"])],
    vec![edge("build", "test")],
    &capabilities,
  )
  .unwrap();

  assert_eq!(first, second);
  assert_eq!(ids(first.nodes()), ["build", "test"]);
  assert_eq!(ids(first.roots()), ["build"]);
  assert_eq!(ids(first.topological_order()), ["build", "test"]);
  let encoded = serde_json::to_vec(&first).unwrap();
  assert_eq!(encoded, serde_json::to_vec(&second).unwrap());
  assert_eq!(
    serde_json::from_slice::<PipelineDag>(&encoded).unwrap(),
    first.as_dag().clone()
  );
}

#[test]
fn restored_snapshot_requires_current_capability_validation_before_publication() {
  let published = PublishablePipelineDag::new(vec![node("build", &["native"])], vec![], &catalog(&["native"])).unwrap();
  let restored = serde_json::from_value::<PipelineDag>(serde_json::to_value(&published).unwrap()).unwrap();

  assert_eq!(
    restored.for_publication(&CapabilityCatalog::default()),
    Err(PipelineError::UnavailableCapability {
      node_id: id("build"),
      capability: capability("native"),
    })
  );
}

#[test]
fn rejects_cycles_missing_nodes_duplicate_nodes_and_edges() {
  let capabilities = CapabilityCatalog::default();
  let build = node("build", &[]);
  let test = node("test", &[]);

  assert_eq!(
    PublishablePipelineDag::new(vec![build.clone(), build.clone()], vec![], &capabilities),
    Err(PipelineError::DuplicateNode { node_id: id("build") })
  );
  assert_eq!(
    PublishablePipelineDag::new(vec![build.clone()], vec![edge("missing", "build")], &capabilities),
    Err(PipelineError::MissingNode { node_id: id("missing") })
  );
  assert_eq!(
    PublishablePipelineDag::new(
      vec![build.clone(), test.clone()],
      vec![edge("build", "test"), edge("build", "test")],
      &capabilities
    ),
    Err(PipelineError::DuplicateEdge)
  );
  assert_eq!(
    PublishablePipelineDag::new(vec![build.clone()], vec![edge("build", "build")], &capabilities),
    Err(PipelineError::SelfDependency { node_id: id("build") })
  );
  assert_eq!(
    PublishablePipelineDag::new(
      vec![build, test],
      vec![edge("build", "test"), edge("test", "build")],
      &capabilities
    ),
    Err(PipelineError::Cycle)
  );
}

#[test]
fn rejects_invalid_unavailable_and_duplicate_capabilities() {
  assert_eq!(
    ExecutionCapability::new("OCI Hypervisor"),
    Err(PipelineError::InvalidCapability)
  );
  let native = capability("native");
  assert_eq!(
    PipelineNode::new(
      id("build"),
      JobName::new("build").unwrap(),
      DependencyPolicy::AllSucceeded,
      vec![native.clone(), native.clone()],
      json!({})
    ),
    Err(PipelineError::DuplicateCapability { node_id: id("build") })
  );
  assert_eq!(
    PublishablePipelineDag::new(vec![node("build", &["native"])], vec![], &catalog(&["oci.process"])),
    Err(PipelineError::UnavailableCapability {
      node_id: id("build"),
      capability: native,
    })
  );
}

#[test]
fn bounds_fan_in_and_fan_out() {
  let capabilities = CapabilityCatalog::default();
  let mut fan_in_nodes = vec![node("target", &[])];
  let mut fan_in_edges = Vec::new();
  for index in 0..=MAX_PIPELINE_FAN_IN {
    let source = format!("source-{index:03}");
    fan_in_nodes.push(node(&source, &[]));
    fan_in_edges.push(edge(&source, "target"));
  }
  assert_eq!(
    PublishablePipelineDag::new(fan_in_nodes, fan_in_edges, &capabilities),
    Err(PipelineError::FanInExceeded { node_id: id("target") })
  );

  let mut fan_out_nodes = vec![node("source", &[])];
  let mut fan_out_edges = Vec::new();
  for index in 0..=MAX_PIPELINE_FAN_OUT {
    let target = format!("target-{index:03}");
    fan_out_nodes.push(node(&target, &[]));
    fan_out_edges.push(edge("source", &target));
  }
  assert_eq!(
    PublishablePipelineDag::new(fan_out_nodes, fan_out_edges, &capabilities),
    Err(PipelineError::FanOutExceeded { node_id: id("source") })
  );
}

#[test]
fn dependency_policies_make_failure_propagation_explicit() {
  use DependencyDecision::{Blocked, Ready, Skipped};
  use DependencyOutcome::{Cancelled, Failed, Pending, Skipped as UpstreamSkipped, Succeeded};

  assert_eq!(DependencyPolicy::AllSucceeded.decide([Succeeded, Pending]), Blocked);
  assert_eq!(DependencyPolicy::AllSucceeded.decide([Succeeded, Failed]), Skipped);
  assert_eq!(DependencyPolicy::AllSucceeded.decide([Succeeded, Succeeded]), Ready);
  assert_eq!(DependencyPolicy::AllCompleted.decide([Failed, Cancelled]), Ready);
  assert_eq!(DependencyPolicy::AllCompleted.decide([Failed, Pending]), Blocked);
  assert_eq!(DependencyPolicy::AnySucceeded.decide([Failed, Succeeded]), Ready);
  assert_eq!(DependencyPolicy::AnySucceeded.decide([Failed, Pending]), Blocked);
  assert_eq!(
    DependencyPolicy::AnySucceeded.decide([Failed, UpstreamSkipped]),
    Skipped
  );
  assert_eq!(DependencyPolicy::AllSucceeded.decide([]), Blocked);
  assert!(serde_json::from_str::<DependencyPolicy>("\"unsupported\"").is_err());
}

fn node(identity: &str, capabilities: &[&str]) -> PipelineNode {
  PipelineNode::new(
    id(identity),
    JobName::new(identity).unwrap(),
    DependencyPolicy::AllSucceeded,
    capabilities.iter().map(|value| capability(value)).collect(),
    json!({"command": identity, "nested": {"b": 2, "a": 1}}),
  )
  .unwrap()
}

fn edge(predecessor: &str, dependent: &str) -> PipelineEdge {
  PipelineEdge::new(id(predecessor), id(dependent))
}

fn id(value: &str) -> PipelineNodeId {
  PipelineNodeId::new(value).unwrap()
}

fn capability(value: &str) -> ExecutionCapability {
  ExecutionCapability::new(value).unwrap()
}

fn catalog(values: &[&str]) -> CapabilityCatalog {
  CapabilityCatalog::new(values.iter().map(|value| capability(value)))
}

fn ids<'a>(nodes: impl IntoIterator<Item = &'a PipelineNode>) -> Vec<&'a str> {
  nodes.into_iter().map(|node| node.id().as_str()).collect()
}
