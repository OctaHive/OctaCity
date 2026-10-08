//! Canonical protected tool-action contract shared by server and Agent.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
  FactoryCommandPermissionV3, FactoryImmutableReferenceV3, FactoryMountModeV3, FactoryOutputPermissionsV3,
  FactoryPermissionSetV3, FactoryResourceLimitsV3, MAX_FACTORY_COMMAND_ARGUMENT_BYTES, MAX_FACTORY_COMMAND_ARGUMENTS,
  MAX_FACTORY_PERMISSION_ENTRIES,
};

/// Domain separator used when hashing [`CanonicalFactoryToolActionV3::canonical_bytes`].
pub const FACTORY_TOOL_ACTION_DIGEST_DOMAIN: &str = "octacity.factory.tool-action.v1";

/// Final code-owned disposition for one exact protected action.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryToolActionDispositionV3 {
  /// Execute only after Agent/backend revalidation succeeds.
  Allow,
  /// Refuse the action.
  Deny,
  /// Keep the action blocked for an explicit policy or operator decision.
  Escalate,
}

/// Evidence source used by deterministic policy for one disposition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryToolActionDecisionSourceV3 {
  /// Deterministic policy decided without a provider call.
  HardPolicy,
  /// A recorded Decision Signal receipt narrowed an in-envelope action.
  DecisionSignal,
  /// A missing or invalid assessment path applied a conservative fallback.
  FailClosed,
}

/// Secret-free decision binding a disposition to one canonical proposal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "FactoryToolActionDecisionWireV3")]
pub struct FactoryToolActionDecisionV3 {
  proposal_sha256: String,
  disposition: FactoryToolActionDispositionV3,
  source: FactoryToolActionDecisionSourceV3,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  decision_signal_receipt_sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryToolActionDecisionWireV3 {
  proposal_sha256: String,
  disposition: FactoryToolActionDispositionV3,
  source: FactoryToolActionDecisionSourceV3,
  #[serde(default)]
  decision_signal_receipt_sha256: Option<String>,
}

impl TryFrom<FactoryToolActionDecisionWireV3> for FactoryToolActionDecisionV3 {
  type Error = String;

  fn try_from(value: FactoryToolActionDecisionWireV3) -> Result<Self, Self::Error> {
    Self::new(
      value.proposal_sha256,
      value.disposition,
      value.source,
      value.decision_signal_receipt_sha256,
    )
  }
}

impl FactoryToolActionDecisionV3 {
  /// Constructs a decision without accepting raw action material.
  pub fn new(
    proposal_sha256: String,
    disposition: FactoryToolActionDispositionV3,
    source: FactoryToolActionDecisionSourceV3,
    decision_signal_receipt_sha256: Option<String>,
  ) -> Result<Self, String> {
    validate_sha256("tool_action.proposal_sha256", &proposal_sha256)?;
    if let Some(digest) = decision_signal_receipt_sha256.as_deref() {
      validate_sha256("tool_action.decision_signal_receipt_sha256", digest)?;
    }
    if (source == FactoryToolActionDecisionSourceV3::DecisionSignal) != decision_signal_receipt_sha256.is_some() {
      return Err("tool-action Decision Signal source and receipt digest disagree".to_owned());
    }
    Ok(Self {
      proposal_sha256,
      disposition,
      source,
      decision_signal_receipt_sha256,
    })
  }

  /// Returns the digest of the exact blocked proposal.
  #[must_use]
  pub fn proposal_sha256(&self) -> &str {
    &self.proposal_sha256
  }

  /// Returns the final code-owned disposition.
  #[must_use]
  pub const fn disposition(&self) -> FactoryToolActionDispositionV3 {
    self.disposition
  }

  /// Returns how the disposition was selected.
  #[must_use]
  pub const fn source(&self) -> FactoryToolActionDecisionSourceV3 {
    self.source
  }

  /// Returns the immutable signal receipt digest, when one was consumed.
  #[must_use]
  pub fn decision_signal_receipt_sha256(&self) -> Option<&str> {
    self.decision_signal_receipt_sha256.as_deref()
  }
}

/// Access requested for one canonical portable path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryToolPathAccessV3 {
  /// Read without mutation.
  Read,
  /// Create, replace, or remove content.
  Write,
}

/// One path touched by a proposed protected tool action.
#[derive(Clone, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryToolPathV3 {
  /// Canonical absolute path in the portable Factory namespace.
  pub path: String,
  /// Maximum access requested for this action.
  pub access: FactoryToolPathAccessV3,
}

