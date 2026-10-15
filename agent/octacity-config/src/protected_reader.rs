//! Validates the separately installed protected-reader technical envelope.

use std::path::{Path, PathBuf};

use octacity_protocol::{
  FactoryImmutableReferenceV3, FactoryOutputPermissionsV3, FactoryResourceLimitsV3, FactoryToolActionProposalV3,
};
use serde::Deserialize;

use crate::{
  ConfigError,
  validation::{canonical_directory, invalid, validate_distinct_roots, validate_octa_platform, validate_sha256},
};

/// Frozen technical intent for one operator-installed standalone reader.
///
/// Source paths, commands, credentials and network authority come from neither
/// this configuration nor model arguments. Binary identity and backend
/// qualification are checked separately by the Agent composition root.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedReaderConfig {
  /// Explicit optional coding-harness adapter. Absence keeps controlled Jobs disabled.
  #[serde(default)]
  pub bridge: Option<ProtectedReaderBridgeConfig>,
  /// Dedicated immutable worker distribution, separate from writable Job roots.
  pub release_root: PathBuf,
  /// Portable relative executable path inside the worker distribution.
  pub entrypoint: PathBuf,
  /// Exact binary platform, possibly different from the Agent host platform.
  pub platform: String,
  /// Immutable tool identity frozen into each authorized read action.
  pub tool: FactoryImmutableReferenceV3,
  /// Immutable executable identity whose SHA-256 binds the installed bytes.
  pub executable: FactoryImmutableReferenceV3,
  /// Physical allocation reserved within the signed Job resource envelope.
  pub resources: FactoryResourceLimitsV3,
  /// Exact publication authority frozen into each authorized read action.
  pub outputs: FactoryOutputPermissionsV3,
}

/// Separately pinned adapter executable from the same immutable reader installation.
/// This is technical inventory, not a grant or proof of runtime qualification.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedReaderBridgeConfig {
  /// Portable path to the adapter binary, distinct from the confined worker.
  pub entrypoint: PathBuf,
  /// Exact SHA-256 of the separately installed adapter bytes.
  pub sha256: String,
  /// Installed structured tool name interpreted only by the concrete adapter.
  pub hook_tool_name: String,
}

impl ProtectedReaderConfig {
  pub(super) fn validate(&mut self, roots: &[(&str, PathBuf)], credential_parent: &Path) -> Result<(), ConfigError> {
    validate_octa_platform("protected_reader.platform", &self.platform)?;
    let recipe = FactoryToolActionProposalV3 {
      tool: self.tool.clone(),
      executable: self.executable.clone(),
      arguments: vec![],
      paths: vec![],
      network_hosts: vec![],
      secret_profiles: vec![],
      workload_identity_profiles: vec![],
      descendants: 1,
      resources: self.resources,
      outputs: self.outputs.clone(),
    }
    .canonicalize()
    .map_err(|_| ConfigError::Invalid("invalid protected_reader technical envelope".to_owned()))?;
    self.outputs = recipe.into_proposal().outputs;
    if self.entrypoint.as_os_str().is_empty()
      || !self
        .entrypoint
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
      || !self
        .entrypoint
        .to_str()
        .is_some_and(|path| !path.contains(['\\', ':']) && !path.chars().any(char::is_control))
    {
      return invalid("protected_reader entrypoint must be a portable relative path");
    }
    if let Some(bridge) = &self.bridge {
      if bridge.entrypoint.as_os_str().is_empty()
        || bridge.entrypoint == self.entrypoint
        || !bridge
          .entrypoint
          .components()
          .all(|part| matches!(part, std::path::Component::Normal(_)))
        || !bridge
          .entrypoint
          .to_str()
          .is_some_and(|path| !path.contains(['\\', ':']) && !path.chars().any(char::is_control))
        || bridge.hook_tool_name.is_empty()
        || bridge.hook_tool_name.len() > 256
        || bridge.hook_tool_name.chars().any(char::is_control)
      {
        return invalid("protected reader adapter installation is invalid");
      }
      validate_sha256("protected reader adapter", &bridge.sha256)?;
    }
    self.release_root = canonical_directory("protected_reader.release_root", &self.release_root)?;
    for (name, root) in roots
      .iter()
      .map(|(name, root)| (*name, root.as_path()))
      .chain(std::iter::once(("credential_file parent", credential_parent)))
    {
      validate_distinct_roots(&[
        ("protected_reader.release_root", self.release_root.clone()),
        (name, root.to_owned()),
      ])?;
    }
    Ok(())
  }
}
