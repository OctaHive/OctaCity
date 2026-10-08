//! Bounded adapter between Codex `PreToolUse` JSON and Factory tool actions.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

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
/// byte-bounded. Raw tool input is never included in `Debug` output.
#[derive(Clone, Deserialize)]
pub struct CodexPreToolUseInput {
  hook_event_name: String,
  tool_name: String,
  tool_use_id: String,
}

impl std::fmt::Debug for CodexPreToolUseInput {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("CodexPreToolUseInput")
      .field("hook_event_name", &self.hook_event_name)
      .field("tool_name", &self.tool_name)
      .field("tool_use_id", &self.tool_use_id)
      .finish()
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
    {
      return Err(CodexHookError::InvalidInput);
    }
    Ok(input)
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