impl fmt::Debug for FactoryToolPathV3 {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("FactoryToolPathV3")
      .field("access", &self.access)
      .field("path", &"[redacted]")
      .finish()
  }
}

/// Untrusted bounded proposal emitted by a blocking coding-harness hook.
///
/// This value deliberately has a redacted [`Debug`] implementation. Raw
/// arguments and paths are transient authorization inputs and must not enter
/// receipts or operational logs.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryToolActionProposalV3 {
  /// Exact logical tool identity selected by the harness.
  pub tool: FactoryImmutableReferenceV3,
  /// Exact executable identity selected for the command.
  pub executable: FactoryImmutableReferenceV3,
  /// Concrete fixed-length command arguments.
  pub arguments: Vec<String>,
  /// Canonical portable paths the action will touch.
  pub paths: Vec<FactoryToolPathV3>,
  /// Exact network destinations requested by the action.
  pub network_hosts: Vec<String>,
  /// Logical secret profiles requested without credential material.
  pub secret_profiles: Vec<String>,
  /// Logical workload-identity profiles requested without credential material.
  pub workload_identity_profiles: Vec<String>,
  /// Maximum child processes requested by this action.
  pub descendants: u32,
  /// Resource ceiling requested for the protected operation.
  pub resources: FactoryResourceLimitsV3,
  /// Output authority requested for the protected operation.
  pub outputs: FactoryOutputPermissionsV3,
}

impl fmt::Debug for FactoryToolActionProposalV3 {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("FactoryToolActionProposalV3")
      .field("arguments", &self.arguments.len())
      .field("paths", &self.paths.len())
      .field("network_hosts", &self.network_hosts.len())
      .field("secret_profiles", &self.secret_profiles.len())
      .field("workload_identity_profiles", &self.workload_identity_profiles.len())
      .field("descendants", &self.descendants)
      .field("output_kinds", &self.outputs.kinds.len())
      .finish_non_exhaustive()
  }
}

impl FactoryToolActionProposalV3 {
  /// Validates bounds and normalizes unordered collections into one stable form.
  pub fn canonicalize(mut self) -> Result<CanonicalFactoryToolActionV3, String> {
    self.tool.validate("tool_action.tool")?;
    self.executable.validate("tool_action.executable")?;
    validate_arguments(&self.arguments)?;
    bounded_count("tool_action.paths", self.paths.len())?;
    for path in &self.paths {
      super::factory::portable_absolute_path("tool_action.paths.path", &path.path)?;
    }
    self.paths.sort_unstable();
    if self.paths.windows(2).any(|pair| pair[0].path == pair[1].path) {
      return Err("tool-action paths must identify each path once".to_owned());
    }
    canonicalize_strings("tool_action.network_hosts", &mut self.network_hosts, validate_host)?;
    canonicalize_strings("tool_action.secret_profiles", &mut self.secret_profiles, validate_key)?;
    canonicalize_strings(
      "tool_action.workload_identity_profiles",
      &mut self.workload_identity_profiles,
      validate_key,
    )?;
    validate_resources(self.descendants, self.resources)?;
    self.outputs.kinds.sort_unstable();
    validate_outputs(&self.outputs)?;
    Ok(CanonicalFactoryToolActionV3(self))
  }
}

/// Validated canonical proposal retained only at the enforcement boundary.
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalFactoryToolActionV3(FactoryToolActionProposalV3);

impl fmt::Debug for CanonicalFactoryToolActionV3 {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.0.fmt(formatter)
  }
}

impl CanonicalFactoryToolActionV3 {
  /// Consumes the canonical wrapper and returns the exact normalized proposal.
  #[must_use]
  pub fn into_proposal(self) -> FactoryToolActionProposalV3 {
    self.0
  }

