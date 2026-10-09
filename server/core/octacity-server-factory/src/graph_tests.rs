use crate::graph::{StructuralEdge, StructuralIssue, analyze};

fn edge(predecessor: &str, successor: &str) -> StructuralEdge<String> {
  StructuralEdge::new(predecessor.to_owned(), successor.to_owned())
}

#[test]
fn canonicalizes_a_diamond_and_reports_bounded_facts() {
  let facts = analyze(
    ["delta", "beta", "alpha", "gamma"].map(str::to_owned),
    [
      edge("gamma", "delta"),
      edge("alpha", "gamma"),
      edge("beta", "delta"),
      edge("alpha", "beta"),
    ],
    ["alpha".to_owned()],
  )
  .unwrap();

  assert_eq!(facts.nodes, ["alpha", "beta", "delta", "gamma"]);
  assert_eq!(
    facts.edges,
    [
      edge("alpha", "beta"),
      edge("alpha", "gamma"),
      edge("beta", "delta"),
      edge("gamma", "delta"),
    ]
  );
  assert_eq!(facts.reachable, ["alpha", "beta", "delta", "gamma"]);
  assert_eq!(facts.topological_order.unwrap(), ["alpha", "beta", "gamma", "delta"]);
  assert_eq!(facts.measurements.node_count, 4);
  assert_eq!(facts.measurements.edge_count, 4);
  assert_eq!(facts.measurements.max_depth, Some(2));
  assert_eq!(facts.measurements.max_fan_out, 2);
}

#[test]
fn reports_components_and_withholds_acyclic_facts_for_a_cycle() {
  let facts = analyze(
    ["delta", "alpha", "gamma", "beta"].map(str::to_owned),
    [
      edge("gamma", "delta"),
      edge("beta", "gamma"),
      edge("beta", "alpha"),
      edge("alpha", "beta"),
    ],
    ["alpha".to_owned()],
  )
  .unwrap();

  assert_eq!(
    facts.components,
    [
      vec!["alpha".to_owned(), "beta".to_owned()],
      vec!["delta".to_owned()],
      vec!["gamma".to_owned()]
    ]
  );
  assert_eq!(facts.cyclic_components, [vec!["alpha".to_owned(), "beta".to_owned()]]);
  assert_eq!(facts.topological_order, None);
  assert_eq!(facts.measurements.max_depth, None);
}

#[test]
fn returns_all_canonical_structure_issues_before_analysis() {
  let issues = analyze(
    ["alpha", "alpha"].map(str::to_owned),
    [edge("missing", "alpha"), edge("missing", "alpha")],
    ["root".to_owned()],
  )
  .unwrap_err();

  assert_eq!(
    issues,
    [
      StructuralIssue::DuplicateNode("alpha".to_owned()),
      StructuralIssue::DuplicateEdge(edge("missing", "alpha")),
      StructuralIssue::MissingNode("missing".to_owned()),
      StructuralIssue::MissingNode("root".to_owned()),
    ]
  );
}
