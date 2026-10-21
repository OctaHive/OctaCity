//! Closed review references and retained facts, independent of provider wire objects.
use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let digest = || schema_ref("FactoryDigest");
  let native = || json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_EXTERNAL_WORK_IDENTITY_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_EXTERNAL_WORK_IDENTITY_BYTES});
  schemas.insert(
    "IssueCodeReviewSelection".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        (
          "item",
          object([("resource", native()), ("item", native())], &["resource", "item"]),
        ),
      ],
      &["source", "item"],
    ),
  );
  schemas.insert(
    "IssueCodeReviewBinding".into(),
    object(
      [
        ("association", schema_ref("IssueWorkAssociation")),
        ("record", digest()),
        ("candidate", schema_ref("FactoryCandidateSubject")),
      ],
      &["association", "record", "candidate"],
    ),
  );
  schemas.insert(
    "AttachIssueCodeReviewRequest".into(),
    object(
      [
        ("selection", schema_ref("IssueCodeReviewSelection")),
        ("binding", nullable(schema_ref("IssueCodeReviewBinding"))),
      ],
      &["selection"],
    ),
  );
  schemas.insert(
    "IssueCodeReviewProvenance".into(),
    json!({"oneOf":[
      object([("kind",string_enum(&["explicit"])),("actor",schema_ref("AuditActor"))],&["kind","actor"]),
      object([("kind",string_enum(&["delivery"])),("receipt",digest())],&["kind","receipt"])
    ]}),
  );
  schemas.insert(
    "IssueCodeReviewAssociation".into(),
    object(
      [
        ("id", digest()),
        ("sequence", positive_integer()),
        ("project_id", id()),
        ("issue_id", id()),
        ("selection", schema_ref("IssueCodeReviewSelection")),
        ("binding", nullable(schema_ref("IssueCodeReviewBinding"))),
        ("provenance", schema_ref("IssueCodeReviewProvenance")),
        ("attached_at", non_negative_integer()),
      ],
      &[
        "id",
        "sequence",
        "project_id",
        "issue_id",
        "selection",
        "binding",
        "provenance",
        "attached_at",
      ],
    ),
  );
  schemas.insert(
    "IssueCodeReviewOutcome".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("association", schema_ref("IssueCodeReviewAssociation")),
      ],
      &["disposition", "association"],
    ),
  );
  let text = || {
    nullable(
      json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_FACTORY_SAFE_TEXT_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_FACTORY_SAFE_TEXT_BYTES}),
    )
  };
  let revision = || {
    nullable(
      json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_IMMUTABLE_REVISION_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_IMMUTABLE_REVISION_BYTES}),
    )
  };
  schemas.insert("IssueCodeReviewAvailability".into(),json!({"type":"string","enum":["available","blocked"],"description":"Current protected read authority only. Retained observations can be stale even when available; GET never contacts the provider."}));
  schemas.insert(
    "IssueCodeReviewObservation".into(),
    object(
      [
        ("selection", schema_ref("IssueCodeReviewSelection")),
        ("observed_at", non_negative_integer()),
        ("title", text()),
        ("number", text()),
        ("url", text()),
        ("author", text()),
        ("state", nullable(string_enum(&["open", "closed", "merged"]))),
        ("draft", nullable(boolean())),
        ("head", revision()),
        ("base", revision()),
        ("changed_files", nullable(non_negative_integer())),
        ("additions", nullable(non_negative_integer())),
        ("deletions", nullable(non_negative_integer())),
        ("review", text()),
        ("checks", text()),
      ],
      &[
        "selection",
        "observed_at",
        "title",
        "number",
        "url",
        "author",
        "state",
        "draft",
        "head",
        "base",
        "changed_files",
        "additions",
        "deletions",
        "review",
        "checks",
      ],
    ),
  );
  schemas.insert(
    "IssueCodeReviewProjection".into(),
    object(
      [
        ("association", schema_ref("IssueCodeReviewAssociation")),
        ("observation", nullable(schema_ref("IssueCodeReviewObservation"))),
        ("availability", schema_ref("IssueCodeReviewAvailability")),
      ],
      &["association", "observation", "availability"],
    ),
  );
  schemas.insert("IssueCodeReviewPage".into(),object([
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,"items":schema_ref("IssueCodeReviewProjection")})),
    ("next_cursor",nullable(json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES}))),
    ("observation",string_enum(&["issue-code-review-membership-v1"]))
  ],&["items","next_cursor","observation"]));
  schemas.insert("RefreshIssueCodeReviewRequest".into(), object([], &[]));
  schemas.insert(
    "IssueCodeReviewObservationRecord".into(),
    object(
      [
        ("id", digest()),
        ("sequence", positive_integer()),
        ("association_id", digest()),
        ("observation", schema_ref("IssueCodeReviewObservation")),
      ],
      &["id", "sequence", "association_id", "observation"],
    ),
  );
  schemas.insert(
    "IssueCodeReviewObservationOutcome".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("observation", schema_ref("IssueCodeReviewObservationRecord")),
      ],
      &["disposition", "observation"],
    ),
  );
  schemas.insert("IssueCodeReviewObservationPage".into(),object([
    ("association",schema_ref("IssueCodeReviewAssociation")),
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_HISTORY_PAGE_SIZE,"items":schema_ref("IssueCodeReviewObservationRecord")})),
    ("availability",schema_ref("IssueCodeReviewAvailability")),
    ("next_cursor",nullable(json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES})))
  ],&["association","items","availability","next_cursor"]));
}