  /// Returns deterministic bytes for a SHA-256 digest shared across boundaries.
  #[must_use]
  pub fn canonical_bytes(&self) -> Vec<u8> {
    let action = &self.0;
    let mut encoded = Vec::new();
    encode_reference(&mut encoded, b"tool", &action.tool);
    encode_reference(&mut encoded, b"executable", &action.executable);
    encode_strings(&mut encoded, b"arguments", &action.arguments);
    encode_text(&mut encoded, b"paths");
    encode_count(&mut encoded, action.paths.len());
    for path in &action.paths {
      encode_text(&mut encoded, path.path.as_bytes());
      encoded.push(match path.access {
        FactoryToolPathAccessV3::Read => 0,
        FactoryToolPathAccessV3::Write => 1,
      });
    }
    encode_strings(&mut encoded, b"network", &action.network_hosts);
    encode_strings(&mut encoded, b"secrets", &action.secret_profiles);
    encode_strings(&mut encoded, b"identities", &action.workload_identity_profiles);
    encode_text(&mut encoded, b"descendants");
    encoded.extend_from_slice(&action.descendants.to_be_bytes());
    encode_resources(&mut encoded, action.resources);
    encode_outputs(&mut encoded, &action.outputs);
    encoded
  }

  /// Returns the canonical domain-separated digest used at every boundary.
  #[must_use]
  pub fn proposal_sha256(&self) -> String {
    domain_digest(FACTORY_TOOL_ACTION_DIGEST_DOMAIN, &[&self.canonical_bytes()])
  }

  /// Checks the complete concrete action against one permission ceiling.
  pub fn is_permitted_by(&self, permissions: &FactoryPermissionSetV3) -> Result<bool, String> {
    permissions.validate()?;
    let action = &self.0;
    Ok(
      permissions.tools.contains(&action.tool)
        && permissions.executables.contains(&action.executable)
        && permissions
          .commands
          .iter()
          .any(|permission| command_permits(permission, action))
        && action.descendants <= permissions.max_descendants
        && action.paths.iter().all(|path| path_is_permitted(path, permissions))
        && is_subset(&action.network_hosts, &permissions.network_hosts)
        && is_subset(&action.secret_profiles, &permissions.secret_profiles)
        && is_subset(
          &action.workload_identity_profiles,
          &permissions.workload_identity_profiles,
        )
        && resources_fit(action.resources, permissions.resources)
        && outputs_fit(&action.outputs, &permissions.outputs),
    )
  }

  /// Returns a secret-free summary suitable for Decision Signal input.
  #[must_use]
  pub fn redacted_summary(&self) -> FactoryToolActionSummaryV3 {
    FactoryToolActionSummaryV3 {
      argument_count: self.0.arguments.len() as u16,
      read_path_count: self
        .0
        .paths
        .iter()
        .filter(|path| path.access == FactoryToolPathAccessV3::Read)
        .count() as u16,
      write_path_count: self
        .0
        .paths
        .iter()
        .filter(|path| path.access == FactoryToolPathAccessV3::Write)
        .count() as u16,
      network_host_count: self.0.network_hosts.len() as u16,
      secret_profile_count: self.0.secret_profiles.len() as u16,
      workload_identity_profile_count: self.0.workload_identity_profiles.len() as u16,
      descendants: self.0.descendants,
      output_kind_count: self.0.outputs.kinds.len() as u16,
    }
  }
}

/// Bounded non-sensitive characteristics exposed to an optional risk provider.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryToolActionSummaryV3 {
  /// Concrete argument count without argument values.
  pub argument_count: u16,
  /// Number of distinct read paths without path values.
  pub read_path_count: u16,
  /// Number of distinct write paths without path values.
  pub write_path_count: u16,
  /// Number of exact hosts without host values.
  pub network_host_count: u16,
  /// Number of secret profiles without profile names or material.
  pub secret_profile_count: u16,
  /// Number of workload identities without profile names or material.
  pub workload_identity_profile_count: u16,
  /// Maximum requested descendants.
  pub descendants: u32,
  /// Number of requested output kinds without their names.
  pub output_kind_count: u16,
}

impl FactoryToolActionSummaryV3 {
  /// Rejects summaries that could not have been produced by a bounded proposal.
  pub fn validate(&self) -> Result<(), String> {
    let bounded = |value: u16| usize::from(value) <= MAX_FACTORY_PERMISSION_ENTRIES;
    if !bounded(self.argument_count)
      || !bounded(self.read_path_count)
      || !bounded(self.write_path_count)
      || !bounded(self.network_host_count)
      || !bounded(self.secret_profile_count)
      || !bounded(self.workload_identity_profile_count)
      || !bounded(self.output_kind_count)
    {
      return Err("tool-action summary exceeds the protocol entry bound".to_owned());
    }
    Ok(())
  }
}

