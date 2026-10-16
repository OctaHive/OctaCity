use octacity_github::issue_selector::{IssueSelector, IssueSelectorDraft, IssueState, IssueStateSelection};

fn names(values: &[&str]) -> Vec<String> {
  values.iter().map(|value| (*value).to_owned()).collect()
}

fn draft(all_of: &[&str], any_of: &[&str], none_of: &[&str]) -> IssueSelectorDraft {
  IssueSelectorDraft {
    state: IssueStateSelection::Open,
    all_of: names(all_of),
    any_of: names(any_of),
    none_of: names(none_of),
  }
}

#[test]
fn compound_predicates_use_arbitrary_literal_labels_without_a_business_vocabulary() {
  let selector = IssueSelector::new(draft(
    &["ready-for-review", "Документация ✨"],
    &["team:docs / follow-up", "release [next].*"],
    &["on hold", "не брать"],
  ))
  .unwrap();
  for (labels, expected) in [
    (vec![], false),
    (vec!["ready-for-review", "Документация ✨"], false),
    (vec!["ready-for-review", "team:docs / follow-up"], false),
    (vec!["Документация ✨", "team:docs / follow-up"], false),
    (
      vec!["ready-for-review", "Документация ✨", "team:docs / follow-up"],
      true,
    ),
    (
      vec!["ready-for-review", "Документация ✨", "release [next].*", "unrelated"],
      true,
    ),
    (vec!["ready-for-review", "Документация ✨", "release next"], false),
    (
      vec![
        "ready-for-review",
        "Документация ✨",
        "team:docs / follow-up",
        "on hold",
      ],
      false,
    ),
    (
      vec!["ready-for-review", "Документация ✨", "release [next].*", "не брать"],
      false,
    ),
    (vec!["approved", "bug", "feature-request"], false),
  ] {
    assert_eq!(
      selector.matches(IssueState::Open, &names(&labels)),
      expected,
      "{labels:?}"
    );
  }
}

#[test]
fn empty_predicates_add_no_implicit_label_restriction_including_unlabeled_issues() {
  let unrestricted = IssueSelector::new(draft(&[], &[], &[])).unwrap();
  for labels in [vec![], vec!["unrelated"], vec!["approved", "rejected", "bug"]] {
    assert!(unrestricted.matches(IssueState::Open, &names(&labels)), "{labels:?}");
  }
  let required_only = IssueSelector::new(draft(&["ready-for-review"], &[], &[])).unwrap();
  assert!(!required_only.matches(IssueState::Open, &[]));
  assert!(required_only.matches(IssueState::Open, &names(&["ready-for-review"])));

  let excluded_only = IssueSelector::new(draft(&[], &[], &["blocked"])).unwrap();
  assert!(excluded_only.matches(IssueState::Open, &[]));
  assert!(excluded_only.matches(IssueState::Open, &names(&["ready-for-review"])));
  assert!(!excluded_only.matches(IssueState::Open, &names(&["blocked"])));
}

#[test]
fn issue_state_selection_is_independent_of_label_predicates() {
  for (selection, actual, expected) in [
    (IssueStateSelection::Open, IssueState::Open, true),
    (IssueStateSelection::Open, IssueState::Closed, false),
    (IssueStateSelection::Closed, IssueState::Open, false),
    (IssueStateSelection::Closed, IssueState::Closed, true),
    (IssueStateSelection::All, IssueState::Open, true),
    (IssueStateSelection::All, IssueState::Closed, true),
  ] {
    let mut definition = draft(&[], &[], &[]);
    definition.state = selection;
    let selector = IssueSelector::new(definition).unwrap();
    assert_eq!(selector.matches(actual, &[]), expected, "{selection:?}/{actual:?}");
  }
  let mut definition = draft(&["ready-for-review"], &[], &[]);
  definition.state = IssueStateSelection::All;
  let selector = IssueSelector::new(definition).unwrap();
  assert!(!selector.matches(IssueState::Closed, &[]));
  assert!(selector.matches(IssueState::Closed, &names(&["ready-for-review"])));
  assert!(serde_json::from_str::<IssueState>("\"all\"").is_err());
  assert!(serde_json::from_str::<IssueStateSelection>("\"merged\"").is_err());
}

