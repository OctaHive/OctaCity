//! Frozen connection authority for native observations; no provider response parser.
use octacity_server_factory::{FactoryDigest, FactoryKey, FactoryTaskToolchain, ImmutableReference, NodeAttemptId};
use serde::Serialize;

use super::{FactoryCodexNativeCapture, FactoryMcpCallStatus, FactoryMcpEvidenceReference};
use crate::{
  ApplicationError, CompiledCodexTask, FactoryCodexMcpConfiguration, FactoryCodexMcpServer,
  FactoryProtectedDocumentKind, FactoryRoleInvocation,
};

/// Safe observation metadata retained by the ordinary stage result owner.
#[derive(Serialize)]
pub struct FactoryMcpObservationBinding {
  /// Exact trace/record/response locator.
  pub reference: FactoryMcpEvidenceReference,
  /// Frozen native connection alias.
  pub connection: FactoryKey,
  /// Exact transport/authentication configuration pin.
  pub configuration: ImmutableReference,
  /// Credential issuer's reviewed resource scope, when authenticated.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub resource: Option<ImmutableReference>,
  /// Actually invoked native tool.
  pub tool: String,
  /// Actual sanitized arguments' content identity; bytes remain in the trace.
  pub arguments_digest: FactoryDigest,
  /// Native start record, with arguments at `/event/item/arguments`.
  pub started_sequence: u64,
  /// Local time of the latest observation inside the original execution window.
  pub observed_at: octacity_server_domain::Timestamp,
  /// Native response lifecycle; failures remain inspectable.
  pub status: FactoryMcpCallStatus,
  /// Capture does not imply remote search completeness.
  pub coverage: String,
}

/// Sealed association with the original compiled Role invocation, never model metadata.
pub struct FactoryMcpBoundCapture<'a> {
  pub(super) capture: &'a FactoryCodexNativeCapture,
  invocation: FactoryDigest,
  node: NodeAttemptId,
  observations: Vec<FactoryMcpObservationBinding>,
}
impl FactoryMcpBoundCapture<'_> {
  /// Exact retained Role package digest.
  pub const fn invocation(&self) -> FactoryDigest {
    self.invocation
  }
  /// Original stage attempt, independent of any candidate/model text.
  pub const fn node(&self) -> NodeAttemptId {
    self.node
  }
  /// Bounded metadata; consumed values remain under the referenced Artifact.
  pub fn observations(&self) -> &[FactoryMcpObservationBinding] {
    &self.observations
  }
}

impl FactoryCodexNativeCapture {
  /// Binds only connections/tools selected in the exact compiled private package.
  /// Callers first qualify that compiled program against the original frozen Job.
  pub fn bind_mcp_connections<'a>(
    &'a self,
    invocation: &FactoryRoleInvocation,
    compiled: &CompiledCodexTask,
    configurations: &[FactoryCodexMcpConfiguration],
  ) -> Result<FactoryMcpBoundCapture<'a>, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let inputs = invocation.package().inputs();
    let package = compiled
      .documents()
      .iter()
      .find(|document| document.kind() == FactoryProtectedDocumentKind::RoleExecutionPackage)
      .ok_or_else(invalid)?;
    let prompt =
      std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).map_err(|_| invalid())?;
    if package.bytes() != invocation.package().canonical_bytes().map_err(|_| invalid())?
      || self.prompt_digest != crate::codex_prompt_provenance_digest(prompt)
      || self.toolchain != inputs.profile.definition().toolchain
    {
      return Err(invalid());
    }
    let execution = &inputs.profile.definition().execution;
    let mut observations = Vec::new();
    for call in self.mcp_calls() {
      let alias = FactoryKey::new(call.connection()).map_err(|_| invalid())?;
      let binding = execution.mcp_servers.get(&alias).ok_or_else(invalid)?;
      let configuration = configurations
        .iter()
        .find(|entry| entry.reference() == &binding.configuration)
        .ok_or_else(invalid)?;
      configuration.validate().map_err(|_| invalid())?;
      let tools = match configuration.server() {
        FactoryCodexMcpServer::Http { enabled_tools, .. } | FactoryCodexMcpServer::Stdio { enabled_tools, .. } => {
          enabled_tools
        }
      };
      if tools
        .as_ref()
        .is_some_and(|tools| !tools.iter().any(|tool| tool == call.tool()))
      {
        return Err(invalid());
      }
      observations.push(FactoryMcpObservationBinding {
        reference: call.reference(),
        connection: alias,
        configuration: binding.configuration.clone(),
        resource: configuration
          .authorization()
          .map(|authorization| authorization.resource.clone()),
        tool: call.tool().into(),
        arguments_digest: FactoryDigest::content_sha256(&serde_json::to_vec(call.arguments()).map_err(|_| invalid())?),
        started_sequence: call.started_sequence(),
        observed_at: call.observed_at(),
        status: call.status(),
        coverage: "unknown".into(),
      });
    }
    Ok(FactoryMcpBoundCapture {
      capture: self,
      invocation: invocation.package().digest().map_err(|_| invalid())?,
      node: inputs.task.node_attempt_id(),
      observations,
    })
  }
}

pub(super) fn toolchain(expected: &super::FactoryCodexProvenanceContract<'_>) -> FactoryTaskToolchain {
  FactoryTaskToolchain {
    plugin: expected.plugin.clone(),
    executable: expected.executable.clone(),
    model: expected.model.clone(),
  }
}
