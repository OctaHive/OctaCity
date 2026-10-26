//! Safe preview/reporting representations exclude raw inputs, handles and private progress.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let id = || json!({"type":"string","format":"uuid"});
  let digest = || reference("FactoryDigest");
  let time = || json!({"type":"integer","format":"int64"});
  let nullable = |value: Value| json!({"anyOf":[value,{"type":"null"}]});
  let failure = json!({"type":"string","enum":["unavailable","rate_limited","transient","timeout","cancelled","admission_capacity","retention_pending","protocol_fault","invalid_configuration","conflict"]});
  let issue = object(
    json!({"project_id":id(),"issue_id":text(256)}),
    &["project_id", "issue_id"],
  );
  let selection = object(
    json!({"input_id":id(),"reasons":array(json!({"type":"string","enum":["state_mismatch","required_label_missing","alternative_label_missing","excluded_label_present","predicate_mismatch"]}),5)}),
    &["input_id", "reasons"],
  );
  let item = object(json!({"resource":text(256),"item":text(256)}), &["resource", "item"]);
  let summary = object(
    json!({"item":item.clone(),"display_id":text(256),"title":text(512),"state":text(128),"labels":array(text(256),64),"selections":array(selection,64)}),
    &["item", "display_id", "title", "state", "labels", "selections"],
  );
  schemas.insert(
    "ProjectIssueSourcePreviewSubmission".into(),
    object(
      json!({"connection_id":id(),"connection_version":{"type":"integer","minimum":1},
      "access_version":{"type":"integer","minimum":1},"source":reference("WorkSourceBinding"),
      "limit":{"type":"integer","minimum":1,"maximum":octacity_server_application::MAX_WORK_SOURCE_PREVIEW_ITEMS}}),
      &[
        "connection_id",
        "connection_version",
        "access_version",
        "source",
        "limit",
      ],
    ),
  );
  let task_summary = object(
    json!({"item":item.clone(),"display_id":text(256),"title":text(512),"state":text(128),"labels":array(text(256),64)}),
    &["item", "display_id", "title", "state", "labels"],
  );
  schemas.insert("ProjectIssueSourcePreview".into(), object(
    json!({"source":reference("WorkSourceBinding"),"observed_at_unix_ms":time(),"failure":nullable(failure.clone()),
      "retry_after_millis":nullable(json!({"type":"integer","minimum":0,"maximum":octacity_server_application::MAX_WORK_SOURCE_PREVIEW_RETRY_MILLIS})),
      "items":array(task_summary,usize::from(octacity_server_application::MAX_WORK_SOURCE_PREVIEW_ITEMS)),"has_more":{"type":"boolean"}}),
    &["source","observed_at_unix_ms","failure","retry_after_millis","items","has_more"],
  ));
  let import_definition = object(
    json!({"source":reference("WorkSourceBinding"),
    "connection":reference("WorkSourceConnectionReference"),"predicate":reference("WorkSourcePredicate"),"enabled":{"type":"boolean"}}),
    &["source", "connection", "enabled"],
  );
  let import_configuration = object(
    json!({"id":id(),"project_id":id(),"version":{"type":"integer","minimum":1},
    "definition":import_definition.clone(),"published_at":time()}),
    &["id", "project_id", "version", "definition", "published_at"],
  );
  schemas.insert("ProjectIssueSourceImportConfiguration".into(), import_configuration);
  schemas.insert(
    "ProjectIssueSourceImportSubmission".into(),
    object(
      json!({"intent":object(
    json!({"id":id(),"project_id":id(),"expected_current_version":nullable(json!({"type":"integer","minimum":1})),
      "definition":import_definition}),&["id","project_id","expected_current_version","definition"])}),
      &["intent"],
    ),
  );
  schemas.insert(
    "ProjectIssueSourceImportMutation".into(),
    object(
      json!({"disposition":{"type":"string","enum":["applied","replayed"]},
    "configuration":reference("ProjectIssueSourceImportConfiguration")}),
      &["disposition", "configuration"],
    ),
  );
  schemas.insert(
    "ProjectIssueSourceImportResource".into(),
    object(
      json!({"configuration":reference("ProjectIssueSourceImportConfiguration")}),
      &["configuration"],
    ),
  );
  schemas.insert(
    "ProjectIssueSourceImportPage".into(),
    object(
      json!({"items":array(reference("ProjectIssueSourceImportConfiguration"),usize::from(octacity_server_application::MAX_INTEGRATION_DISCOVERY_PAGE_SIZE)),
    "next_cursor":nullable(id())}),
      &["items", "next_cursor"],
    ),
  );
  let association = object(
    json!({"id":digest(),"native_correlation":digest(),"input_id":id(),"input_version":{"type":"integer","minimum":1},"work_id":id(),"run_id":id()}),
    &[
      "id",
      "native_correlation",
      "input_id",
      "input_version",
      "work_id",
      "run_id",
    ],
  );
  schemas.insert("WorkSourceReportingRecord".into(), object(
    json!({"definition":reference("FactoryFlowDefinitionReference"),"flow_run_id":id(),"cycle_id":id(),"node":text(128),"outcome":text(128),"node_attempt_id":id()}),
    &["definition","flow_run_id","cycle_id","node","outcome","node_attempt_id"],
  ));
  schemas.insert("WorkSourceReportingOrigin".into(), object(
    json!({"action":reference("WorkSourceReportingRecord"),"stages":array(reference("WorkSourceReportingRecord"),octacity_server_application::factory_configuration_document_bounds().schema_entries)}),
    &["action","stages"],
  ));
  let reporting = object(
    json!({"operation_id":digest(),"state":{"type":"string","enum":["pending","claimed","applied","exhausted"]},"failure":{"type":"string","enum":["forbidden","unsupported","exhausted","conflict"]},"attempt":{"type":"integer","minimum":0},"available_at":time(),"origin":reference("WorkSourceReportingOrigin")}),
    &["operation_id", "state", "attempt", "available_at"],
  );
  let admission = object(
    json!({"association":association,"reporting":array(reference("WorkSourceReportingDiagnostic"),usize::from(octacity_server_application::MAX_WORK_SOURCE_REPORTING_OPERATIONS)),"reporting_truncated":{"type":"boolean"},"issue":reference("WorkSourceIssueLink")}),
    &["association", "reporting", "reporting_truncated"],
  );
  let candidate = object(
    json!({"native_correlation":digest(),"summary":summary,"disposition":{"type":"string","enum":["filtered","selected","ambiguous"]},"admissions":array(reference("WorkSourceAdmissionDiagnostic"),64),"issue":reference("WorkSourceIssueLink")}),
    &["native_correlation", "summary", "disposition", "admissions"],
  );
  let scan = object(
    json!({"started_at":time(),"window":nullable(json!({"type":"array","items":time(),"minItems":2,"maxItems":2})),"reconciliation":{"type":"boolean"},"schedule":nullable(object(json!({"retry_at":time(),"attempts":{"type":"integer","minimum":0},"failure":nullable(failure.clone()),"exhausted":{"type":"boolean"}}),&["retry_at","attempts","failure","exhausted"])),"selection":nullable(object(json!({"item":item,"matching_inputs":array(id(),64)}),&["item","matching_inputs"]))}),
    &["started_at", "window", "reconciliation", "schedule", "selection"],
  );
  let selectors = array(
    object(
      json!({"input_id":id(),"binding":reference("WorkSourceBinding"),"selection":nullable(reference("WorkSourceSelectionLabels"))}),
      &["input_id", "binding", "selection"],
    ),
    64,
  );
  for (name, value) in [
    ("WorkSourceReportingDiagnostic", reporting),
    ("WorkSourceIssueLink", issue),
    ("WorkSourceAdmissionDiagnostic", admission),
    (
      "WorkSourcePreviewSubmission",
      object(
        json!({"intent":reference("WorkSourceConfigurationIntent"),"limit":{"type":"integer","minimum":1,"maximum":100}}),
        &["intent", "limit"],
      ),
    ),
    (
      "WorkSourcePreview",
      object(
        json!({"configuration":reference("WorkSourceConfigurationIntent"),"selectors":selectors.clone(),"observed_at_unix_ms":time(),"failure":nullable(failure),"retry_after_millis":nullable(json!({"type":"integer","minimum":0,"maximum":octacity_server_application::MAX_WORK_SOURCE_PREVIEW_RETRY_MILLIS})),"candidates":array(candidate,100),"has_more":{"type":"boolean"},"admissions_truncated":{"type":"boolean"}}),
        &[
          "configuration",
          "selectors",
          "observed_at_unix_ms",
          "failure",
          "retry_after_millis",
          "candidates",
          "has_more",
          "admissions_truncated",
        ],
      ),
    ),
    (
      "WorkSourceDiagnostics",
      object(
        json!({"configuration":reference("WorkSourceConfigurationRecord"),"selectors":selectors,"scan":nullable(scan),"admissions":array(reference("WorkSourceAdmissionDiagnostic"),64),"next_cursor":nullable(text(512))}),
        &["configuration", "selectors", "scan", "admissions", "next_cursor"],
      ),
    ),
  ] {
    schemas.insert(name.into(), value);
  }
}