#[test]
fn contradictory_predicates_fail_with_safe_field_errors_before_matching() {
  for (definition, expected) in [
    (
      draft(&["private label value"], &[], &["private label value"]),
      "invalid GitHub Issue selector: NoneOf (Contradictory)",
    ),
    (
      draft(&[], &["feature", "enhancement"], &["feature", "enhancement"]),
      "invalid GitHub Issue selector: AnyOf (Contradictory)",
    ),
    (
      draft(&["ready"], &["feature"], &["feature"]),
      "invalid GitHub Issue selector: AnyOf (Contradictory)",
    ),
  ] {
    assert_eq!(IssueSelector::new(definition).unwrap_err().to_string(), expected);
  }

  let partially_excluded = IssueSelector::new(draft(&["ready"], &["feature", "enhancement"], &["feature"])).unwrap();
  assert!(partially_excluded.matches(IssueState::Open, &names(&["ready", "enhancement"])));
  assert!(!partially_excluded.matches(IssueState::Open, &names(&["ready", "enhancement", "feature"])));
  let overlapping_required_alternative = IssueSelector::new(draft(&["ready"], &["ready"], &[])).unwrap();
  assert!(overlapping_required_alternative.matches(IssueState::Open, &names(&["ready"])));
}

#[test]
fn duplicate_labels_are_rejected_in_each_predicate_without_echoing_the_name() {
  for (definition, expected) in [
    (
      draft(&["private name", "private name"], &[], &[]),
      "invalid GitHub Issue selector: AllOf (Duplicate)",
    ),
    (
      draft(&[], &["private name", "private name"], &[]),
      "invalid GitHub Issue selector: AnyOf (Duplicate)",
    ),
    (
      draft(&[], &[], &["private name", "private name"]),
      "invalid GitHub Issue selector: NoneOf (Duplicate)",
    ),
  ] {
    assert_eq!(IssueSelector::new(definition).unwrap_err().to_string(), expected);
  }
}

fn predicate(field: &str, labels: Vec<String>) -> IssueSelectorDraft {
  let mut definition = draft(&[], &[], &[]);
  match field {
    "AllOf" => definition.all_of = labels,
    "AnyOf" => definition.any_of = labels,
    "NoneOf" => definition.none_of = labels,
    _ => unreachable!(),
  }
  definition
}

#[test]
fn predicate_counts_and_label_names_obey_declared_local_safety_bounds() {
  for field in ["AllOf", "AnyOf", "NoneOf"] {
    assert!(IssueSelector::new(predicate(field, (0..32).map(|i| format!("label-{i}")).collect())).is_ok());
    let oversized = predicate(field, (0..33).map(|i| format!("label-{i}")).collect());
    assert_eq!(
      IssueSelector::new(oversized).unwrap_err().to_string(),
      format!("invalid GitHub Issue selector: {field} (LimitExceeded)")
    );
    for invalid in [
      "".to_owned(),
      "   ".to_owned(),
      "private\nlabel".to_owned(),
      "private\0label".to_owned(),
      "x".repeat(257),
      "é".repeat(129),
    ] {
      assert_eq!(
        IssueSelector::new(predicate(field, vec![invalid]))
          .unwrap_err()
          .to_string(),
        format!("invalid GitHub Issue selector: {field} (InvalidLabel)")
      );
    }
    assert!(IssueSelector::new(predicate(field, vec!["🔎".repeat(64)])).is_ok());
    assert!(IssueSelector::new(predicate(field, names(&[" exact label with spaces "]))).is_ok());
  }
}

