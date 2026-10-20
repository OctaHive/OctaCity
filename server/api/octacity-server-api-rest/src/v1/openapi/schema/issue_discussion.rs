//! Bounded discussion schemas expose plain text only at the dedicated content boundary.
use super::super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  use octacity_server_application::{MAX_ISSUE_COMMENT_BODY_BYTES, MAX_ISSUE_DISCUSSION_COMMENTS};
  let identity = || json!({"type":"string","format":"uuid"});
  let body = json!({"type":"string","minLength":1,"maxLength":MAX_ISSUE_COMMENT_BODY_BYTES,
    "x-max-utf8-bytes":MAX_ISSUE_COMMENT_BODY_BYTES,
    "description":"Nonblank plain operator text. Recognized credentials/private capabilities reject; never execution or approval authority."});
  schemas.insert("IssueCommentBody".into(), body.clone());
  let mut request = object([("body", body), ("parent_id", nullable(identity()))], &["body"]);
  request["x-max-lifetime-comments"] = json!(MAX_ISSUE_DISCUSSION_COMMENTS);
  schemas.insert("AppendIssueCommentRequest".into(), request);
  schemas.insert(
    "IssueCommentMetadata".into(),
    object(
      [
        ("id", identity()),
        ("project_id", identity()),
        ("issue_id", identity()),
        (
          "sequence",
          json!({"type":"integer","minimum":1,"maximum":MAX_ISSUE_DISCUSSION_COMMENTS}),
        ),
        ("parent_id", nullable(identity())),
        ("actor", schema_ref("AuditActor")),
        ("created_at", non_negative_integer()),
        ("body_digest", schema_ref("FactoryDigest")),
        (
          "body_bytes",
          json!({"type":"integer","minimum":1,"maximum":MAX_ISSUE_COMMENT_BODY_BYTES}),
        ),
      ],
      &[
        "id",
        "project_id",
        "issue_id",
        "sequence",
        "parent_id",
        "actor",
        "created_at",
        "body_digest",
        "body_bytes",
      ],
    ),
  );
  schemas.insert(
    "IssueCommentOutcome".into(),
    object(
      [
        ("disposition", schema_ref("MutationDisposition")),
        ("comment", schema_ref("IssueCommentMetadata")),
      ],
      &["disposition", "comment"],
    ),
  );
  schemas.insert(
    "IssueCommentContent".into(),
    object(
      [
        ("comment", schema_ref("IssueCommentMetadata")),
        ("body", schema_ref("IssueCommentBody")),
      ],
      &["comment", "body"],
    ),
  );
}
