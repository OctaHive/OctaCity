//! Final Agent-side enforcement for protected coding-harness tool actions.

use std::fmt;

use octacity_protocol::{
  CanonicalFactoryToolActionV3, FactoryPermissionSetV3, FactoryToolActionDecisionV3, FactoryToolActionDispositionV3,
  FactoryToolActionProposalV3,
};
use thiserror::Error;

/// Stable, secret-free reason a protected tool action was blocked.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FactoryToolActionEnforcementError {
  /// The proposed action was malformed or exceeded a protocol bound.
  #[error("protected tool-action proposal is invalid")]
  InvalidProposal,
  /// A signed or local/backend permission ceiling was malformed.
  #[error("protected tool-action permission ceiling is invalid")]
  InvalidPermissionCeiling,
  /// The proposal differs from the exact action authorized by the server.
  #[error("protected tool-action proposal changed after authorization")]
  ProposalChanged,
  /// Policy did not allow this exact proposal to execute.
  #[error("protected tool-action is not allowed")]
  NotAllowed,
  /// The exact action exceeds the signed or local/backend permission ceiling.
  #[error("protected tool-action exceeds its permission ceiling")]
  OutsidePermissionCeiling,
}

/// Exact canonical action that passed final Agent/backend enforcement.
pub struct AuthorizedFactoryToolAction {
  action: CanonicalFactoryToolActionV3,
}

impl fmt::Debug for AuthorizedFactoryToolAction {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("AuthorizedFactoryToolAction")
      .field("action", &self.action)
      .finish()
  }
}

impl AuthorizedFactoryToolAction {
  /// Consumes the authorization proof and returns the exact canonical action.
  ///
  /// A caller must execute this returned value rather than a separately retained
  /// proposal, preventing a check/use split across the enforcement boundary.
  #[must_use]
  pub fn into_proposal(self) -> FactoryToolActionProposalV3 {
    self.action.into_proposal()
  }
}

/// Revalidates the unchanged allowed action immediately before backend use.
///
/// Both the signed JobSpec permissions and the Agent/backend-local ceiling are
/// authoritative. A decision signal cannot bypass either ceiling, and deny or
/// escalate dispositions never produce an executable value.
pub fn enforce_factory_tool_action(
  proposal: FactoryToolActionProposalV3,
  decision: &FactoryToolActionDecisionV3,
  signed_permissions: &FactoryPermissionSetV3,
  local_backend_permissions: &FactoryPermissionSetV3,
) -> Result<AuthorizedFactoryToolAction, FactoryToolActionEnforcementError> {
  let action = proposal
    .canonicalize()
    .map_err(|_| FactoryToolActionEnforcementError::InvalidProposal)?;
  let proposal_digest = action.proposal_sha256();
  if proposal_digest != decision.proposal_sha256() {
    return Err(FactoryToolActionEnforcementError::ProposalChanged);
  }
  if decision.disposition() != FactoryToolActionDispositionV3::Allow {
    return Err(FactoryToolActionEnforcementError::NotAllowed);
  }
  let inside_signed = action
    .is_permitted_by(signed_permissions)
    .map_err(|_| FactoryToolActionEnforcementError::InvalidPermissionCeiling)?;
  let inside_local = action
    .is_permitted_by(local_backend_permissions)
    .map_err(|_| FactoryToolActionEnforcementError::InvalidPermissionCeiling)?;
  if !inside_signed || !inside_local {
    return Err(FactoryToolActionEnforcementError::OutsidePermissionCeiling);
  }
  Ok(AuthorizedFactoryToolAction { action })
}

#[cfg(test)]
mod tests {
  use octacity_protocol::{
    FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryImmutableReferenceV3, FactoryMountModeV3,
    FactoryMountPermissionV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3, FactoryToolActionDecisionSourceV3,
    FactoryToolPathAccessV3, FactoryToolPathV3,
  };

  use super::*;

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  #[test]
  fn unchanged_allowed_action_passes_both_permission_ceilings() {
    let proposal = proposal();
    let decision = allow(&proposal);
    let authorized = enforce_factory_tool_action(proposal.clone(), &decision, &permissions(), &permissions()).unwrap();

    assert_eq!(authorized.into_proposal(), proposal);
  }