#[test]
fn label_case_cannot_hide_a_match_duplicate_or_contradiction() {
  let selector = IssueSelector::new(draft(
    &["Ready-for-Review", "Документация"],
    &["Team:Docs"],
    &["Blocked"],
  ))
  .unwrap();
  assert!(selector.matches(
    IssueState::Open,
    &names(&["ready-for-review", "ДОКУМЕНТАЦИЯ", "team:docs"])
  ));
  assert!(!selector.matches(
    IssueState::Open,
    &names(&["ready-for-review", "документация", "team:docs", "BLOCKED"])
  ));
  assert_eq!(
    serde_json::to_value(&selector).unwrap()["all_of"],
    serde_json::json!(["Ready-for-Review", "Документация"])
  );

  for field in ["AllOf", "AnyOf", "NoneOf"] {
    assert_eq!(
      IssueSelector::new(predicate(field, names(&["Review", "REVIEW"])))
        .unwrap_err()
        .to_string(),
      format!("invalid GitHub Issue selector: {field} (Duplicate)")
    );
  }
  assert_eq!(
    IssueSelector::new(draft(&["Ready"], &[], &["READY"]))
      .unwrap_err()
      .to_string(),
    "invalid GitHub Issue selector: NoneOf (Contradictory)"
  );
  assert_eq!(
    IssueSelector::new(draft(&[], &["Feature", "Enhancement"], &["FEATURE", "ENHANCEMENT"]))
      .unwrap_err()
      .to_string(),
    "invalid GitHub Issue selector: AnyOf (Contradictory)"
  );
}

#[test]
fn configuration_decoding_is_closed_bounded_and_revalidates_the_selector() {
  let valid = serde_json::to_vec(&draft(&[], &[], &[])).unwrap();
  let selector = IssueSelector::from_config(&valid).unwrap();
  assert!(selector.matches(IssueState::Open, &[]));
  assert_eq!(
    IssueSelector::from_config(&serde_json::to_vec(&selector).unwrap()).unwrap(),
    selector
  );

  let mut at_limit = vec![b' '; 65_536 - valid.len()];
  at_limit.extend_from_slice(&valid);
  assert!(IssueSelector::from_config(&at_limit).is_ok());
  at_limit.push(b' ');
  assert_eq!(
    IssueSelector::from_config(&at_limit).unwrap_err().to_string(),
    "invalid GitHub Issue selector: Selector (LimitExceeded)"
  );

  for invalid in [
    vec![0xff],
    b"not-json".to_vec(),
    b"null".to_vec(),
    b"[]".to_vec(),
    br#"{"state":"merged","all_of":[],"any_of":[],"none_of":[]}"#.to_vec(),
    br#"{"all_of":[],"any_of":[],"none_of":[]}"#.to_vec(),
    br#"{"state":"open","any_of":[],"none_of":[]}"#.to_vec(),
    br#"{"state":"open","all_of":"private input","any_of":[],"none_of":[]}"#.to_vec(),
    br#"{"state":"open","all_of":[],"any_of":[],"none_of":[],"query":"private provider query"}"#.to_vec(),
    br#"{"state":"open","state":"closed","all_of":[],"any_of":[],"none_of":[]}"#.to_vec(),
  ] {
    assert_eq!(
      IssueSelector::from_config(&invalid).unwrap_err().to_string(),
      "invalid GitHub Issue selector: Selector (Malformed)"
    );
  }
  let contradictory = serde_json::to_vec(&draft(&["private value"], &[], &["private value"])).unwrap();
  assert_eq!(
    IssueSelector::from_config(&contradictory).unwrap_err().to_string(),
    "invalid GitHub Issue selector: NoneOf (Contradictory)"
  );
  let oversized_predicate =
    serde_json::to_vec(&predicate("AnyOf", (0..33).map(|i| format!("label-{i}")).collect())).unwrap();
  assert_eq!(
    IssueSelector::from_config(&oversized_predicate)
      .unwrap_err()
      .to_string(),
    "invalid GitHub Issue selector: AnyOf (LimitExceeded)"
  );
}
