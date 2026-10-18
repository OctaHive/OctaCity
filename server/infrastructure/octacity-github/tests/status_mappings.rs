use octacity_github::status_mapping::StatusMapping;
use serde_json::json;

fn document() -> serde_json::Value {
  json!({"config_version":1,"managed_label_groups":{"review":["ready","blocked"]},
    "statuses":{"publish":{"add":["ready"],"remove":["blocked"]}}})
}

#[test]
fn editable_mappings_narrow_installed_managed_labels_and_states_for_arbitrary_logical_values() {
  let effects = octacity_work_source_protocol::reporter::ProjectionEffects {
    add: vec!["blocked".into()],
    remove: vec!["ready".into()],
    state: Some("closed".into()),
  };
  let legacy = StatusMapping::from_config(&serde_json::to_vec(&document()).unwrap()).unwrap();
  assert!(legacy.with_effects("different-status", &effects).is_err());
  let mut doc = document();
  doc["editable"] = json!(true);
  doc["editable_states"] = json!(["closed"]);
  let mapping = StatusMapping::from_config(&serde_json::to_vec(&doc).unwrap()).unwrap();
  let narrowed = mapping.with_effects("different-status", &effects).unwrap();
  assert_eq!(narrowed.effects("different-status").unwrap().add(), ["blocked"]);
  assert_eq!(narrowed.effects("different-status").unwrap().remove(), ["ready"]);
  for invalid in [
    octacity_work_source_protocol::reporter::ProjectionEffects {
      add: vec!["unmanaged".into()],
      ..effects.clone()
    },
    octacity_work_source_protocol::reporter::ProjectionEffects {
      state: Some("open".into()),
      ..effects.clone()
    },
    octacity_work_source_protocol::reporter::ProjectionEffects {
      remove: vec!["BLOCKED".into()],
      ..effects.clone()
    },
  ] {
    assert!(mapping.with_effects("different-status", &invalid).is_err());
  }
  assert_eq!(
    mapping.effects("publish").unwrap().add(),
    ["ready"],
    "an override cannot mutate the installed mapping"
  );
}

#[test]
fn custom_effects_keep_declared_label_bounds_without_reencoding_the_installed_catalogue() {
  let labels: Vec<_> = (0..128).map(|n| format!("{n:03}{}", "x".repeat(253))).collect();
  let bytes = serde_json::to_vec(&json!({"config_version":1,"editable":true,
    "managed_label_groups":{"first":&labels[..64],"second":&labels[64..]},
    "statuses":{"installed":{"add":[],"remove":[]}}}))
  .unwrap();
  let installed = StatusMapping::from_config(&bytes).unwrap();
  let effects = octacity_work_source_protocol::reporter::ProjectionEffects {
    add: labels[..64].to_vec(),
    remove: labels[64..].to_vec(),
    state: None,
  };
  let selected = installed.with_effects("custom", &effects).unwrap();
  assert_eq!(selected.effects("custom").unwrap().add(), &labels[..64]);
  assert_eq!(selected.effects("custom").unwrap().remove(), &labels[64..]);
}

#[test]
fn conflicting_unmanaged_and_unbounded_mappings_are_rejected_before_use() {
  let mut invalid = Vec::new();
  for (path, value) in [
    ("/config_version", json!(2)),
    ("/managed_label_groups", json!({})),
    ("/managed_label_groups/review", json!([])),
    ("/managed_label_groups/review", json!(["ready", "READY"])),
    ("/managed_label_groups/review", json!(["ready", " "])),
    ("/managed_label_groups/review", json!(["ready", "bad\nlabel"])),
    ("/managed_label_groups/review", json!(["x".repeat(257)])),
    (
      "/managed_label_groups/review",
      json!((0..65).map(|n| format!("label-{n}")).collect::<Vec<_>>()),
    ),
    ("/statuses", json!({})),
    ("/statuses/publish/add", json!(["unmanaged"])),
    ("/statuses/publish/add", json!(["ready", "READY"])),
    ("/statuses/publish/remove", json!(["READY"])),
    ("/statuses/publish", json!({"add":[],"remove":[],"state":"all"})),
    ("/statuses/publish", json!({"add":[],"remove":[],"create_labels":true})),
  ] {
    let mut doc = document();
    *doc.pointer_mut(path).unwrap() = value;
    invalid.push(doc);
  }
  let mut overlap = document();
  overlap["managed_label_groups"]["another"] = json!(["READY"]);
  invalid.push(overlap);
  for doc in invalid {
    let result = StatusMapping::from_config(&serde_json::to_vec(&doc).unwrap());
    assert!(result.is_err(), "invalid mapping accepted: {doc}");
  }
  let bytes = vec![b' '; 65537];
  assert!(StatusMapping::from_config(&bytes).is_err());
}

#[test]
fn duplicate_status_or_group_definitions_cannot_silently_replace_reviewed_effects() {
  for bytes in [
    br#"{"config_version":1,"managed_label_groups":{"review":["ready"],"review":["blocked"]},"statuses":{"done":{"add":["blocked"],"remove":[]}}}"#.as_slice(),
    br#"{"config_version":1,"managed_label_groups":{"review":["ready","blocked"]},"statuses":{"done":{"add":["ready"],"remove":[]},"done":{"add":["blocked"],"remove":[]}}}"#.as_slice(),
  ] {
    assert!(StatusMapping::from_config(bytes).is_err(), "ambiguous configuration must be rejected");
  }
}

#[test]
fn dot_segment_labels_cannot_turn_a_targeted_removal_into_a_collection_delete() {
  for label in [".", ".."] {
    let mut doc = document();
    doc["managed_label_groups"]["review"] = json!([label]);
    doc["statuses"]["publish"] = json!({"add":[],"remove":[label]});
    assert!(StatusMapping::from_config(&serde_json::to_vec(&doc).unwrap()).is_err());
  }
}

#[test]
fn state_only_reporting_needs_no_dummy_managed_labels() {
  let doc = json!({"config_version":1,"managed_label_groups":{},
    "statuses":{"archive":{"add":[],"remove":[],"state":"closed"}}});
  let mapping = StatusMapping::from_config(&serde_json::to_vec(&doc).unwrap()).unwrap();
  let effect = mapping.effects("archive").unwrap();
  assert!(effect.add().is_empty());
  assert!(effect.remove().is_empty());
  assert_eq!(
    effect.state(),
    Some(octacity_github::issue_selector::IssueState::Closed)
  );
}

#[test]
fn arbitrary_statuses_resolve_only_explicit_managed_effects() {
  let mapping = StatusMapping::from_config(
    &serde_json::to_vec(&json!({
      "config_version": 1,
      "managed_label_groups": {"editorial": ["ready / review", "needs context"], "kind": ["Документация ✨"]},
      "statuses": {
        "needs-human-eyes": {"add": ["ready / review", "Документация ✨"], "remove": ["needs context"]},
        "archive": {"add": [], "remove": [], "state": "closed"},
        "resume": {"add": [], "remove": [], "state": "open"}
      }
    }))
    .unwrap(),
  )
  .unwrap();
  let effects = mapping.effects("needs-human-eyes").unwrap();
  assert_eq!(effects.add(), ["ready / review", "Документация ✨"]);
  assert_eq!(effects.remove(), ["needs context"]);
  assert_eq!(effects.state(), None);
  assert!(mapping.effects("archive").unwrap().state().is_some());
  for unsupported in ["approved", "rejected", "bug", "feature-request", "NEEDS-HUMAN-EYES"] {
    assert!(mapping.effects(unsupported).is_err());
  }
}