  #[test]
  fn changed_or_non_allowed_actions_never_produce_an_executable_value() {
    let proposal = proposal();
    let allowed = allow(&proposal);
    let mut changed = proposal.clone();
    changed.arguments.push("different".to_owned());
    assert_eq!(
      enforce_factory_tool_action(changed, &allowed, &permissions(), &permissions()).unwrap_err(),
      FactoryToolActionEnforcementError::ProposalChanged
    );

    for disposition in [
      FactoryToolActionDispositionV3::Deny,
      FactoryToolActionDispositionV3::Escalate,
    ] {
      let decision = decision(&proposal, disposition);
      assert_eq!(
        enforce_factory_tool_action(proposal.clone(), &decision, &permissions(), &permissions()).unwrap_err(),
        FactoryToolActionEnforcementError::NotAllowed
      );
    }
  }

  #[test]
  fn local_backend_narrowing_is_authoritative_and_errors_are_secret_free() {
    let proposal = proposal();
    let decision = allow(&proposal);
    let mut local = permissions();
    local.network_hosts.clear();
    let error = enforce_factory_tool_action(proposal, &decision, &permissions(), &local).unwrap_err();

    assert_eq!(error, FactoryToolActionEnforcementError::OutsidePermissionCeiling);
    let rendered = format!("{error:?} {error}");
    for raw in [
      "sensitive-argument",
      "/workspace/source/private",
      "api.openai.com",
      "model-coding",
    ] {
      assert!(!rendered.contains(raw));
    }
  }

  fn allow(proposal: &FactoryToolActionProposalV3) -> FactoryToolActionDecisionV3 {
    decision(proposal, FactoryToolActionDispositionV3::Allow)
  }

  fn decision(
    proposal: &FactoryToolActionProposalV3,
    disposition: FactoryToolActionDispositionV3,
  ) -> FactoryToolActionDecisionV3 {
    let canonical = proposal.clone().canonicalize().unwrap();
    FactoryToolActionDecisionV3::new(
      canonical.proposal_sha256(),
      disposition,
      FactoryToolActionDecisionSourceV3::HardPolicy,
      None,
    )
    .unwrap()
  }

  fn proposal() -> FactoryToolActionProposalV3 {
    FactoryToolActionProposalV3 {
      tool: reference("shell"),
      executable: reference("codex-cli"),
      arguments: vec!["sensitive-argument".to_owned()],
      paths: vec![FactoryToolPathV3 {
        path: "/workspace/source/private".to_owned(),
        access: FactoryToolPathAccessV3::Read,
      }],
      network_hosts: vec!["api.openai.com".to_owned()],
      secret_profiles: vec!["model-coding".to_owned()],
      workload_identity_profiles: vec![],
      descendants: 1,
      resources: FactoryResourceLimitsV3 {
        cpu_millis: 500,
        memory_bytes: 512,
        disk_bytes: 512,
        process_count: 2,
        elapsed_millis: 1_000,
      },
      outputs: FactoryOutputPermissionsV3 {
        kinds: vec!["codex-result".to_owned()],
        max_artifact_count: 1,
        max_artifact_bytes: 512,
        max_report_count: 0,
        max_report_bytes: 0,
      },
    }
  }

  fn permissions() -> FactoryPermissionSetV3 {
    let executable = reference("codex-cli");
    FactoryPermissionSetV3 {
      plugins: vec![reference("codex")],
      executables: vec![executable.clone()],
      tools: vec![reference("shell")],
      commands: vec![FactoryCommandPermissionV3 {
        executable,
        arguments: vec![FactoryCommandArgumentV3::Any { max_bytes: 256 }],
      }],
      max_descendants: 2,
      mounts: vec![
        FactoryMountPermissionV3 {
          root: "/octacity/protected".to_owned(),
          mode: FactoryMountModeV3::ReadOnly,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/output".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/scratch".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/source".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
      ],
      network_hosts: vec!["api.openai.com".to_owned()],
      secret_profiles: vec!["model-coding".to_owned()],
      workload_identity_profiles: vec![],
      resources: FactoryResourceLimitsV3 {
        cpu_millis: 1_000,
        memory_bytes: 1_024,
        disk_bytes: 1_024,
        process_count: 4,
        elapsed_millis: 60_000,
      },
      outputs: FactoryOutputPermissionsV3 {
        kinds: vec!["codex-result".to_owned()],
        max_artifact_count: 2,
        max_artifact_bytes: 1_024,
        max_report_count: 0,
        max_report_bytes: 0,
      },
    }
  }

  fn reference(identity: &str) -> FactoryImmutableReferenceV3 {
    FactoryImmutableReferenceV3 {
      identity: identity.to_owned(),
      version: "1.0.0".to_owned(),
      sha256: DIGEST.to_owned(),
    }
  }
}