/// Returns a digest binding a response to one request, lease, proposal, and receipt.
#[must_use]
pub fn factory_tool_action_authorization_sha256(
  request_id: &str,
  lease: &crate::LeaseFence,
  decision: &FactoryToolActionDecisionV3,
) -> String {
  let attempt = lease.attempt.to_be_bytes();
  let disposition = [match decision.disposition {
    FactoryToolActionDispositionV3::Allow => 0,
    FactoryToolActionDispositionV3::Deny => 1,
    FactoryToolActionDispositionV3::Escalate => 2,
  }];
  let source = [match decision.source {
    FactoryToolActionDecisionSourceV3::HardPolicy => 0,
    FactoryToolActionDecisionSourceV3::DecisionSignal => 1,
    FactoryToolActionDecisionSourceV3::FailClosed => 2,
  }];
  domain_digest(
    "octacity.factory.tool-action-authorization.v1",
    &[
      request_id.as_bytes(),
      lease.lease_id.as_bytes(),
      lease.job_id.as_bytes(),
      &attempt,
      lease.fencing_token.as_bytes(),
      decision.proposal_sha256.as_bytes(),
      &disposition,
      &source,
      decision
        .decision_signal_receipt_sha256
        .as_deref()
        .unwrap_or_default()
        .as_bytes(),
    ],
  )
}

fn validate_arguments(arguments: &[String]) -> Result<(), String> {
  if arguments.len() > MAX_FACTORY_COMMAND_ARGUMENTS
    || arguments.iter().any(|argument| argument.chars().any(char::is_control))
    || arguments
      .iter()
      .try_fold(0_usize, |total, argument| total.checked_add(argument.len()))
      .is_none_or(|bytes| bytes > MAX_FACTORY_COMMAND_ARGUMENT_BYTES)
  {
    return Err("tool-action arguments exceed the protocol bound".to_owned());
  }
  Ok(())
}

fn canonicalize_strings(
  name: &str,
  values: &mut [String],
  validate: fn(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
  bounded_count(name, values.len())?;
  for value in values.iter() {
    validate(name, value)?;
  }
  values.sort_unstable();
  if values.windows(2).any(|pair| pair[0] == pair[1]) {
    return Err(format!("{name} contains a duplicate value"));
  }
  Ok(())
}

fn validate_host(name: &str, value: &str) -> Result<(), String> {
  super::factory::validate_host(name, value)
}

fn validate_key(name: &str, value: &str) -> Result<(), String> {
  super::factory::bounded_key(name, value)
}

fn validate_resources(descendants: u32, resources: FactoryResourceLimitsV3) -> Result<(), String> {
  if resources.cpu_millis == 0
    || resources.memory_bytes == 0
    || resources.disk_bytes == 0
    || resources.process_count == 0
    || resources.elapsed_millis == 0
    || descendants >= resources.process_count
  {
    return Err("tool-action resource bounds are invalid".to_owned());
  }
  Ok(())
}

fn validate_outputs(outputs: &FactoryOutputPermissionsV3) -> Result<(), String> {
  bounded_count("tool_action.outputs.kinds", outputs.kinds.len())?;
  for kind in &outputs.kinds {
    validate_key("tool_action.outputs.kinds", kind)?;
  }
  if outputs.kinds.windows(2).any(|pair| pair[0] == pair[1])
    || (outputs.max_artifact_count == 0) != (outputs.max_artifact_bytes == 0)
    || (outputs.max_report_count == 0) != (outputs.max_report_bytes == 0)
    || (!outputs.kinds.is_empty()) != (outputs.max_artifact_count > 0 || outputs.max_report_count > 0)
  {
    return Err("tool-action output bounds are invalid".to_owned());
  }
  Ok(())
}

fn command_permits(permission: &FactoryCommandPermissionV3, action: &FactoryToolActionProposalV3) -> bool {
  permission.executable == action.executable && permission.permits_arguments(&action.arguments)
}

fn path_is_permitted(path: &FactoryToolPathV3, permissions: &FactoryPermissionSetV3) -> bool {
  permissions.mounts.iter().any(|mount| {
    super::factory::path_contains(&mount.root, &path.path)
      && (path.access == FactoryToolPathAccessV3::Read || mount.mode == FactoryMountModeV3::ReadWrite)
  })
}

fn is_subset(values: &[String], grants: &[String]) -> bool {
  values.iter().all(|value| grants.binary_search(value).is_ok())
}

const fn resources_fit(requested: FactoryResourceLimitsV3, granted: FactoryResourceLimitsV3) -> bool {
  requested.cpu_millis <= granted.cpu_millis
    && requested.memory_bytes <= granted.memory_bytes
    && requested.disk_bytes <= granted.disk_bytes
    && requested.process_count <= granted.process_count
    && requested.elapsed_millis <= granted.elapsed_millis
}

fn outputs_fit(requested: &FactoryOutputPermissionsV3, granted: &FactoryOutputPermissionsV3) -> bool {
  is_subset(&requested.kinds, &granted.kinds)
    && requested.max_artifact_count <= granted.max_artifact_count
    && requested.max_artifact_bytes <= granted.max_artifact_bytes
    && requested.max_report_count <= granted.max_report_count
    && requested.max_report_bytes <= granted.max_report_bytes
}

fn bounded_count(name: &str, count: usize) -> Result<(), String> {
  if count > MAX_FACTORY_PERMISSION_ENTRIES {
    Err(format!("{name} exceeds the protocol entry bound"))
  } else {
    Ok(())
  }
}

fn validate_sha256(name: &str, value: &str) -> Result<(), String> {
  if value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
  {
    Ok(())
  } else {
    Err(format!("{name} must be a lowercase SHA-256 digest"))
  }
}

fn domain_digest(domain: &str, fields: &[&[u8]]) -> String {
  let mut digest = Sha256::new();
  hash_field(&mut digest, domain.as_bytes());
  for field in fields {
    hash_field(&mut digest, field);
  }
  let bytes = digest.finalize();
  let mut encoded = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    use std::fmt::Write as _;
    write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
  }
  encoded
}

