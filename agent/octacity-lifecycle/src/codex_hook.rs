//! Bounded adapter between Codex `PreToolUse` JSON and Factory tool actions.

use octacity_protocol::{
  CanonicalFactoryToolActionV3, FactoryImmutableReferenceV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::ProtectedReadRecipe;

/// Maximum hook document accepted before JSON parsing.
pub const MAX_CODEX_HOOK_BYTES: usize = 64 * 1024;

/// Stable error returned for malformed or unsupported hook input.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CodexHookError {
  /// Input is not one bounded `PreToolUse` event.
  #[error("Codex PreToolUse input is invalid")]
  InvalidInput,
}

/// Minimal trusted projection of the official Codex `PreToolUse` input.
///
/// Common hook fields are deliberately ignored after the complete input is
/// byte-bounded. Provider-controlled identifiers and raw tool input are never
/// included in `Debug` output.
#[derive(Clone, Deserialize)]
pub struct CodexPreToolUseInput {
  hook_event_name: String,
  tool_name: String,
  tool_use_id: String,
  tool_input: Value,
}

impl std::fmt::Debug for CodexPreToolUseInput {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("CodexPreToolUseInput")
      .field("hook_event_name", &self.hook_event_name)
      .finish_non_exhaustive()
  }
}

impl CodexPreToolUseInput {
  /// Parses one bounded hook document and rejects non-`PreToolUse` events.
  pub fn parse(bytes: &[u8]) -> Result<Self, CodexHookError> {
    if bytes.is_empty() || bytes.len() > MAX_CODEX_HOOK_BYTES {
      return Err(CodexHookError::InvalidInput);
    }
    let input: Self = serde_json::from_slice(bytes).map_err(|_| CodexHookError::InvalidInput)?;
    if input.hook_event_name != "PreToolUse"
      || !bounded_identifier(&input.tool_name)
      || !bounded_identifier(&input.tool_use_id)
      || !input.tool_input.is_object()
    {
      return Err(CodexHookError::InvalidInput);
    }
    Ok(input)
  }

  /// Normalizes only an installed MCP reader with the strict `{path}` contract.
  /// Operator-pinned identities and limits supply the executable envelope;
  /// input cannot select a command, add authority, or invoke a native shell.
  /// Execution still requires the broker and trusted protected dispatcher.
  pub fn normalize_read(&self, recipe: &CodexMcpReadRecipe) -> Result<CanonicalFactoryToolActionV3, CodexHookError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ReadInput {
      path: String,
    }
    if self.tool_name != recipe.hook_tool_name {
      return Err(CodexHookError::InvalidInput);
    }
    let input: ReadInput = serde_json::from_value(self.tool_input.clone()).map_err(|_| CodexHookError::InvalidInput)?;
    recipe
      .reader
      .action(&input.path)
      .map_err(|_| CodexHookError::InvalidInput)
  }
}

/// Operator-installed definition of one finite structured MCP read operation.
/// It must also identify the real trusted reader in the protected dispatcher;
/// constructing this recipe alone does not qualify a harness or backend.
#[derive(Clone)]
pub struct CodexMcpReadRecipe {
  hook_tool_name: String,
  reader: ProtectedReadRecipe,
}

impl CodexMcpReadRecipe {
  /// Pins the exact MCP tool and reader executable, resource and output bounds.
  /// The reader accepts one absolute portable path argument and can request no
  /// network, credential, workload-identity or filesystem-write authority.
  pub fn new(
    hook_tool_name: String,
    tool: FactoryImmutableReferenceV3,
    executable: FactoryImmutableReferenceV3,
    resources: FactoryResourceLimitsV3,
    outputs: FactoryOutputPermissionsV3,
  ) -> Result<Self, CodexHookError> {
    let mcp_name = hook_tool_name
      .strip_prefix("mcp__")
      .and_then(|name| name.split_once("__"));
    if !bounded_identifier(&hook_tool_name)
      || !mcp_name.is_some_and(|(server, tool)| !server.is_empty() && !tool.is_empty())
    {
      return Err(CodexHookError::InvalidInput);
    }
    let reader =
      ProtectedReadRecipe::new(tool, executable, resources, outputs).map_err(|_| CodexHookError::InvalidInput)?;
    Ok(Self { hook_tool_name, reader })
  }
}

