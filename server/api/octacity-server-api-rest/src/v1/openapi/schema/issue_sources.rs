//! Closed safe Source metadata; native vocabulary stays in installed capabilities.
use super::super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let pin = || schema_ref("FactoryImmutableReferenceResource");
  let native = || json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_EXTERNAL_WORK_IDENTITY_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_EXTERNAL_WORK_IDENTITY_BYTES});
  let continuation = || {
    nullable(
      json!({"type":"string","minLength":1,"maxLength":octacity_server_application::MAX_SOURCE_POLL_CURSOR_BYTES,"x-max-utf8-bytes":octacity_server_application::MAX_SOURCE_POLL_CURSOR_BYTES}),
    )
  };
  schemas.insert(
    "SetIssueSourcePullRequest".into(),
    object(
      [("expected_generation", positive_integer()), ("enabled", boolean())],
      &["expected_generation", "enabled"],
    ),
  );
  schemas.insert(
    "IssueSourcePullOutcome".into(),
    object(
      [("generation", positive_integer()), ("enabled", boolean())],
      &["generation", "enabled"],
    ),
  );
  schemas.insert(
    "RefreshIssueSourceRequest".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("continuation", continuation()),
      ],
      &["source"],
    ),
  );
  schemas.insert(
    "IssueSourceRefreshOutcome".into(),
    object(
      [
        ("issue_id", id()),
        (
          "disposition",
          string_enum(&[
            "scanning",
            "read_profile_mismatch",
            "pull_disabled",
            "fresh",
            "unchanged",
            "duplicate",
            "stale",
            "uncertain",
          ]),
        ),
        ("status", schema_ref("IssueSourceStatusOutcome")),
      ],
      &["issue_id", "disposition"],
    ),
  );
  schemas.insert(
    "PreviewIssueSourceRequest".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("scan_id", nullable(schema_ref("FactoryDigest"))),
        ("continuation", continuation()),
      ],
      &["source"],
    ),
  );
  schemas.insert("IssueSourceCandidates".into(),object([
    ("scan_id",schema_ref("FactoryDigest")),("items",json!({"type":"array","maxItems":octacity_server_application::MAX_SOURCE_POLL_PAGE_ITEMS,"items":schema_ref("IssueSourceItem")})),
    ("next_cursor",continuation())
  ], &["scan_id","items","next_cursor"]));
  schemas.insert(
    "IssueSourceItem".into(),
    object([("resource", native()), ("item", native())], &["resource", "item"]),
  );
  schemas.insert(
    "IssueSourceSelection".into(),
    object(
      [
        ("project_id", id()),
        ("source", schema_ref("WorkSourceBinding")),
        ("item", schema_ref("IssueSourceItem")),
      ],
      &["project_id", "source", "item"],
    ),
  );
  schemas.insert(
    "IssueSourceLink".into(),
    object(
      [
        ("issue_id", id()),
        ("selection", schema_ref("IssueSourceSelection")),
        ("native_correlation", schema_ref("FactoryDigest")),
        ("linked_at", non_negative_integer()),
      ],
      &["issue_id", "selection", "native_correlation", "linked_at"],
    ),
  );
  schemas.insert(
    "IssueSourceLinkResponse".into(),
    nullable(schema_ref("IssueSourceLink")),
  );
  schemas.insert("IssueSourceOutcome".into(),json!({"oneOf":[
    object([("kind",json!({"const":"linked"})),("link",schema_ref("IssueSourceLink")),("disposition",string_enum(&["applied","replayed"]))], &["kind","link","disposition"]),
    object([("kind",json!({"const":"conflict"})),("canonical",nullable(schema_ref("IssueSourceLink")))], &["kind","canonical"])
  ],"discriminator":{"propertyName":"kind"}}));
  schemas.insert(
    "AttachIssueSourceRequest".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("item", schema_ref("IssueSourceItem")),
        ("expected_field_policy", schema_ref("IssueFieldPolicy")),
        ("scan_id", nullable(schema_ref("FactoryDigest"))),
        ("continuation", continuation()),
      ],
      &["source", "item", "expected_field_policy"],
    ),
  );
  schemas.insert(
    "IssueSourceObservation".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("plugin", pin()),
        ("provider_configuration", schema_ref("FactoryDigest")),
        ("selection_digest", schema_ref("FactoryDigest")),
        ("snapshot_digest", schema_ref("FactoryDigest")),
        ("observed_at", non_negative_integer()),
        ("expires_at", non_negative_integer()),
      ],
      &[
        "source",
        "plugin",
        "provider_configuration",
        "selection_digest",
        "snapshot_digest",
        "observed_at",
        "expires_at",
      ],
    ),
  );
  schemas.insert(
    "IssueSourceStatusProvenance".into(),
    object(
      [
        ("mapping", pin()),
        ("observation", schema_ref("FactoryDigest")),
        ("native_correlation", schema_ref("FactoryDigest")),
      ],
      &["mapping", "observation", "native_correlation"],
    ),
  );
  schemas.insert(
    "IssueSourceStatusOutcome".into(),
    object(
      [
        ("provenance", schema_ref("IssueSourceStatusProvenance")),
        (
          "condition",
          string_enum(&[
            "applied",
            "echo",
            "unmapped",
            "ambiguous",
            "workflow_mismatch",
            "forbidden_transition",
            "evidence_required",
            "not_fresh",
            "unmanaged",
          ]),
        ),
      ],
      &["provenance", "condition"],
    ),
  );
  schemas.insert(
    "IssueSourceRefreshState".into(),
    object(
      [
        ("enabled", boolean()),
        ("generation", positive_integer()),
        ("accepted", schema_ref("IssueSourceObservation")),
        ("pending", nullable(schema_ref("IssueSourceObservation"))),
        ("mapping", pin()),
        ("status", schema_ref("IssueSourceStatusOutcome")),
      ],
      &["enabled", "generation", "accepted", "pending"],
    ),
  );
  let fields = schemas["IssueFieldPolicy"]["properties"]["source_owned"]["items"].clone();
  let field_list = || json!({"type":"array","maxItems":fields["enum"].as_array().expect("closed fields").len(),"uniqueItems":true,"items":fields});
  schemas.insert(
    "IssueSourceWriterChoice".into(),
    object(
      [
        ("binding", schema_ref("WorkSourceBinding")),
        ("effects", schema_ref("WorkSourceReportingPolicy")),
      ],
      &["binding", "effects"],
    ),
  );
  schemas.insert(
    "IssueSourceChoice".into(),
    object(
      [
        ("source", schema_ref("WorkSourceBinding")),
        ("native_resource", native()),
        ("readable", schema_ref("WorkSourceReportingPolicy")),
        ("pull_fields", field_list()),
        ("push_fields", field_list()),
        ("writer", nullable(schema_ref("IssueSourceWriterChoice"))),
      ],
      &[
        "source",
        "native_resource",
        "readable",
        "pull_fields",
        "push_fields",
        "writer",
      ],
    ),
  );
  schemas.insert("IssueSourceChoices".into(),object([
    ("issue_version",positive_integer()),
    ("items",json!({"type":"array","maxItems":octacity_server_application::MAX_FACTORY_EXECUTOR_BINDINGS,"items":schema_ref("IssueSourceChoice")}))
  ], &["issue_version","items"]));
  schemas.insert(
    "IssueSourceFieldAuthority".into(),
    string_enum(&["source_owned_pull", "octacity_owned_push", "unsynchronized"]),
  );
  schemas.insert("IssueSourceMappingDefinition".into(),object([
    ("source",schema_ref("WorkSourceBinding")),("reporter",nullable(schema_ref("WorkSourceBinding"))),
    ("push_enabled",boolean()),
    ("fields",json!({"type":"object","maxProperties":fields["enum"].as_array().expect("closed Issue fields").len(),"propertyNames":fields,"additionalProperties":schema_ref("IssueSourceFieldAuthority")})),
    ("status",nullable(schema_ref("IssueSourceStatusMapping")))
  ], &["source","reporter","push_enabled","fields","status"]));
  schemas
    .get_mut("IssueSourceMappingDefinition")
    .expect("inserted mapping definition")["x-max-utf8-bytes"] =
    json!(octacity_server_application::MAX_ISSUE_SOURCE_MAPPING_BYTES);
  schemas.insert("IssueSourceStatusMapping".into(),object([
    ("workflow",schema_ref("IssueWorkflowPin")),
    ("pull",json!({"type":"array","maxItems":octacity_server_application::MAX_SOURCE_REPORTING_STATUSES,"items":object([("native",schema_ref("WorkSourcePredicate")),("status_id",id())], &["native","status_id"])})),
    ("push",nullable(schema_ref("WorkSourceStatusMapping")))
  ], &["workflow","pull","push"]));
  schemas.insert(
    "PublishedIssueSourceMapping".into(),
    object(
      [
        ("project_id", id()),
        ("issue_id", id()),
        ("version", positive_integer()),
        ("definition", schema_ref("IssueSourceMappingDefinition")),
        ("published_at", non_negative_integer()),
      ],
      &["project_id", "issue_id", "version", "definition", "published_at"],
    ),
  );
  schemas.insert(
    "PublishIssueSourceMappingRequest".into(),
    object(
      [
        ("expected_mapping", nullable(pin())),
        ("definition", schema_ref("IssueSourceMappingDefinition")),
      ],
      &["expected_mapping", "definition"],
    ),
  );
  schemas.insert(
    "IssueSourceMappingMutationResponse".into(),
    object(
      [
        ("disposition", string_enum(&["applied", "replayed"])),
        ("resource", schema_ref("PublishedIssueSourceMapping")),
        ("mapping_reference", pin()),
      ],
      &["disposition", "resource", "mapping_reference"],
    ),
  );
  schemas.insert(
    "IssueSynchronization".into(),
    object(
      [
        ("issue_version", positive_integer()),
        ("workflow", schema_ref("IssueWorkflowPin")),
        ("field_policy", schema_ref("IssueFieldPolicy")),
        ("link", nullable(schema_ref("IssueSourceLink"))),
        ("mapping", nullable(schema_ref("PublishedIssueSourceMapping"))),
        ("mapping_reference", nullable(pin())),
        ("observation", nullable(schema_ref("IssueSourceRefreshState"))),
      ],
      &[
        "issue_version",
        "workflow",
        "field_policy",
        "link",
        "mapping",
        "mapping_reference",
        "observation",
      ],
    ),
  );
}