fn hash_field(digest: &mut Sha256, field: &[u8]) {
  digest.update(
    u64::try_from(field.len())
      .expect("protocol bounds fit in u64")
      .to_be_bytes(),
  );
  digest.update(field);
}

fn encode_reference(encoded: &mut Vec<u8>, name: &[u8], reference: &FactoryImmutableReferenceV3) {
  encode_text(encoded, name);
  encode_text(encoded, reference.identity.as_bytes());
  encode_text(encoded, reference.version.as_bytes());
  encode_text(encoded, reference.sha256.as_bytes());
}

fn encode_strings(encoded: &mut Vec<u8>, name: &[u8], values: &[String]) {
  encode_text(encoded, name);
  encode_count(encoded, values.len());
  for value in values {
    encode_text(encoded, value.as_bytes());
  }
}

fn encode_resources(encoded: &mut Vec<u8>, resources: FactoryResourceLimitsV3) {
  encode_text(encoded, b"resources");
  encoded.extend_from_slice(&resources.cpu_millis.to_be_bytes());
  encoded.extend_from_slice(&resources.memory_bytes.to_be_bytes());
  encoded.extend_from_slice(&resources.disk_bytes.to_be_bytes());
  encoded.extend_from_slice(&resources.process_count.to_be_bytes());
  encoded.extend_from_slice(&resources.elapsed_millis.to_be_bytes());
}

fn encode_outputs(encoded: &mut Vec<u8>, outputs: &FactoryOutputPermissionsV3) {
  encode_strings(encoded, b"outputs", &outputs.kinds);
  encoded.extend_from_slice(&outputs.max_artifact_count.to_be_bytes());
  encoded.extend_from_slice(&outputs.max_artifact_bytes.to_be_bytes());
  encoded.extend_from_slice(&outputs.max_report_count.to_be_bytes());
  encoded.extend_from_slice(&outputs.max_report_bytes.to_be_bytes());
}

fn encode_text(encoded: &mut Vec<u8>, value: &[u8]) {
  encode_count(encoded, value.len());
  encoded.extend_from_slice(value);
}

