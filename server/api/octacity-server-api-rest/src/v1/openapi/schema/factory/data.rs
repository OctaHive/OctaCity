use super::*;

pub(super) fn insert(schemas: &mut Map<String, Value>) {
  let bounds = octacity_server_application::factory_configuration_document_bounds();
  schemas.insert(
    "FactoryFlowDataSchema".to_owned(),
    object(
      [
        ("reference", schema_ref("FactoryImmutableReferenceResource")),
        ("shape", schema_ref("FactoryFlowValueSchema")),
      ],
      &["reference", "shape"],
    ),
  );
  schemas.insert(
    "FactoryFlowFieldSchema".to_owned(),
    object(
      [("required", boolean()), ("value", schema_ref("FactoryFlowValueSchema"))],
      &["required", "value"],
    ),
  );
  schemas.insert("FactoryFlowValueSchema".to_owned(),json!({"x-max-depth":bounds.schema_depth,"oneOf":[
    tagged("type","boolean",[],&[]),
    tagged("type","string",[("min_bytes",non_negative_integer()),("max_bytes",bounded_integer(0,bounds.value_bytes as u64))],&["min_bytes","max_bytes"]),
    tagged("type","integer",[("minimum",integer()),("maximum",integer())],&["minimum","maximum"]),
    tagged("type","object",[("fields",bounded_map(schema_ref("FactoryFlowFieldSchema"),bounds.schema_entries))],&["fields"]),
    tagged("type","array",[("item",schema_ref("FactoryFlowValueSchema")),("min_items",non_negative_integer()),("max_items",bounded_integer(0,bounds.schema_items.into()))],&["item","min_items","max_items"]),
    tagged("type","enum",[("values",bounded_array(json!({"type":["string","integer","boolean","null"]}),bounds.schema_entries))],&["values"]),
    tagged("type","externally_tagged",[("variants",json!({
      "type":"object","minProperties":1,"maxProperties":bounds.schema_entries,
      "propertyNames":{"type":"string","minLength":1,"maxLength":bounds.schema_variant_name_bytes,"x-max-utf8-bytes":bounds.schema_variant_name_bytes},
      "additionalProperties":{"oneOf":[{"type":"null"},schema_ref("FactoryFlowValueSchema")]}
    }))],&["variants"]),
  ]}));
  schemas.insert(
    "FactoryFlowDataMapping".to_owned(),
    object([("value", schema_ref("FactoryFlowValueMapping"))], &["value"]),
  );
  schemas.insert("FactoryFlowValueMapping".to_owned(),json!({"x-max-depth":bounds.schema_depth,"oneOf":[
    tagged("mapping","pointer",[("path",json!({"type":"string"}))],&["path"]),
    tagged("mapping","constant",[("value",json!({}))],&["value"]),
    tagged("mapping","object",[("fields",bounded_map(schema_ref("FactoryFlowValueMapping"),bounds.schema_entries))],&["fields"]),
    tagged("mapping","array",[("items",bounded_array(schema_ref("FactoryFlowValueMapping"),bounds.schema_items.into()))],&["items"]),
    tagged("mapping","if_present",[("path",json!({"type":"string"})),("then",schema_ref("FactoryFlowValueMapping")),("otherwise",schema_ref("FactoryFlowValueMapping"))],&["path","then","otherwise"]),
  ]}));
  schemas.insert("FactoryFlowInputSource".to_owned(),json!({"oneOf":[
    tagged("source","node_configuration",[("path",json!({"type":"string"}))],&["path"]),
    tagged("source","caller_input",[("path",json!({"type":"string"}))],&["path"]),
    tagged("source","predecessor",[("view",schema_ref("FactoryFlowRecordView")),("path",json!({"type":"string"}))],&["path"]),
    tagged("source","incoming",[("schema",schema_ref("FactoryImmutableReferenceResource")),("path",json!({"type":"string"}))],&["schema","path"]),
    tagged("source","work",[("path",json!({"type":"string"}))],&["path"]),
    tagged("source","record",[("node",key()),("outcome",key()),("view",schema_ref("FactoryFlowRecordView")),("path",json!({"type":"string"}))],&["node","outcome","path"]),
  ]}));
  schemas.insert(
    "FactoryFlowRecordView".to_owned(),
    json!({"type":"string","enum":["payload","metadata"]}),
  );
  schemas.insert(
    "FactoryFlowInputBinding".to_owned(),
    object(
      [
        (
          "sources",
          bounded_map(schema_ref("FactoryFlowInputSource"), bounds.schema_entries),
        ),
        ("mapping", schema_ref("FactoryFlowDataMapping")),
      ],
      &["sources", "mapping"],
    ),
  );
}
