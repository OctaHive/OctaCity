//! Context remains within the selected code resource or explicit no-code Work input.
use super::*;

#[test]
fn no_code_context_uses_versioned_work_input_provenance_and_cannot_include_repository_ranges() {
  use octacity_server_factory::{
    ContextManifest, ContextManifestEntry, ContextManifestId, ContextSourceKind, FactoryContextReference,
    FactoryRepositoryPath, FactoryRepositoryRange, FactorySafeText, FactoryTaskSubject,
  };
  run_ready(async {
    for repository_work in [false, true] {
      let snapshot = resource_snapshot(repository_work).await;
      let flow = &snapshot.admitted_flow;
      let root = flow.closure().definition(flow.closure().root()).unwrap();
      let node = root
        .nodes()
        .iter()
        .find(|node| node.code() == Some(&FlowCodeBinding::None))
        .unwrap();
      let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
      let input = FlowNodeInput::new(
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        node.key().clone(),
        FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
      )
      .unwrap();
      let subject = serde_json::from_value::<FactoryTaskSubject>(serde_json::json!({
        "kind":"work_input_v1","value":snapshot.work.subject(),
      }));
      assert!(
        subject.is_ok(),
        "document-only Context needs explicit Work/input provenance: {subject:?}"
      );
      let subject = subject.unwrap();
      let document = artifact(73);
      let entry = ContextManifestEntry::new(
        ContextSourceKind::Specification,
        key("guidance"),
        subject.clone(),
        FactoryContextReference::Artifact(document.clone()),
        document.content_digest(),
        document.encoded_size(),
        FactorySafeText::new("Explicit selected Project document").unwrap(),
        digest(74),
      )
      .unwrap();
      let context =
        ContextManifest::new(ContextManifestId::generate(), subject.clone(), digest(75), vec![entry]).unwrap();
      let selected = input.with_context(context.clone(), vec![]).unwrap();
      assert_eq!(selected.context(), Some(&context));
      let restored = FlowNodeInput::restore(
        &serde_json::to_vec(&selected).unwrap(),
        &snapshot.work,
        flow,
        flow.root_run(),
        flow.initial_cycle(),
        schema,
        selected.digest().unwrap(),
      )
      .unwrap();
      assert_eq!(restored, selected);
      let range = FactoryRepositoryRange::new(
        RepositoryId::generate(),
        "unselected-revision".parse().unwrap(),
        FactoryRepositoryPath::new("src/lib.rs").unwrap(),
        1,
        1,
      )
      .unwrap();
      assert!(
        ContextManifestEntry::new(
          ContextSourceKind::RepositoryRange,
          key("unselected_code"),
          subject,
          FactoryContextReference::RepositoryRange(range),
          digest(76),
          1,
          FactorySafeText::new("Not an attached source").unwrap(),
          digest(77)
        )
        .is_err()
      );
    }
  });
}

#[test]
fn explicit_base_code_selection_cannot_be_overridden_by_a_candidate_context() {
  use octacity_server_factory::{
    CandidateSubject, ContextManifest, ContextManifestEntry, ContextManifestId, ContextSourceKind,
    FactoryContextReference, FactorySafeText, FactoryTaskSubject,
  };
  run_ready(async {
    let snapshot = resource_snapshot(true).await;
    let flow = &snapshot.admitted_flow;
    let root = flow.closure().definition(flow.closure().root()).unwrap();
    let node = root
      .nodes()
      .iter()
      .find(|node| matches!(node.code(),Some(FlowCodeBinding::Source {source,..}) if source.as_str() == "second"))
      .unwrap();
    let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
    let input = FlowNodeInput::new(
      &snapshot.work,
      flow,
      flow.root_run(),
      flow.initial_cycle(),
      node.key().clone(),
      FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
    )
    .unwrap();
    let subject = FactoryTaskSubject::Candidate(CandidateSubject::new(
      input.exact_resource().unwrap().clone(),
      "unselected-candidate".parse().unwrap(),
      digest(82),
    ));
    let document = artifact(83);
    let entry = ContextManifestEntry::new(
      ContextSourceKind::Specification,
      key("guidance"),
      subject.clone(),
      FactoryContextReference::Artifact(document.clone()),
      document.content_digest(),
      document.encoded_size(),
      FactorySafeText::new("Explicit document with an unselected revision").unwrap(),
      digest(84),
    )
    .unwrap();
    let context = ContextManifest::new(ContextManifestId::generate(), subject, digest(85), vec![entry]).unwrap();
    assert!(
      input.with_context(context, vec![]).is_err(),
      "Context must not override an explicit admitted-base resource choice"
    );
  });
}