fn encode_count(encoded: &mut Vec<u8>, value: usize) {
  encoded.extend_from_slice(
    &u64::try_from(value)
      .expect("bounded tool-action collections fit into u64")
      .to_be_bytes(),
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    AuthorizeToolActionRequest, AuthorizeToolActionResponse, COORDINATOR_PROTOCOL_VERSION, FactoryCommandArgumentV3,
    FactoryMountPermissionV3, LeaseFence,
  };

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  #[test]
  fn proposals_are_canonical_secret_safe_and_checked_against_every_category() {
    let mut reversed = proposal();
    reversed.network_hosts.reverse();
    reversed.paths.reverse();
    let canonical = reversed.canonicalize().unwrap();
    let expected = proposal().canonicalize().unwrap();
    assert_eq!(canonical.canonical_bytes(), expected.canonical_bytes());
    assert!(canonical.is_permitted_by(&permissions()).unwrap());

    let debug = format!("{canonical:?}");
    assert!(!debug.contains("sensitive-argument"));
    assert!(!debug.contains("/workspace/source/private"));
    assert!(!debug.contains("model-coding"));

    let mut outside = proposal();
    outside.network_hosts.push("outside.example".to_owned());
    let outside = outside.canonicalize().unwrap();
    assert!(!outside.is_permitted_by(&permissions()).unwrap());
  }

  #[test]
  fn malformed_or_ambiguous_proposals_are_rejected_before_authorization() {
    let mut duplicate_path = proposal();
    duplicate_path.paths.push(duplicate_path.paths[0].clone());
    assert!(duplicate_path.canonicalize().is_err());

    let mut traversal = proposal();
    traversal.paths[0].path = "/workspace/source/../secret".to_owned();
    assert!(traversal.canonicalize().is_err());

    let mut control = proposal();
    control.arguments[0] = "unsafe\nargument".to_owned();
    assert!(control.canonicalize().is_err());
  }

  #[test]
  fn authorization_response_binds_the_exact_fence_proposal_and_receipt() {
    let canonical = proposal().canonicalize().unwrap();
    let request = AuthorizeToolActionRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "request-1".to_owned(),
      registration_id: "registration-1".to_owned(),
      lease: LeaseFence {
        lease_id: "lease-1".to_owned(),
        job_id: "job-1".to_owned(),
        attempt: 1,
        fencing_token: "fence-1".to_owned(),
      },
      proposal_sha256: canonical.proposal_sha256(),
      proposal_summary: canonical.redacted_summary(),
    };
    let decision = FactoryToolActionDecisionV3::new(
      request.proposal_sha256.clone(),
      FactoryToolActionDispositionV3::Allow,
      FactoryToolActionDecisionSourceV3::DecisionSignal,
      Some("a".repeat(64)),
    )
    .unwrap();
    let response = AuthorizeToolActionResponse::new(&request, decision).unwrap();
    assert!(response.validate(&request).is_ok());

    let mut other_fence = request.clone();
    other_fence.lease.fencing_token = "fence-2".to_owned();
    assert!(response.validate(&other_fence).is_err());

    let mut other_receipt = response.clone();
    other_receipt.decision = FactoryToolActionDecisionV3::new(
      request.proposal_sha256.clone(),
      FactoryToolActionDispositionV3::Allow,
      FactoryToolActionDecisionSourceV3::DecisionSignal,
      Some("b".repeat(64)),
    )
    .unwrap();
    assert!(other_receipt.validate(&request).is_err());
  }

  #[test]
  fn decisions_reject_invalid_digests_and_inconsistent_signal_evidence() {
    let invalid_digest = serde_json::json!({
      "proposal_sha256": "not-a-digest",
      "disposition": "allow",
      "source": "hard_policy"
    });
    assert!(serde_json::from_value::<FactoryToolActionDecisionV3>(invalid_digest).is_err());

    let missing_receipt = serde_json::json!({
      "proposal_sha256": DIGEST,
      "disposition": "deny",
      "source": "decision_signal"
    });
    assert!(serde_json::from_value::<FactoryToolActionDecisionV3>(missing_receipt).is_err());

    let unexpected_receipt = serde_json::json!({
      "proposal_sha256": DIGEST,
      "disposition": "allow",
      "source": "hard_policy",
      "decision_signal_receipt_sha256": DIGEST
    });
    assert!(serde_json::from_value::<FactoryToolActionDecisionV3>(unexpected_receipt).is_err());
  }

  fn proposal() -> FactoryToolActionProposalV3 {
    FactoryToolActionProposalV3 {
      tool: reference("shell"),
      executable: reference("codex-cli"),
      arguments: vec!["sensitive-argument".to_owned()],
      paths: vec![
        FactoryToolPathV3 {
          path: "/workspace/source/private".to_owned(),
          access: FactoryToolPathAccessV3::Read,
        },
        FactoryToolPathV3 {
          path: "/workspace/scratch/result".to_owned(),
          access: FactoryToolPathAccessV3::Write,
        },
      ],
      network_hosts: vec!["api.openai.com".to_owned(), "objects.example".to_owned()],
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
      network_hosts: vec!["api.openai.com".to_owned(), "objects.example".to_owned()],
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
