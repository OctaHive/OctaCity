use super::*;
use octacity_protocol::{
  FactoryImmutableReferenceV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3, FactoryToolActionProposalV3,
  FactoryToolPathAccessV3, FactoryToolPathV3,
};

#[test]
fn an_installed_structured_mcp_read_recipe_binds_the_exact_protected_operation() {
  let tool = reference("operator-read-tool");
  let executable = reference("operator-read-executable");
  let resources = FactoryResourceLimitsV3 {
    cpu_millis: 100,
    memory_bytes: 4096,
    disk_bytes: 4096,
    process_count: 2,
    elapsed_millis: 1000,
  };
  let outputs = FactoryOutputPermissionsV3 {
    kinds: vec![],
    max_artifact_count: 0,
    max_artifact_bytes: 0,
    max_report_count: 0,
    max_report_bytes: 0,
  };
  let recipe = CodexMcpReadRecipe::new(
    "mcp__qualified_fs__read_file".to_owned(),
    tool.clone(),
    executable.clone(),
    resources,
    outputs.clone(),
  )
  .unwrap();
  let input = CodexPreToolUseInput::parse(
    br#"{
    "hook_event_name":"PreToolUse", "tool_name":"mcp__qualified_fs__read_file", "tool_use_id":"call-1",
    "tool_input":{"path":"/workspace/source/project/lib.rs"}
  }"#,
  )
  .unwrap();
  assert_eq!(
    input.normalize_read(&recipe).unwrap().into_proposal(),
    FactoryToolActionProposalV3 {
      tool,
      executable,
      arguments: vec!["/workspace/source/project/lib.rs".to_owned()],
      paths: vec![FactoryToolPathV3 {
        path: "/workspace/source/project/lib.rs".to_owned(),
        access: FactoryToolPathAccessV3::Read
      }],
      network_hosts: vec![],
      secret_profiles: vec![],
      workload_identity_profiles: vec![],
      descendants: 1,
      resources,
      outputs,
    }
  );
  for input in [
    br#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_use_id":"call-2","tool_input":{"command":"cat /workspace/source/project/lib.rs"}}"#.as_slice(),
    br#"{"hook_event_name":"PreToolUse","tool_name":"apply_patch","tool_use_id":"call-2","tool_input":{"command":"private patch"}}"#,
    br#"{"hook_event_name":"PreToolUse","tool_name":"mcp__other__read_file","tool_use_id":"call-2","tool_input":{"path":"/workspace/source/project/lib.rs"}}"#,
    br#"{"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file","tool_use_id":"call-2","tool_input":{"path":"/workspace/source/project/lib.rs","write":true}}"#,
    br#"{"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file","tool_use_id":"call-2","tool_input":{"path":"/workspace/source/../protected/canary"}}"#,
  ] {
    assert!(CodexPreToolUseInput::parse(input).unwrap().normalize_read(&recipe).is_err());
  }
}

fn reference(identity: &str) -> FactoryImmutableReferenceV3 {
  FactoryImmutableReferenceV3 {
    identity: identity.to_owned(),
    version: "1.0.0".to_owned(),
    sha256: "ab".repeat(32),
  }
}
