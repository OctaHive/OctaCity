//! Closed read-only result schemas; reference visibility never grants byte access or acceptance.
use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let text =
    || json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_FACTORY_SAFE_TEXT_BYTES});
  let digest = || schema_ref("FactoryDigest");
  let cursor = || {
    nullable(
      json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_ISSUE_OBSERVATION_CURSOR_BYTES}),
    )
  };
  schemas.insert(
    "IssueCheckState".into(),
    string_enum(&[
      "missing",
      "pending",
      "running",
      "pass",
      "fail",
      "cancelled",
      "skipped",
      "indeterminate",
      "unavailable",
    ]),
  );
  schemas.insert(
    "IssueEvidenceProducer".into(),
    object(
      [
        ("build_id", id()),
        ("attempt_id", id()),
        ("job_id", id()),
        ("tool", schema_ref("FactoryImmutableReferenceResource")),
        ("plugin", schema_ref("FactoryImmutableReferenceResource")),
      ],
      &["build_id", "attempt_id", "job_id", "tool", "plugin"],
    ),
  );
  schemas.insert(
    "IssueCheckProjection".into(),
    object(
      [
        ("id", digest()),
        ("name", text()),
        ("required", boolean()),
        ("state", schema_ref("IssueCheckState")),
        ("producer", nullable(schema_ref("IssueEvidenceProducer"))),
        ("record", nullable(digest())),
        ("observed_at", nullable(non_negative_integer())),
      ],
      &["id", "name", "required", "state", "producer", "record", "observed_at"],
    ),
  );
  schemas.insert("IssueChecksPage".into(),object([("association",schema_ref("IssueWorkAssociation")),("subject",schema_ref("FactoryCandidateSubject")),("record",digest()),("run_version",positive_integer()),("coverage",schema_ref("IssueResultCoverage")),("items",json!({"type":"array","maxItems":octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,"items":schema_ref("IssueCheckProjection")})),("next_cursor",cursor())],&["association","subject","record","run_version","coverage","items","next_cursor"]));
  schemas.insert(
    "IssueArtifactAvailability".into(),
    string_enum(&["available", "expired", "unavailable"]),
  );
  schemas.insert(
    "IssueArtifactMetadata".into(),
    object(
      [
        ("name", text()),
        ("media_type", text()),
        (
          "origin",
          string_enum(&["project_input", "source_input", "build_output"]),
        ),
        ("availability", schema_ref("IssueArtifactAvailability")),
        ("published_at", nullable(non_negative_integer())),
        ("expires_at", nullable(non_negative_integer())),
      ],
      &[
        "name",
        "media_type",
        "origin",
        "availability",
        "published_at",
        "expires_at",
      ],
    ),
  );
  schemas.insert(
    "IssueArtifactProjection".into(),
    object(
      [
        ("artifact", schema_ref("FactoryArtifactReference")),
        (
          "group",
          string_enum(&["original_input", "output", "report", "screenshot"]),
        ),
        ("producer", nullable(schema_ref("IssueEvidenceProducer"))),
        ("record", nullable(digest())),
        ("subject", nullable(schema_ref("FactoryCandidateSubject"))),
        ("metadata", nullable(schema_ref("IssueArtifactMetadata"))),
        ("availability", schema_ref("IssueArtifactAvailability")),
      ],
      &[
        "artifact",
        "group",
        "producer",
        "record",
        "subject",
        "metadata",
        "availability",
      ],
    ),
  );
  schemas.insert("IssueArtifactsPage".into(),object([("association",schema_ref("IssueWorkAssociation")),("run_version",positive_integer()),("coverage",schema_ref("IssueResultCoverage")),("items",json!({"type":"array","maxItems":octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,"items":schema_ref("IssueArtifactProjection")})),("next_cursor",cursor())],&["association","run_version","coverage","items","next_cursor"]));
  schemas.insert(
    "IssueExternalCheck".into(),
    object(
      [
        ("id", text()),
        ("name", text()),
        ("producer", nullable(text())),
        ("state", schema_ref("IssueCheckState")),
        ("required", nullable(boolean())),
      ],
      &["id", "name", "producer", "state", "required"],
    ),
  );
  let revision =
    || json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_IMMUTABLE_REVISION_BYTES});
  schemas.insert("IssueExternalChecks".into(),object([("source",string_enum(&["external_ci"])),("head",revision()),("observed_at",non_negative_integer()),("has_more",boolean()),("items",json!({"type":"array","maxItems":octacity_server_application::MAX_FACTORY_DIAGNOSTIC_PAGE_SIZE,"items":schema_ref("IssueExternalCheck")}))],&["source","head","observed_at","has_more","items"]));
  schemas["IssueCodeReviewObservation"]["properties"]["check_details"] = schema_ref("IssueExternalChecks");
  schemas.insert(
    "IssueExternalCheckObservation".into(),
    object(
      [
        ("id", digest()),
        ("head", nullable(revision())),
        ("observed_at", non_negative_integer()),
        ("checks", nullable(schema_ref("IssueExternalChecks"))),
      ],
      &["id", "head", "observed_at", "checks"],
    ),
  );
  schemas.insert(
    "IssueExternalChecksPage".into(),
    object(
      [
        ("source", string_enum(&["external_ci"])),
        ("association", schema_ref("IssueCodeReviewAssociation")),
        ("availability", schema_ref("IssueCodeReviewAvailability")),
        ("coverage", schema_ref("IssueResultCoverage")),
        (
          "items",
          json!({"type":"array","maxItems":octacity_server_application::MAX_ISSUE_EXTERNAL_CHECK_OBSERVATIONS,"items":schema_ref("IssueExternalCheckObservation")}),
        ),
        ("next_cursor", cursor()),
      ],
      &[
        "source",
        "association",
        "availability",
        "coverage",
        "items",
        "next_cursor",
      ],
    ),
  );
  schemas.insert("IssueArtifactContent".into(),json!({"type":"string","format":"binary","description":format!("Explicit retained download up to {} bytes. Preview is bounded to {} encoded/output bytes, dimensions {} and allocation {} bytes; text is forced plaintext and PNG/JPEG/WebP is decoded and normalized to PNG. HTML/SVG is unsupported.",octacity_server_application::MAX_ISSUE_ARTIFACT_DOWNLOAD_BYTES,octacity_server_application::MAX_ISSUE_ARTIFACT_PREVIEW_BYTES,octacity_server_application::MAX_ISSUE_ARTIFACT_PREVIEW_DIMENSION,octacity_server_application::MAX_ISSUE_ARTIFACT_PREVIEW_ALLOC_BYTES)}));
}