#[test]
fn source_context_cannot_widen_the_frozen_step_path_and_line_scope() {
  use octacity_server_factory::{
    ContextManifest, ContextManifestEntry, ContextManifestId, ContextSourceKind, FactoryContextReference,
    FactoryRepositoryPath, FactoryRepositoryRange, FactorySafeText,
  };
  run_ready(async {
    let scope = serde_json::from_value(serde_json::json!({"kind":"selected","entries":[
      {"kind":"path","path":"src/engine"},
      {"kind":"range","path":"docs/design.md","lines":{"first_line":10,"last_line":20}}
    ]}))
    .unwrap();
    let CodeFixture {
      store,
      repositories,
      request,
      ..
    } = code_fixture_with_resources(octacity_server_factory::FlowCodeRevision::AdmittedBase, scope).await;
    let admission = admit(
      &FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default()))
        .with_repositories(repositories),
      request,
    )
    .await
    .unwrap();
    let snapshot = store.factory_run_snapshot(admission.factory_run_id).await.unwrap();
    let flow = &snapshot.admitted_flow;
    let root = flow.closure().definition(flow.closure().root()).unwrap();
    let node = root
      .nodes()
      .iter()
      .find(|node| matches!(node.code(),Some(FlowCodeBinding::Source {source,..}) if source.as_str() == "first"))
      .unwrap();
    let schema = flow.data_schema(node.input_schema().unwrap()).unwrap();
    let input = FlowNodeInput::new(
      &snapshot.work,
      flow,
      flow.root_run(),
      flow.initial_cycle(),
      node.key().clone(),
      FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
    )
    .unwrap();
    let resource = input.exact_resource().unwrap();
    let context_for = |path: &str, first, last| {
      let subject = input.task_subject().unwrap();
      let range = FactoryRepositoryRange::new(
        resource.repository_id(),
        resource.base_revision().clone(),
        FactoryRepositoryPath::new(path).unwrap(),
        first,
        last,
      )
      .unwrap();
      let entry = ContextManifestEntry::new(
        ContextSourceKind::RepositoryRange,
        key("selected_code"),
        subject.clone(),
        FactoryContextReference::RepositoryRange(range),
        digest(90),
        1,
        FactorySafeText::new("Explicit bounded code view").unwrap(),
        digest(91),
      )
      .unwrap();
      ContextManifest::new(ContextManifestId::generate(), subject, digest(92), vec![entry]).unwrap()
    };
    for (path, first, last) in [("src/engine/lib.rs", 1, 100), ("docs/design.md", 12, 13)] {
      input
        .clone()
        .with_context(context_for(path, first, last), vec![])
        .unwrap();
    }
    for (path, first, last) in [
      ("src/other.rs", 1, 10),
      ("src/engine2/lib.rs", 1, 10),
      ("docs/design.md", 9, 10),
      ("docs/design.md", 20, 21),
      ("docs/private.md", 10, 11),
    ] {
      assert!(
        input
          .clone()
          .with_context(context_for(path, first, last), vec![])
          .is_err(),
        "Context widened the selected scope: {path}:{first}-{last}"
      );
    }
  });
}

#[path = "factory_resource_context_owner_tests.rs"]
mod artifact_owner;
