//! Retained attribution and derived diagnostics, never writable verification authority.
use super::super::*;
use super::factory::{bounded_array, tagged};
use octacity_server_application::{
  MAX_ISSUE_CLASSIFICATION_FIELDS, MAX_ISSUE_CLASSIFICATION_REFERENCES, MAX_ISSUE_MATERIALS,
  MAX_ISSUE_PROPERTY_KEY_BYTES,
};

fn id() -> Value {
  json!({"type":"string","format":"uuid"})
}
fn key() -> Value {
  json!({"type":"string","minLength":1,"maxLength":MAX_ISSUE_PROPERTY_KEY_BYTES,"pattern":"^[a-z0-9_]+$"})
}
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  schemas.insert(
    "IssueClassificationFieldReference".into(),
    json!({"oneOf":[
      tagged("kind","priority",[("definition_version",non_negative_integer())],&[]),
      // Missing built-ins in an empty catalog are reviewed at version zero;
      // committed component attributions still require a positive publication.
      tagged("kind","component",[("definition_version",non_negative_integer())],&["definition_version"]),
      tagged("kind","type",[("definition_version",non_negative_integer())],&["definition_version"]),
      tagged("kind","size",[],&[]),
      tagged("kind","property",[("id",key()),("definition_version",positive_integer())],&["id","definition_version"]),
      tagged("kind","dependencies",[],&[])
    ]}),
  );
  schemas.insert("IssueClassificationOrigin".into(), json!({"oneOf":[
    tagged("kind","manual",[("actor",schema_ref("AuditActor"))],&["actor"]),
    tagged("kind","pipeline",[
      ("actor",schema_ref("AuditActor")),("work_id",id()),("run_id",id()),
      ("record",schema_ref("FactoryDigest")),("policy",super::factory::key()),("operation_id",schema_ref("FactoryDigest"))
    ],&["actor","work_id","run_id","record","policy","operation_id"]),
    tagged("kind","source",[
      ("actor",schema_ref("AuditActor")),("mapping",schema_ref("FactoryImmutableReferenceResource")),
      ("observation",schema_ref("FactoryDigest")),("native_correlation",schema_ref("FactoryDigest"))
    ],&["actor","mapping","observation","native_correlation"])
  ]}));
  schemas.insert(
    "IssueClassificationSubject".into(),
    json!({"oneOf":[
      tagged("kind","exact",[("value",schema_ref("FactoryExactSubject"))],&["value"]),
      tagged("kind","candidate",[("value",schema_ref("FactoryCandidateSubject"))],&["value"])
    ]}),
  );
  schemas.insert(
    "IssueClassificationAssessmentInputs".into(),
    object(
      [
        ("subject", schema_ref("IssueClassificationSubject")),
        (
          "materials",
          bounded_array(schema_ref("IssueMaterialReference"), MAX_ISSUE_MATERIALS),
        ),
      ],
      &[],
    ),
  );
  schemas.insert(
    "IssueClassificationAttribution".into(),
    object(
      [
        ("field", schema_ref("IssueClassificationFieldReference")),
        ("origin", schema_ref("IssueClassificationOrigin")),
        ("assessed_version", positive_integer()),
        ("requirements_version", positive_integer()),
        ("assessed_at", non_negative_integer()),
        (
          "evidence",
          bounded_array(schema_ref("FactoryDigest"), MAX_ISSUE_CLASSIFICATION_REFERENCES),
        ),
        ("inputs", schema_ref("IssueClassificationAssessmentInputs")),
      ],
      &[
        "field",
        "origin",
        "assessed_version",
        "requirements_version",
        "assessed_at",
      ],
    ),
  );
  schemas.insert(
    "IssueClassificationEvidenceReference".into(),
    object(
      [
        ("work_id", id()),
        ("run_id", id()),
        ("record", schema_ref("FactoryDigest")),
        ("subject", schema_ref("IssueClassificationSubject")),
        ("environment", schema_ref("FactoryImmutableReferenceResource")),
        ("criterion", schema_ref("FactoryImmutableReferenceResource")),
        ("observed_at", non_negative_integer()),
        ("fresh_until", non_negative_integer()),
      ],
      &[
        "work_id",
        "run_id",
        "record",
        "subject",
        "environment",
        "criterion",
        "observed_at",
        "fresh_until",
      ],
    ),
  );
  schemas.insert(
    "IssueClassificationDependencyReference".into(),
    object(
      [
        ("work_id", id()),
        ("run_id", id()),
        ("work_digest", schema_ref("FactoryDigest")),
        ("terminal", super::factory::key()),
      ],
      &["work_id", "run_id", "work_digest", "terminal"],
    ),
  );
  let mut metadata = object(
    [
      (
        "fields",
        bounded_array(
          schema_ref("IssueClassificationAttribution"),
          MAX_ISSUE_CLASSIFICATION_FIELDS,
        ),
      ),
      (
        "evidence",
        bounded_array(
          schema_ref("IssueClassificationEvidenceReference"),
          MAX_ISSUE_CLASSIFICATION_REFERENCES,
        ),
      ),
      (
        "dependencies",
        bounded_array(
          schema_ref("IssueClassificationDependencyReference"),
          MAX_ISSUE_CLASSIFICATION_REFERENCES,
        ),
      ),
    ],
    &["fields"],
  );
  metadata["description"] = json!(
    "Committed Issue attribution only; scalar values stay in content/planning. Safe producer/evidence pins do not grant content access or imply that a proposed result was applied. Omitted for unclassified legacy Issues."
  );
  schemas.insert("IssueClassificationMetadata".into(), metadata);
  schemas.get_mut("IssueResource").unwrap()["properties"]["classification"] = schema_ref("IssueClassificationMetadata");

  let mut problems: Vec<_> = [
    "missing_value",
    "missing_assessment",
    "definition_unavailable",
    "requirements_changed",
    "code_subject_changed",
    "code_subject_unavailable",
    "evidence_missing",
    "unsupported_reproduction",
  ]
  .map(|code| tagged("code", code, [], &[]))
  .into();
  problems.push(tagged(
    "code",
    "material_changed",
    [("reference", schema_ref("IssueMaterialReference"))],
    &["reference"],
  ));
  problems.extend(
    [
      "evidence_unavailable",
      "evidence_expired",
      "evidence_subject_changed",
      "evidence_subject_unavailable",
      "evidence_claim_conflict",
      "evidence_inconclusive",
    ]
    .map(|code| tagged("code", code, [("record", schema_ref("FactoryDigest"))], &["record"])),
  );
  // Each bounded source can contribute at most one problem of each declared kind.
  let maximum_problems = problems.len() * (1 + MAX_ISSUE_CLASSIFICATION_REFERENCES + MAX_ISSUE_MATERIALS);
  schemas.insert("IssueClassificationProblem".into(), json!({"oneOf":problems}));
  let problems = || bounded_array(schema_ref("IssueClassificationProblem"), maximum_problems);
  schemas.insert(
    "IssueClassificationEvidenceReview".into(),
    object(
      [("record", schema_ref("FactoryDigest")), ("problems", problems())],
      &["record", "problems"],
    ),
  );
  schemas.insert(
    "IssueClassificationFieldReview".into(),
    object(
      [
        ("field", schema_ref("IssueClassificationFieldReference")),
        ("problems", problems()),
        ("requires_verified_evidence", boolean()),
        (
          "evidence",
          bounded_array(
            schema_ref("IssueClassificationEvidenceReview"),
            MAX_ISSUE_CLASSIFICATION_REFERENCES,
          ),
        ),
      ],
      &["field", "problems", "requires_verified_evidence", "evidence"],
    ),
  );
  let mut review = object(
    [
      ("version", positive_integer()),
      (
        "fields",
        bounded_array(
          schema_ref("IssueClassificationFieldReview"),
          MAX_ISSUE_CLASSIFICATION_FIELDS,
        ),
      ),
    ],
    &["version", "fields"],
  );
  review["description"] = json!(
    "Read-only applicability of this exact Issue revision, not an execution readiness verdict. No repository target is selected here: code-dependent assessments report unresolved subject diagnostics. Checks use server time and independently authorized evidence; reads never renew attribution or start execution."
  );
  schemas.insert("IssueClassificationReview".into(), review);
}
