//! Input settings remain provider-owned immutable references, separate from connection profiles.
use super::*;
pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let pin = || reference("FactoryImmutableReferenceResource");
  let id = || json!({"type":"string","format":"uuid"});
  let version = || json!({"type":"integer","minimum":1});
  let optional_version = || json!({"type":["integer","null"],"minimum":1});
  let nullable = |value: Value| json!({"anyOf":[value,{"type":"null"}]});
  let binding = object(
    json!({"installation":pin(),"capability":pin(),"configuration":pin(),"resource":pin()}),
    &["installation", "capability", "configuration", "resource"],
  );
  let connection = object(
    json!({"id":id(),"version":version(),"access_version":version(),"source_configuration":pin(),"reporter_configuration":nullable(pin())}),
    &[
      "id",
      "version",
      "access_version",
      "source_configuration",
      "reporter_configuration",
    ],
  );
  let repository = object(
    json!({"project_id":id(),"id":id(),"version":version(),"definition_digest":reference("FactoryDigest")}),
    &["project_id", "id", "version", "definition_digest"],
  );
  let draft = object(
    json!({"connection":nullable(reference("WorkSourceConnectionReference")),"target":reference("FactoryConfigurationReference"),"repository":repository,"source":reference("WorkSourceBinding"),"reporter":nullable(reference("WorkSourceBinding")),"enabled":{"type":"boolean","default":false}}),
    &["target", "repository", "source"],
  );
  let input_intent = object(
    json!({"id":id(),"expected_version":optional_version(),"definition":reference("WorkSourceInputDraft")}),
    &["id", "definition"],
  );
  let group_intent = object(
    json!({"id":id(),"expected_version":optional_version(),"members":array(object(json!({"id":id(),"version":version()}),&["id","version"]),64)}),
    &["id", "members"],
  );
  let intent = object(
    json!({"project_id":id(),"inputs":array(reference("WorkSourceInputIntent"),64),"group":nullable(reference("WorkSourceGroupIntent"))}),
    &["project_id", "inputs"],
  );
  let input = object(
    json!({"id":id(),"version":version(),"project_id":id(),"root_flow":reference("FactoryFlowDefinitionReference"),"definition":reference("WorkSourceInputDraft")}),
    &["id", "version", "project_id", "root_flow", "definition"],
  );
  let group = object(
    json!({"id":id(),"version":version(),"project_id":id(),"members":array(reference("WorkSourceInput"),64)}),
    &["id", "version", "project_id", "members"],
  );
  let publication = object(
    json!({"project_id":id(),"inputs":array(object(json!({"input":reference("WorkSourceInput"),"expected_version":optional_version()}),&["input","expected_version"]),64),"group":nullable(object(json!({"group":reference("WorkSourceGroup"),"expected_version":optional_version()}),&["group","expected_version"]))}),
    &["project_id", "inputs", "group"],
  );
  let record = json!({"oneOf":[
    object(json!({"kind":{"const":"input"},"definition":reference("WorkSourceInput")}),&["kind","definition"]),
    object(json!({"kind":{"const":"group"},"definition":reference("WorkSourceGroup")}),&["kind","definition"])
  ],"discriminator":{"propertyName":"kind"}});
  let labels = object(
    json!({"state":text(128),"all_of":array(text(octacity_server_application::MAX_SOURCE_SELECTION_LABEL_BYTES),octacity_server_application::MAX_SOURCE_SELECTION_LABELS),"any_of":array(text(octacity_server_application::MAX_SOURCE_SELECTION_LABEL_BYTES),octacity_server_application::MAX_SOURCE_SELECTION_LABELS),"none_of":array(text(octacity_server_application::MAX_SOURCE_SELECTION_LABEL_BYTES),octacity_server_application::MAX_SOURCE_SELECTION_LABELS)}),
    &["state", "all_of", "any_of", "none_of"],
  );
  let choices = object(
    json!({"connection_id":id(),"connection_version":version(),"access_version":version(),"items":array(object(json!({"binding":reference("WorkSourceBinding"),"connection_configuration":pin(),"purpose":{"type":"string","enum":["task_read","task_update"]},"native_id":text(1024),"locator":text(1024),"selection":nullable(labels.clone())}),&["binding","connection_configuration","purpose","native_id","locator","selection"]),256)}),
    &["connection_id", "connection_version", "access_version", "items"],
  );
  for (name, value) in [
    ("WorkSourceSelectionLabels", labels),
    ("WorkSourceBinding", binding),
    ("WorkSourceConnectionReference", connection),
    ("WorkSourceInputDraft", draft),
    ("WorkSourceInputIntent", input_intent),
    ("WorkSourceGroupIntent", group_intent),
    ("WorkSourceConfigurationIntent", intent),
    (
      "WorkSourceConfigurationSubmission",
      object(
        json!({"intent":reference("WorkSourceConfigurationIntent")}),
        &["intent"],
      ),
    ),
    ("WorkSourceInput", input),
    ("WorkSourceGroup", group),
    ("WorkSourcePublication", publication),
    ("WorkSourceConfigurationRecord", record),
    (
      "WorkSourceConfigurationValidation",
      object(
        json!({"publication":reference("WorkSourcePublication")}),
        &["publication"],
      ),
    ),
    (
      "WorkSourceConfigurationMutation",
      object(
        json!({"disposition":{"type":"string","enum":["applied","replayed"]},"publication":reference("WorkSourcePublication"),"published_at_unix_ms":{"type":"integer","minimum":0}}),
        &["disposition", "publication", "published_at_unix_ms"],
      ),
    ),
    (
      "WorkSourceConfigurationPage",
      object(
        json!({"items":array(reference("WorkSourceConfigurationRecord"),usize::from(octacity_server_application::MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE)),"next_cursor":{"type":["string","null"],"maxLength":256}}),
        &["items", "next_cursor"],
      ),
    ),
    ("WorkSourceInputChoices", choices),
  ] {
    schemas.insert(name.into(), value);
  }
}
