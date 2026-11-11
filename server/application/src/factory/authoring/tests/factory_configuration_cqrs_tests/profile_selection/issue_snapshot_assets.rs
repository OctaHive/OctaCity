//! Explicit text and evidence declarations retain bounds and cannot select private report bodies.
use super::*;

fn asset_draft(project: ProjectId) -> (Fixture, serde_json::Value) {
  let (fixture, mut wire) = snapshot_draft(project);
  let texts = FlowValueSchema::Array {
    item: Box::new(FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: 1_048_576,
    }),
    min_items: 0,
    max_items: 32,
  };
  let evidence = FlowValueSchema::Array {
    item: Box::new(FlowValueSchema::String {
      min_bytes: 64,
      max_bytes: 64,
    }),
    min_items: 0,
    max_items: 64,
  };
  snapshot_shape(
    &mut wire,
    [
      (key("visible"), FlowFieldSchema::required(FlowValueSchema::Boolean)),
      (key("notes"), FlowFieldSchema::optional(texts.clone())),
      (key("more_notes"), FlowFieldSchema::optional(texts)),
      (key("proofs"), FlowFieldSchema::optional(evidence)),
    ]
    .into(),
  );
  wire["issue_snapshot"]["materials"] = serde_json::json!({"notes":{
    "source":{"kind":"selected","max_entries":2},"required":false,"max_bytes":1024
  }});
  wire["issue_snapshot"]["evidence"] = serde_json::json!({"proofs":{"field":"visible","required":false,"maximum":2}});
  (fixture, wire)
}

#[test]
fn flow_publication_validates_explicit_material_and_evidence_selections() {
  run_ready(async {
    let project = ProjectId::generate();
    let properties = properties(project).await;
    let comment = serde_json::json!({"kind":"comment","comment_id":octacity_server_domain::IssueCommentId::generate(),"body_digest":digest(5)});
    let document =
      FactoryArtifactReference::new(octacity_server_domain::ArtifactId::generate(), digest(6), 16).unwrap();
    let shared = FactoryArtifactReference::new(octacity_server_domain::ArtifactId::generate(), digest(7), 16).unwrap();
    for source in [
      serde_json::json!({"kind":"selected","max_entries":2}),
      serde_json::json!({"kind":"exact","references":[comment.clone()]}),
      serde_json::json!({"kind":"exact","references":[comment.clone(),
        {"kind":"issue_document","artifact":document},{"kind":"project_document","artifact":shared}]}),
    ] {
      for required in [true, false] {
        let (fixture, mut wire) = asset_draft(project);
        wire["issue_snapshot"]["materials"]["notes"]["source"] = source.clone();
        wire["issue_snapshot"]["materials"]["notes"]["required"] = serde_json::json!(required);
        wire["issue_snapshot"]["evidence"]["proofs"]["required"] = serde_json::json!(required);
        let published = publish(project, fixture, wire.clone(), Some(properties.clone()))
          .await
          .unwrap();
        assert_eq!(
          serde_json::to_value(published).unwrap()["issue_snapshot"],
          wire["issue_snapshot"]
        );
      }
    }
    for (pointer, value, field) in [
      (
        "/issue_snapshot/materials/notes/source",
        serde_json::json!({"kind":"exact","references":[]}),
        "/issue_snapshot/materials/notes",
      ),
      (
        "/issue_snapshot/materials/notes/source",
        serde_json::json!({"kind":"selected","max_entries":33}),
        "/issue_snapshot/materials/notes",
      ),
      (
        "/issue_snapshot/materials/notes/source",
        serde_json::json!({"kind":"exact","references":[comment.clone(),comment]}),
        "/issue_snapshot/materials/notes",
      ),
      (
        "/issue_snapshot/materials/notes/source",
        serde_json::json!({"kind":"exact","references":[
        {"kind":"issue_document","artifact":document},{"kind":"project_document","artifact":document}]}),
        "/issue_snapshot/materials/notes",
      ),
      (
        "/issue_snapshot/materials/notes/max_bytes",
        serde_json::json!(0),
        "/issue_snapshot/materials/notes",
      ),
      (
        "/issue_snapshot/materials/notes/max_bytes",
        serde_json::json!(1_048_577),
        "/issue_snapshot/materials/notes",
      ),
      (
        "/issue_snapshot/evidence/proofs/field",
        serde_json::json!("undeclared"),
        "/issue_snapshot/evidence/proofs",
      ),
      (
        "/issue_snapshot/evidence/proofs/maximum",
        serde_json::json!(0),
        "/issue_snapshot/evidence/proofs",
      ),
      (
        "/issue_snapshot/evidence/proofs/maximum",
        serde_json::json!(65),
        "/issue_snapshot/evidence/proofs",
      ),
    ] {
      let (fixture, mut wire) = asset_draft(project);
      *wire.pointer_mut(pointer).unwrap() = value;
      assert_invalid(publish(project, fixture, wire, Some(properties.clone())).await, field);
    }
    for (entries, bytes) in [(17, 1024), (2, 600_000)] {
      let (fixture, mut wire) = asset_draft(project);
      wire["issue_snapshot"]["materials"]["notes"]["source"]["max_entries"] = serde_json::json!(entries);
      wire["issue_snapshot"]["materials"]["notes"]["max_bytes"] = serde_json::json!(bytes);
      wire["issue_snapshot"]["materials"]["more_notes"] = serde_json::json!({
        "source":{"kind":"selected","max_entries":16},"required":false,"max_bytes":bytes
      });
      assert_invalid(
        publish(project, fixture, wire, Some(properties.clone())).await,
        "/issue_snapshot/materials",
      );
    }
    let (_, mut wire) = asset_draft(project);
    wire["issue_snapshot"]["evidence"]["proofs"]["body"] = serde_json::json!(true);
    assert!(
      serde_json::from_value::<crate::FactoryFlowDefinitionDraft>(wire).is_err(),
      "private report bodies are not an input selector"
    );
  });
}

#[test]
fn flow_publication_accepts_the_exact_combined_32_entry_and_one_mib_material_budget() {
  run_ready(async {
    let project = ProjectId::generate();
    let (fixture, mut wire) = asset_draft(project);
    for alias in ["notes", "more_notes"] {
      wire["issue_snapshot"]["materials"][alias] = serde_json::json!({
        "source":{"kind":"selected","max_entries":16},
        "required":false,"max_bytes":524288
      });
    }
    let expected = wire["issue_snapshot"]["materials"].clone();
    let published = publish(project, fixture, wire, Some(properties(project).await))
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(published).unwrap()["issue_snapshot"]["materials"],
      expected
    );
  });
}
