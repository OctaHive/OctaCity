use serde::Deserialize;

use crate::{
  FactoryKey,
  flow_graph::{FactoryFlowEdge, FactoryFlowGraphIssue, analyze_factory_flow_graph},
};

#[derive(Debug, Deserialize)]
struct Corpus {
  format_version: u8,
  semantics: Semantics,
  cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Semantics {
  edge_orientation: String,
  identity_order: String,
  topological_tie_break: String,
  depth_unit: String,
  facts_require_well_formed_structure: bool,
}

#[derive(Debug, Deserialize)]
struct Case {
  name: String,
  covers: Vec<String>,
  nodes: Vec<String>,
  edges: Vec<Edge>,
  roots: Vec<String>,
  expected: Expected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct Edge {
  from: String,
  to: String,
}

#[derive(Debug, Deserialize)]
struct Expected {
  status: String,
  #[serde(default)]
  canonical_nodes: Vec<String>,
  #[serde(default)]
  canonical_edges: Vec<Edge>,
  #[serde(default)]
  reachable: Vec<String>,
  #[serde(default)]
  components: Vec<Vec<String>>,
  #[serde(default)]
  cyclic_components: Vec<Vec<String>>,
  topological_order: Option<Vec<String>>,
  measurements: Option<Measurements>,
  #[serde(default)]
  issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct Measurements {
  node_count: usize,
  edge_count: usize,
  max_depth: Option<usize>,
  max_fan_out: usize,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Issue {
  kind: String,
  node: Option<String>,
  from: Option<String>,
  to: Option<String>,
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("conformance identity is a valid Factory key")
}

fn edge(value: &Edge) -> FactoryFlowEdge {
  FactoryFlowEdge::new(key(&value.from), key(&value.to))
}

fn strings(values: &[FactoryKey]) -> Vec<String> {
  values.iter().map(ToString::to_string).collect()
}

fn components(values: &[Vec<FactoryKey>]) -> Vec<Vec<String>> {
  values.iter().map(|component| strings(component)).collect()
}

fn issue(value: FactoryFlowGraphIssue) -> Issue {
  match value {
    FactoryFlowGraphIssue::DuplicateNode { node } => Issue {
      kind: "duplicate_node".to_owned(),
      node: Some(node.to_string()),
      from: None,
      to: None,
    },
    FactoryFlowGraphIssue::DuplicateEdge { predecessor, successor } => Issue {
      kind: "duplicate_edge".to_owned(),
      node: None,
      from: Some(predecessor.to_string()),
      to: Some(successor.to_string()),
    },
    FactoryFlowGraphIssue::MissingNode { node } => Issue {
      kind: "missing_node".to_owned(),
      node: Some(node.to_string()),
      from: None,
      to: None,
    },
  }
}

#[test]
fn factory_adapter_satisfies_the_provider_neutral_conformance_corpus() {
  let corpus: Corpus = serde_json::from_str(include_str!("../tests/fixtures/flow-graph-conformance-v1.json")).unwrap();
  assert_eq!(corpus.format_version, 1);
  assert_eq!(corpus.semantics.edge_orientation, "predecessor_to_successor");
  assert_eq!(corpus.semantics.identity_order, "ascending_utf8_bytes");
  assert_eq!(corpus.semantics.topological_tie_break, "smallest_ready_identity");
  assert_eq!(corpus.semantics.depth_unit, "edge_count");
  assert!(corpus.semantics.facts_require_well_formed_structure);

  for case in corpus.cases {
    let result = analyze_factory_flow_graph(
      case.nodes.iter().map(|node| key(node)),
      case.edges.iter().map(edge),
      case.roots.iter().map(|root| key(root)),
    );
    assert!(!case.covers.is_empty(), "{} has no coverage labels", case.name);
    match case.expected.status.as_str() {
      "valid" => {
        let facts = result.unwrap_or_else(|issues| panic!("{} failed: {issues:?}", case.name));
        assert_eq!(strings(facts.nodes()), case.expected.canonical_nodes, "{}", case.name);
        assert_eq!(
          facts
            .edges()
            .iter()
            .map(|edge| Edge {
              from: edge.predecessor().to_string(),
              to: edge.successor().to_string(),
            })
            .collect::<Vec<_>>(),
          case.expected.canonical_edges,
          "{}",
          case.name
        );
        assert_eq!(strings(facts.reachable()), case.expected.reachable, "{}", case.name);
        assert_eq!(
          components(facts.components()),
          case.expected.components,
          "{}",
          case.name
        );
        assert_eq!(
          components(facts.cyclic_components()),
          case.expected.cyclic_components,
          "{}",
          case.name
        );
        assert_eq!(
          facts.topological_order().map(strings),
          case.expected.topological_order,
          "{}",
          case.name
        );
        let expected = case.expected.measurements.expect("valid case has measurements");
        let actual = facts.measurements();
        assert_eq!(actual.node_count(), expected.node_count, "{}", case.name);
        assert_eq!(actual.edge_count(), expected.edge_count, "{}", case.name);
        assert_eq!(actual.max_depth(), expected.max_depth, "{}", case.name);
        assert_eq!(actual.max_fan_out(), expected.max_fan_out, "{}", case.name);
      }
      "invalid" => {
        let actual = result
          .expect_err("invalid conformance case was accepted")
          .into_iter()
          .map(issue)
          .collect::<Vec<_>>();
        assert_eq!(actual, case.expected.issues, "{}", case.name);
      }
      status => panic!("{} uses unknown expected status {status}", case.name),
    }
  }
}
