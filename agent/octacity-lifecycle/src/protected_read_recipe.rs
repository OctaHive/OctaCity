//! Installed, provider-neutral definition of the protected reader operation.

use octacity_protocol::{
  CanonicalFactoryToolActionV3, FactoryImmutableReferenceV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3,
  FactoryToolActionProposalV3, FactoryToolPathAccessV3, FactoryToolPathV3,
};

use crate::AgentToolActionBrokerError;

/// Immutable technical definition of one installed read-only operation.
///
/// The selected tool and reader executable, limits and outputs come from trusted
/// inventory. A path argument requests exactly one read and adds no network,
/// credential, workload-identity or write authority. This value is metadata;
/// constructing it does not establish physical installation or qualification.
#[derive(Clone)]
pub struct ProtectedReadRecipe {
  template: FactoryToolActionProposalV3,
}

impl ProtectedReadRecipe {
  /// Validates and freezes an installed reader's technical envelope.
  pub fn new(
    tool: FactoryImmutableReferenceV3,
    executable: FactoryImmutableReferenceV3,
    resources: FactoryResourceLimitsV3,
    outputs: FactoryOutputPermissionsV3,
  ) -> Result<Self, AgentToolActionBrokerError> {
    let template = FactoryToolActionProposalV3 {
      tool,
      executable,
      arguments: vec![],
      paths: vec![],
      network_hosts: vec![],
      secret_profiles: vec![],
      workload_identity_profiles: vec![],
      descendants: 1,
      resources,
      outputs,
    }
    .canonicalize()
    .map_err(|_| AgentToolActionBrokerError::Denied)?
    .into_proposal();
    Ok(Self { template })
  }

  /// Constructs the exact canonical reader action for one portable path.
  pub fn action(&self, path: &str) -> Result<CanonicalFactoryToolActionV3, AgentToolActionBrokerError> {
    let mut proposal = self.template.clone();
    proposal.arguments = vec![path.to_owned()];
    proposal.paths = vec![FactoryToolPathV3 {
      path: path.to_owned(),
      access: FactoryToolPathAccessV3::Read,
    }];
    proposal.canonicalize().map_err(|_| AgentToolActionBrokerError::Denied)
  }
}