/// Returns the only supported JSON shape that releases the unchanged tool call.
pub fn codex_permitted_output() -> Value {
  serde_json::to_value(CodexHookOutput::decision("allow", None)).expect("fixed Codex hook output serializes")
}

/// Returns an explicit supported denial for every fail-closed broker outcome.
pub fn codex_denied_output() -> Value {
  serde_json::to_value(CodexHookOutput::decision(
    "deny",
    Some("Action denied by OctaCity policy."),
  ))
  .expect("fixed Codex hook output serializes")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexHookOutput<'a> {
  hook_specific_output: CodexHookSpecificOutput<'a>,
}

impl<'a> CodexHookOutput<'a> {
  const fn decision(permission_decision: &'a str, reason: Option<&'a str>) -> Self {
    Self {
      hook_specific_output: CodexHookSpecificOutput {
        hook_event_name: "PreToolUse",
        permission_decision,
        permission_decision_reason: reason,
      },
    }
  }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexHookSpecificOutput<'a> {
  hook_event_name: &'static str,
  permission_decision: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  permission_decision_reason: Option<&'a str>,
}

fn bounded_identifier(value: &str) -> bool {
  !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

#[cfg(test)]
#[path = "codex_mcp_recipe_tests.rs"]
mod recipe_tests;

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn parses_bounded_input_without_exposing_arguments_in_debug_output() {
    let input = input(br#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_use_id":"call-1","tool_input":{"command":"printf super-secret"}}"#);

    let rendered = format!("{input:?}");
    assert!(!rendered.contains("super-secret"));
    assert!(!rendered.contains("tool_input"));
  }

  #[test]
  fn provider_controlled_hook_identifiers_do_not_reach_diagnostics() {
    let input = input(br#"{"hook_event_name":"PreToolUse","tool_name":"private-path-/home/operator/credential","tool_use_id":"provider-secret-canary","tool_input":{}}"#);

    let rendered = format!("{input:?}");
    assert!(!rendered.contains("/home/operator/credential"));
    assert!(!rendered.contains("provider-secret-canary"));
    assert!(rendered.contains("PreToolUse"));
  }

  #[test]
  fn rejects_malformed_oversized_and_non_pre_tool_use_documents() {
    assert!(matches!(
      CodexPreToolUseInput::parse(b"{}"),
      Err(CodexHookError::InvalidInput)
    ));
    assert!(matches!(
      CodexPreToolUseInput::parse(&vec![b'x'; MAX_CODEX_HOOK_BYTES + 1]),
      Err(CodexHookError::InvalidInput)
    ));
    assert!(matches!(
      CodexPreToolUseInput::parse(
        br#"{"hook_event_name":"PostToolUse","tool_name":"Bash","tool_use_id":"call-1","tool_input":{}}"#
      ),
      Err(CodexHookError::InvalidInput)
    ));
  }

  #[test]
  fn rejects_hook_calls_without_a_bounded_object_describing_the_actual_action() {
    for bytes in [
      br#"{"hook_event_name":"PreToolUse","tool_name":"Read","tool_use_id":"call-1"}"#.as_slice(),
      br#"{"hook_event_name":"PreToolUse","tool_name":"Read","tool_use_id":"call-1","tool_input":null}"#.as_slice(),
      br#"{"hook_event_name":"PreToolUse","tool_name":"Read","tool_use_id":"call-1","tool_input":"/private/canary"}"#
        .as_slice(),
    ] {
      assert!(
        matches!(CodexPreToolUseInput::parse(bytes), Err(CodexHookError::InvalidInput)),
        "an incomplete action cannot enter exact-action normalization"
      );
    }
  }

  #[test]
  fn emits_only_the_official_blocking_dispositions() {
    assert_eq!(
      codex_permitted_output(),
      json!({
        "hookSpecificOutput": {
          "hookEventName": "PreToolUse",
          "permissionDecision": "allow"
        }
      })
    );
    assert_eq!(
      codex_denied_output(),
      json!({
        "hookSpecificOutput": {
          "hookEventName": "PreToolUse",
          "permissionDecision": "deny",
          "permissionDecisionReason": "Action denied by OctaCity policy."
        }
      })
    );
  }

  fn input(bytes: &[u8]) -> CodexPreToolUseInput {
    CodexPreToolUseInput::parse(bytes).unwrap()
  }
}
