//! Strict provider-neutral values carried only by JobSpec execution contract v3.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::{ExecutionMode, NetworkPolicy, OctaSpec, OutputLimits, RuntimeSpecV2};

/// Maximum entries in any v3 Factory permission category.
pub const MAX_FACTORY_PERMISSION_ENTRIES: usize = 256;
/// Maximum protected inputs carried by one managed execution manifest.
pub const MAX_PROTECTED_INPUTS: usize = 256;
/// Maximum aggregate bytes declared by one protected-input manifest.
pub const MAX_PROTECTED_INPUT_BYTES: u64 = 1024 * 1024 * 1024;
/// Maximum bytes in one portable execution path.
pub const MAX_FACTORY_PATH_BYTES: usize = 1024;
/// Maximum UTF-8 bytes in one bounded logical identity.
pub const MAX_FACTORY_WIRE_IDENTITY_BYTES: usize = 256;
/// Maximum UTF-8 bytes in one canonical ChangeSet author field.
pub const MAX_CHANGE_SET_AUTHOR_BYTES: usize = 256;
/// Maximum optional textual patch bytes carried by one captured ChangeSet.
pub const MAX_CHANGE_SET_PATCH_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum changed paths authorized by one ChangeSet capture instruction.
pub const MAX_CHANGE_SET_CHANGED_PATHS: u32 = 4_096;
/// Maximum bytes in one changed regular file or symbolic-link target.
pub const MAX_CHANGE_SET_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum aggregate bytes across changed candidate entries.
pub const MAX_CHANGE_SET_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum canonical JSON bytes in one captured ChangeSet manifest.
pub const MAX_CHANGE_SET_MANIFEST_BYTES: u64 =
  (MAX_CHANGE_SET_CHANGED_PATHS as u64 * (MAX_FACTORY_PATH_BYTES as u64 + 128)) + (16 * 1024);
/// Maximum raw Git path-index bytes for every bounded changed path.
pub const MAX_CHANGE_SET_PATH_INDEX_BYTES: u64 =
  (MAX_CHANGE_SET_CHANGED_PATHS as u64 * (MAX_FACTORY_PATH_BYTES as u64 + 128)) + 1024;
/// Reserved logical Artifact name for the exact candidate Git bundle.
pub const CHANGE_SET_BUNDLE_OUTPUT: &str = "change-set.bundle";
/// Reserved logical Artifact name for the canonical capture manifest.
pub const CHANGE_SET_MANIFEST_OUTPUT: &str = "change-set-manifest.json";
/// Reserved logical Artifact name for optional bounded operator evidence.
pub const CHANGE_SET_PATCH_OUTPUT: &str = "change-set.patch";
/// Logical media type of a trusted ChangeSet bundle.
pub const CHANGE_SET_BUNDLE_MEDIA_TYPE: &str = "application/vnd.octacity.changeset-git-bundle.v1";
/// Logical media type of a trusted ChangeSet manifest.
pub const CHANGE_SET_MANIFEST_MEDIA_TYPE: &str = "application/vnd.octacity.changeset-manifest.v1+json";
/// Logical media type of an optional textual ChangeSet patch.
pub const CHANGE_SET_PATCH_MEDIA_TYPE: &str = "text/x-diff";
/// Reserved protected-input destination for an accepted candidate bundle.
pub const CHANGE_SET_BUNDLE_INPUT: &str = "/octacity/protected/change-set.bundle";
/// Reserved protected-input destination for an accepted capture manifest.
pub const CHANGE_SET_MANIFEST_INPUT: &str = "/octacity/protected/change-set-manifest.json";
/// Reserved producer identity used only by the trusted Agent capture boundary.
pub const CHANGE_SET_CAPTURE_PRODUCER_ID: u64 = u64::MAX;
/// Maximum exact arguments in one command permission.
pub const MAX_FACTORY_COMMAND_ARGUMENTS: usize = 64;
/// Maximum combined UTF-8 bytes in one command argument pattern.
pub const MAX_FACTORY_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
/// Reserved read-only root for server-owned v3 inputs.
pub const PROTECTED_INPUT_ROOT: &str = "/octacity/protected";
/// Reserved writable root containing the exact materialized source revision.
pub const FACTORY_SOURCE_ROOT: &str = "/workspace/source";
/// Capability advertised by a harness plugin that blocks every protected tool
/// action until an external authorizer returns a disposition.
pub const BLOCKING_TOOL_AUTHORIZATION_CAPABILITY: &str = "codex.blocking-pre-tool-authorization.v1";
/// Reserved writable root for disposable Factory task state.
pub const FACTORY_SCRATCH_ROOT: &str = "/workspace/scratch";
/// Reserved writable root for declared Factory outputs.
pub const FACTORY_OUTPUT_ROOT: &str = "/workspace/output";

/// Exact immutable file metadata recorded by trusted ChangeSet capture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedChangeSetFileV1 {
  /// Portable file name relative to the private capture directory.
  pub name: String,
  /// Exact byte length.
  pub size_bytes: u64,
  /// Lowercase SHA-256 of the exact bytes.
  pub sha256: String,
}

/// One canonically ordered path transition in a captured candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedChangeSetPathV1 {
  /// Git plumbing status such as `A`, `D`, or `M`.
  pub status: String,
  /// Portable repository-relative path.
  pub path: String,
  /// Six-digit mode in the exact base tree.
  pub old_mode: String,
  /// Six-digit mode in the candidate tree.
  pub new_mode: String,
}

/// Canonical manifest produced outside the coding harness by trusted capture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedChangeSetManifestV1 {
  /// Manifest schema version; currently `1`.
  pub format_version: u16,
  /// Exact checked-out predecessor used as the candidate's direct parent.
  pub base_revision: String,
  /// Canonical candidate commit created by trusted capture.
  pub candidate_revision: String,
  /// Immutable Factory Stage Attempt identity.
  pub stage_attempt_id: String,
  /// Verified local capture-tool identity.
  pub capture_tool: FactoryImmutableReferenceV3,
  /// Canonically ordered changed paths and modes.
  pub changed_paths: Vec<CapturedChangeSetPathV1>,
  /// Required candidate bundle metadata.
  pub bundle: CapturedChangeSetFileV1,
  /// Optional bounded textual patch metadata.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub patch: Option<CapturedChangeSetFileV1>,
}

impl CapturedChangeSetManifestV1 {
  /// Decodes one bounded canonical manifest and validates its structural contract.
  pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CHANGE_SET_MANIFEST_BYTES {
      return Err("ChangeSet manifest exceeds its byte bound".to_owned());
    }
    let manifest: Self =
      serde_json::from_slice(bytes).map_err(|_| "ChangeSet manifest is not valid JSON".to_owned())?;
    manifest.validate()?;
    let mut canonical =
      serde_json::to_vec(&manifest).map_err(|_| "ChangeSet manifest is not serializable".to_owned())?;
    canonical.push(b'\n');
    if canonical != bytes {
      return Err("ChangeSet manifest is not canonical".to_owned());
    }
    Ok(manifest)
  }

  /// Validates provider-owned identities, file metadata, and canonical path order.
  pub fn validate(&self) -> Result<(), String> {
    if self.format_version != 1 {
      return Err("unsupported ChangeSet manifest version".to_owned());
    }
    validate_bounded_identity("base revision", &self.base_revision)?;
    validate_bounded_identity("candidate revision", &self.candidate_revision)?;
    validate_bounded_identity("stage attempt identity", &self.stage_attempt_id)?;
    self.capture_tool.validate_identity()?;
    self.bundle.validate(CHANGE_SET_BUNDLE_OUTPUT, None)?;
    if let Some(patch) = &self.patch {
      patch.validate(CHANGE_SET_PATCH_OUTPUT, Some(MAX_CHANGE_SET_PATCH_BYTES))?;
    }
    if self.changed_paths.len() > MAX_CHANGE_SET_CHANGED_PATHS as usize
      || !self.changed_paths.windows(2).all(|pair| pair[0].path < pair[1].path)
    {
      return Err("ChangeSet paths are not uniquely ordered within their bound".to_owned());
    }
    for path in &self.changed_paths {
      path.validate()?;
    }
    Ok(())
  }
}

impl CapturedChangeSetFileV1 {
  fn validate(&self, expected_name: &str, maximum_size: Option<u64>) -> Result<(), String> {
    if self.name != expected_name
      || self.sha256.len() != 64
      || !self
        .sha256
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
      || maximum_size.is_some_and(|maximum| self.size_bytes > maximum)
    {
      return Err("ChangeSet file metadata is invalid".to_owned());
    }
    Ok(())
  }
}

impl CapturedChangeSetPathV1 {
  /// Validates one portable changed-path transition.
  pub fn validate(&self) -> Result<(), String> {
    if !matches!(self.status.as_str(), "A" | "D" | "M" | "T")
      || !matches!(self.old_mode.as_str(), "000000" | "100644" | "100755" | "120000")
      || !matches!(self.new_mode.as_str(), "000000" | "100644" | "100755" | "120000")
      || self.path.is_empty()
      || self.path.len() > MAX_FACTORY_PATH_BYTES
      || self.path.starts_with('/')
      || self.path.ends_with('/')
      || self.path.contains(['\\', ':', '\0'])
      || self.path.chars().any(char::is_control)
      || self
        .path
        .split('/')
        .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
      return Err("ChangeSet path transition is invalid".to_owned());
    }
    Ok(())
  }
}

fn validate_bounded_identity(name: &str, value: &str) -> Result<(), String> {
  if value.is_empty() || value.len() > MAX_FACTORY_WIRE_IDENTITY_BYTES || value.chars().any(char::is_control) {
    return Err(format!("ChangeSet {name} is invalid"));
  }
  Ok(())
}

/// Program-owned Factory stage represented as causal metadata only.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryStageKindV3 {
  /// Candidate implementation.
  Implementation,
  /// Deterministic candidate validation.
  Validation,
  /// Independent candidate evaluation.
  Evaluation,
  /// Bounded candidate rework.
  Rework,
}

/// Type of immutable record that caused a Factory stage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryCausalRecordKindV3 {
  /// An earlier candidate ChangeSet.
  ChangeSet,
  /// A deterministic candidate Decision.
  Decision,
}

/// Optional immutable predecessor of one Factory stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCausalReferenceV3 {
  /// Stable record kind.
  pub kind: FactoryCausalRecordKindV3,
  /// Logical record identity.
  pub id: String,
  /// SHA-256 digest of the immutable record.
  pub digest: String,
}

/// Logical Factory provenance attached to an otherwise ordinary Build Job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCausalityV3 {
  /// Factory Run identity.
  pub factory_run_id: String,
  /// Immutable Factory Configuration identity.
  pub factory_configuration_id: String,
  /// Positive immutable Factory Configuration version.
  pub factory_configuration_version: u64,
  /// Append-only Stage Attempt identity.
  pub stage_attempt_id: String,
  /// Program-owned stage kind.
  pub stage_kind: FactoryStageKindV3,
  /// Digest of the immutable Task Envelope.
  pub task_envelope_digest: String,
  /// Digest of the exact subject revision and repository identity.
  pub subject_digest: String,
  /// Trusted capture instruction for a writable implementation or rework.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub change_set_capture: Option<ChangeSetCaptureV3>,
  /// Exact accepted candidate reconstructed before a later stage starts.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub change_set_materialization: Option<ChangeSetMaterializationV3>,
  /// Optional causal predecessor.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub parent: Option<FactoryCausalReferenceV3>,
}

impl FactoryCausalityV3 {
  pub(crate) fn validate(&self, inputs: &ProtectedInputManifestV3) -> Result<(), String> {
    for (name, value) in [
      ("factory.factory_run_id", self.factory_run_id.as_str()),
      (
        "factory.factory_configuration_id",
        self.factory_configuration_id.as_str(),
      ),
      ("factory.stage_attempt_id", self.stage_attempt_id.as_str()),
    ] {
      bounded_identity(name, value)?;
    }
    if self.factory_configuration_version == 0 {
      return Err("factory configuration version must be greater than zero".to_owned());
    }
    digest("factory.task_envelope_digest", &self.task_envelope_digest)?;
    digest("factory.subject_digest", &self.subject_digest)?;
    if let Some(capture) = &self.change_set_capture {
      capture.validate()?;
      if !matches!(
        self.stage_kind,
        FactoryStageKindV3::Implementation | FactoryStageKindV3::Rework
      ) {
        return Err("ChangeSet capture is valid only for writable Factory stages".to_owned());
      }
    }
    match (&self.change_set_materialization, self.stage_kind) {
      (None, FactoryStageKindV3::Implementation) => {}
      (
        Some(materialization),
        FactoryStageKindV3::Validation | FactoryStageKindV3::Evaluation | FactoryStageKindV3::Rework,
      ) => {
        materialization.validate(inputs)?;
      }
      (Some(_), FactoryStageKindV3::Implementation) => {
        return Err("ChangeSet materialization is valid only for later Factory stages".to_owned());
      }
      (None, FactoryStageKindV3::Validation | FactoryStageKindV3::Evaluation | FactoryStageKindV3::Rework) => {
        return Err("later Factory stages require exact ChangeSet materialization".to_owned());
      }
    }
    if let Some(parent) = &self.parent {
      bounded_identity("factory.parent.id", &parent.id)?;
      digest("factory.parent.digest", &parent.digest)?;
    }
    Ok(())
  }
}

/// Exact protected inputs used to reconstruct one accepted candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSetMaterializationV3 {
  /// Logical protected input containing the accepted Git bundle.
  pub bundle_input: String,
  /// Logical protected input containing the canonical capture manifest.
  pub manifest_input: String,
  /// Exact accepted candidate commit expected after reconstruction.
  pub candidate_revision: String,
}

impl ChangeSetMaterializationV3 {
  fn validate(&self, inputs: &ProtectedInputManifestV3) -> Result<(), String> {
    bounded_identity("factory.change_set_materialization.bundle_input", &self.bundle_input)?;
    bounded_identity(
      "factory.change_set_materialization.manifest_input",
      &self.manifest_input,
    )?;
    if self.bundle_input == self.manifest_input {
      return Err("ChangeSet materialization inputs must be distinct".to_owned());
    }
    if !(40..=64).contains(&self.candidate_revision.len())
      || !self
        .candidate_revision
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
      return Err("ChangeSet candidate revision must be a lowercase Git object identity".to_owned());
    }
    let bundle = inputs
      .inputs
      .iter()
      .find(|input| input.artifact_id == self.bundle_input)
      .ok_or_else(|| "ChangeSet bundle is absent from protected inputs".to_owned())?;
    let manifest = inputs
      .inputs
      .iter()
      .find(|input| input.artifact_id == self.manifest_input)
      .ok_or_else(|| "ChangeSet manifest is absent from protected inputs".to_owned())?;
    if bundle.media_type != CHANGE_SET_BUNDLE_MEDIA_TYPE
      || bundle.destination != CHANGE_SET_BUNDLE_INPUT
      || manifest.media_type != CHANGE_SET_MANIFEST_MEDIA_TYPE
      || manifest.destination != CHANGE_SET_MANIFEST_INPUT
    {
      return Err("ChangeSet protected inputs use an invalid type or destination".to_owned());
    }
    Ok(())
  }
}

/// Server-selected deterministic Git commit identity for trusted ChangeSet capture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSetCaptureV3 {
  /// Canonical author and committer display name.
  pub author_name: String,
  /// Canonical author and committer email address.
  pub author_email: String,
  /// Unix second derived from the immutable Stage Attempt creation claim.
  pub committed_at: u64,
  /// Optional bounded textual patch. The bundle and manifest are always required.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub patch_max_bytes: Option<u64>,
  /// Immutable changed-path, content, and size policy enforced by the Agent.
  pub policy: ChangeSetCapturePolicyV3,
}

/// Handling of binary blobs during trusted ChangeSet capture.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetBinaryPolicyV3 {
  /// Reject every changed blob detected as binary.
  Reject,
  /// Permit binary blobs within the same per-file and aggregate byte bounds.
  Allow,
}

/// Built-in secret detector selected by immutable capture policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetSecretPatternV3 {
  /// PEM private-key material.
  PemPrivateKey,
  /// AWS access-key identifiers.
  AwsAccessKeyId,
  /// GitHub personal, OAuth, user, server, refresh, or fine-grained tokens.
  GitHubToken,
  /// OpenAI project and legacy API keys.
  OpenAiApiKey,
}

/// Signed fail-closed policy for one trusted ChangeSet capture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSetCapturePolicyV3 {
  /// Canonically ordered repository-relative prefixes permitted to change.
  /// The single value `.` selects the complete repository tree.
  pub allowed_path_prefixes: Vec<String>,
  /// Canonically ordered repository control paths that remain immutable.
  pub forbidden_control_paths: Vec<String>,
  /// Positive maximum number of changed paths.
  pub max_changed_paths: u32,
  /// Positive maximum bytes in one changed candidate entry.
  pub max_file_bytes: u64,
  /// Positive maximum aggregate bytes across changed candidate entries.
  pub max_total_bytes: u64,
  /// Whether an unchanged workspace may produce a candidate commit.
  pub allow_empty: bool,
  /// Explicit handling of binary changed blobs.
  pub binary_policy: ChangeSetBinaryPolicyV3,
  /// Canonically ordered built-in secret detectors that fail capture closed.
  pub forbidden_secret_patterns: Vec<ChangeSetSecretPatternV3>,
}

impl ChangeSetCaptureV3 {
  /// Validates the bounded canonical commit instruction independently of Git.
  pub fn validate(&self) -> Result<(), String> {
    bounded_capture_author("factory.change_set_capture.author_name", &self.author_name)?;
    bounded_capture_author("factory.change_set_capture.author_email", &self.author_email)?;
    if !self.author_email.contains('@') {
      return Err("ChangeSet capture author email must contain '@'".to_owned());
    }
    if self.committed_at == 0 {
      return Err("ChangeSet capture time must be greater than zero".to_owned());
    }
    if self
      .patch_max_bytes
      .is_some_and(|bytes| bytes == 0 || bytes > MAX_CHANGE_SET_PATCH_BYTES)
    {
      return Err(format!(
        "ChangeSet patch byte bound must be between 1 and {MAX_CHANGE_SET_PATCH_BYTES}"
      ));
    }
    self.policy.validate()?;
    Ok(())
  }
}

impl ChangeSetCapturePolicyV3 {
  fn validate(&self) -> Result<(), String> {
    if self.allowed_path_prefixes.is_empty() || self.allowed_path_prefixes.len() > MAX_FACTORY_PERMISSION_ENTRIES {
      return Err("ChangeSet allowed path prefix count is outside protocol bounds".to_owned());
    }
    strictly_ordered(
      "factory.change_set_capture.policy.allowed_path_prefixes",
      &self.allowed_path_prefixes,
      |value| value,
    )?;
    strictly_ordered(
      "factory.change_set_capture.policy.forbidden_control_paths",
      &self.forbidden_control_paths,
      |value| value,
    )?;
    if self.forbidden_control_paths.len() > MAX_FACTORY_PERMISSION_ENTRIES {
      return Err("ChangeSet forbidden control path count is outside protocol bounds".to_owned());
    }
    for path in self.allowed_path_prefixes.iter().chain(&self.forbidden_control_paths) {
      portable_repository_path_prefix(path)?;
    }
    if self.max_changed_paths == 0 || self.max_changed_paths > MAX_CHANGE_SET_CHANGED_PATHS {
      return Err(format!(
        "ChangeSet changed path bound must be between 1 and {MAX_CHANGE_SET_CHANGED_PATHS}"
      ));
    }
    if self.max_file_bytes == 0 || self.max_file_bytes > MAX_CHANGE_SET_FILE_BYTES {
      return Err(format!(
        "ChangeSet file byte bound must be between 1 and {MAX_CHANGE_SET_FILE_BYTES}"
      ));
    }
    if self.max_total_bytes < self.max_file_bytes || self.max_total_bytes > MAX_CHANGE_SET_TOTAL_BYTES {
      return Err(format!(
        "ChangeSet total byte bound must be between max_file_bytes and {MAX_CHANGE_SET_TOTAL_BYTES}"
      ));
    }
    if self.forbidden_secret_patterns.is_empty()
      || self.forbidden_secret_patterns.len() > ChangeSetSecretPatternV3::ALL.len()
    {
      return Err("ChangeSet secret pattern selection is outside protocol bounds".to_owned());
    }
    strictly_ordered(
      "factory.change_set_capture.policy.forbidden_secret_patterns",
      &self.forbidden_secret_patterns,
      |value| value,
    )?;
    Ok(())
  }
}

impl ChangeSetSecretPatternV3 {
  /// Complete stable detector vocabulary for the v3 capture contract.
  pub const ALL: [Self; 4] = [
    Self::PemPrivateKey,
    Self::AwsAccessKeyId,
    Self::GitHubToken,
    Self::OpenAiApiKey,
  ];
}

fn portable_repository_path_prefix(value: &str) -> Result<(), String> {
  if value == "." {
    return Ok(());
  }
  if value.is_empty()
    || value.len() > MAX_FACTORY_PATH_BYTES
    || value.starts_with('/')
    || value.ends_with('/')
    || value.contains(['\\', ':', '\0'])
    || value.chars().any(char::is_control)
    || value
      .split('/')
      .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
  {
    Err("ChangeSet path prefix must be a canonical repository-relative portable path".to_owned())
  } else {
    Ok(())
  }
}

fn bounded_capture_author(name: &str, value: &str) -> Result<(), String> {
  if value.is_empty()
    || value.len() > MAX_CHANGE_SET_AUTHOR_BYTES
    || value.chars().any(|character| character.is_control())
  {
    return Err(format!(
      "{name} must contain 1 to {MAX_CHANGE_SET_AUTHOR_BYTES} bytes without control characters"
    ));
  }
  Ok(())
}

/// One immutable logical Artifact expected under the protected input root.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedInputV3 {
  /// Logical Artifact identity; no storage location or bearer capability.
  pub artifact_id: String,
  /// Exact expected byte count.
  pub size_bytes: u64,
  /// SHA-256 digest of the expected bytes.
  pub sha256: String,
  /// Bounded exact media type without parameters.
  pub media_type: String,
  /// Reserved absolute portable destination inside [`PROTECTED_INPUT_ROOT`].
  pub destination: String,
}

/// Canonically ordered immutable protected inputs for one v3 execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedInputManifestV3 {
  /// Inputs sorted by logical Artifact identity.
  pub inputs: Vec<ProtectedInputV3>,
}

impl ProtectedInputManifestV3 {
  /// Revalidates count, aggregate bytes, digests, media types, and destinations.
  pub fn validate(&self) -> Result<(), String> {
    if self.inputs.is_empty() || self.inputs.len() > MAX_PROTECTED_INPUTS {
      return Err("protected input count is outside protocol bounds".to_owned());
    }
    strictly_ordered("protected inputs", &self.inputs, |input| &input.artifact_id)?;
    let mut total = 0_u64;
    let mut destinations = Vec::with_capacity(self.inputs.len());
    for input in &self.inputs {
      bounded_identity("protected_inputs.artifact_id", &input.artifact_id)?;
      digest("protected_inputs.sha256", &input.sha256)?;
      media_type(&input.media_type)?;
      protected_destination(&input.destination)?;
      total = total
        .checked_add(input.size_bytes)
        .ok_or_else(|| "protected input byte total overflowed".to_owned())?;
      destinations.push(input.destination.as_str());
    }
    if total > MAX_PROTECTED_INPUT_BYTES {
      return Err("protected input bytes exceed the protocol bound".to_owned());
    }
    destinations.sort_unstable();
    if destinations.windows(2).any(|pair| path_contains(pair[0], pair[1])) {
      return Err("protected input destinations must not collide".to_owned());
    }
    Ok(())
  }

  pub(crate) fn contains(&self, artifact_id: &str) -> bool {
    self.inputs.iter().any(|input| input.artifact_id == artifact_id)
  }
}

/// Trusted managed Octa task selection for a protected v3 execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedOctaExecutionV3 {
  /// Logical protected input containing the server-generated Octafile.
  pub octafile_input: String,
  /// Exact non-empty Octa task names, in execution order.
  pub tasks: Vec<String>,
  /// Single logical secret profile selected for a credentialed trusted stage.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub credential_profile: Option<String>,
  /// Optional protected tool-control contract for tasks that may invoke tools.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tool_control: Option<FactoryToolControlV3>,
}

/// Strength of protected tool control required by one managed task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryToolControlModeV3 {
  /// Code-owned deterministic rules decide every in-envelope action.
  Deterministic,
  /// Ambiguous in-envelope actions may use a server-side Decision Signal.
  ToolRisk,
}

/// Exact blocking-hook capability required for one managed tool-using task.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryToolControlV3 {
  /// Plugin whose immutable executable installs the blocking hook.
  pub plugin: String,
  /// Exact semantic capability advertised by the pinned plugin release.
  pub capability: String,
  /// Maximum decision mechanism enabled for this task.
  pub mode: FactoryToolControlModeV3,
}

impl ManagedOctaExecutionV3 {
  pub(crate) fn validate(&self, inputs: &ProtectedInputManifestV3) -> Result<(), String> {
    bounded_identity("execution.octafile_input", &self.octafile_input)?;
    if !inputs.contains(&self.octafile_input) {
      return Err("managed Octafile is absent from protected inputs".to_owned());
    }
    if self.tasks.is_empty() || self.tasks.len() > MAX_FACTORY_PERMISSION_ENTRIES {
      return Err("managed task count is outside protocol bounds".to_owned());
    }
    let mut seen = std::collections::BTreeSet::new();
    for task in &self.tasks {
      bounded_identity("execution.tasks", task)?;
      if !seen.insert(task) {
        return Err("managed task names must not contain duplicates".to_owned());
      }
    }
    if let Some(profile) = &self.credential_profile {
      bounded_key("execution.credential_profile", profile)?;
    }
    if let Some(control) = &self.tool_control {
      bounded_key("execution.tool_control.plugin", &control.plugin)?;
      if control.capability != BLOCKING_TOOL_AUTHORIZATION_CAPABILITY {
        return Err("managed tool control requires the supported blocking-hook capability".to_owned());
      }
    }
    Ok(())
  }
}

/// Exact immutable plugin, executable, or tool identity.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryImmutableReferenceV3 {
  /// Provider-neutral logical identity.
  pub identity: String,
  /// Exact immutable version.
  pub version: String,
  /// SHA-256 digest of the immutable implementation.
  pub sha256: String,
}

impl FactoryImmutableReferenceV3 {
  /// Validates the bounded logical identity, exact version, and SHA-256 digest.
  pub fn validate_identity(&self) -> Result<(), String> {
    self.validate("immutable reference")
  }

  pub(crate) fn validate(&self, name: &str) -> Result<(), String> {
    bounded_key(name, &self.identity)?;
    bounded_identity(name, &self.version)?;
    digest(name, &self.sha256)
  }
}

/// One fixed-position argument constraint.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryCommandArgumentV3 {
  /// Require one exact bounded value.
  Exact {
    /// Exact argument bytes.
    value: String,
  },
  /// Allow any one argument at this position up to an explicit UTF-8 byte ceiling.
  Any {
    /// Positive per-argument ceiling included in the aggregate pattern bound.
    max_bytes: u32,
  },
}

/// One allowed executable and fixed-length argument pattern.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCommandPermissionV3 {
  /// Exact executable identity also present in `executables`.
  pub executable: FactoryImmutableReferenceV3,
  /// Fixed-length argument constraints.
  pub arguments: Vec<FactoryCommandArgumentV3>,
}

impl FactoryCommandPermissionV3 {
  /// Checks concrete arguments against the fixed-length bounded pattern.
  #[must_use]
  pub fn permits_arguments(&self, arguments: &[String]) -> bool {
    let aggregate_bytes = arguments
      .iter()
      .try_fold(0_usize, |total, argument| total.checked_add(argument.len()));
    self.arguments.len() == arguments.len()
      && aggregate_bytes.is_some_and(|bytes| bytes <= MAX_FACTORY_COMMAND_ARGUMENT_BYTES)
      && self
        .arguments
        .iter()
        .zip(arguments)
        .all(|(pattern, argument)| match pattern {
          FactoryCommandArgumentV3::Exact { value } => argument == value,
          FactoryCommandArgumentV3::Any { max_bytes } => {
            usize::try_from(*max_bytes).is_ok_and(|max_bytes| argument.len() <= max_bytes)
          }
        })
  }
}

/// Maximum access for one portable filesystem root.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryMountModeV3 {
  /// Read-only projection.
  ReadOnly,
  /// Read-write projection.
  ReadWrite,
}

/// One portable filesystem permission.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryMountPermissionV3 {
  /// Canonical absolute portable root.
  pub root: String,
  /// Maximum access granted at that root.
  pub mode: FactoryMountModeV3,
}

/// Independent resource ceilings for one managed execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryResourceLimitsV3 {
  /// CPU allocation in millicores.
  pub cpu_millis: u32,
  /// Memory ceiling in bytes.
  pub memory_bytes: u64,
  /// Writable-disk ceiling in bytes.
  pub disk_bytes: u64,
  /// Total process-count ceiling, including the primary process.
  pub process_count: u32,
  /// Elapsed-time ceiling in milliseconds.
  pub elapsed_millis: u64,
}

/// Output kinds and aggregate publication ceilings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryOutputPermissionsV3 {
  /// Canonically ordered logical output kinds.
  pub kinds: Vec<String>,
  /// Maximum Artifact count.
  pub max_artifact_count: u32,
  /// Maximum aggregate Artifact bytes.
  pub max_artifact_bytes: u64,
  /// Maximum report count.
  pub max_report_count: u32,
  /// Maximum aggregate report bytes.
  pub max_report_bytes: u64,
}

/// Deny-by-default signed Factory execution authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPermissionSetV3 {
  /// Exact permitted task-plugin identities in canonical order.
  pub plugins: Vec<FactoryImmutableReferenceV3>,
  /// Exact permitted executable identities in canonical order.
  pub executables: Vec<FactoryImmutableReferenceV3>,
  /// Exact permitted tool identities in canonical order.
  pub tools: Vec<FactoryImmutableReferenceV3>,
  /// Exact command patterns in canonical order.
  pub commands: Vec<FactoryCommandPermissionV3>,
  /// Maximum descendants of the primary process.
  pub max_descendants: u32,
  /// Non-overlapping portable mounts in canonical order.
  pub mounts: Vec<FactoryMountPermissionV3>,
  /// Exact permitted DNS names or IP addresses in canonical order.
  pub network_hosts: Vec<String>,
  /// Logical secret profiles in canonical order.
  pub secret_profiles: Vec<String>,
  /// Logical workload-identity profiles in canonical order.
  pub workload_identity_profiles: Vec<String>,
  /// Independent resource ceilings.
  pub resources: FactoryResourceLimitsV3,
  /// Bounded output authority.
  pub outputs: FactoryOutputPermissionsV3,
}

impl FactoryPermissionSetV3 {
  /// Revalidates canonical ordering, bounds, and internal references.
  pub fn validate(&self) -> Result<(), String> {
    for (name, references) in [
      ("permissions.plugins", self.plugins.as_slice()),
      ("permissions.executables", self.executables.as_slice()),
      ("permissions.tools", self.tools.as_slice()),
    ] {
      bounded_count(name, references.len())?;
      strictly_ordered(name, references, |reference| &reference.identity)?;
      for reference in references {
        reference.validate(name)?;
      }
    }
    bounded_count("permissions.commands", self.commands.len())?;
    strictly_ordered("permissions.commands", &self.commands, |command| command)?;
    for command in &self.commands {
      if !self.executables.contains(&command.executable) {
        return Err("command executable is absent from the executable allowlist".to_owned());
      }
      if command.arguments.len() > MAX_FACTORY_COMMAND_ARGUMENTS {
        return Err("command argument count exceeds the protocol bound".to_owned());
      }
      let bytes = command.arguments.iter().try_fold(0_usize, |total, argument| {
        let bytes = match argument {
          FactoryCommandArgumentV3::Exact { value } => value.len(),
          FactoryCommandArgumentV3::Any { max_bytes } => usize::try_from(*max_bytes).ok()?,
        };
        total.checked_add(bytes)
      });
      if bytes.is_none_or(|bytes| bytes > MAX_FACTORY_COMMAND_ARGUMENT_BYTES) {
        return Err("command argument bytes exceed the protocol bound".to_owned());
      }
      for argument in &command.arguments {
        match argument {
          FactoryCommandArgumentV3::Exact { value } => {
            bounded_text(
              "permissions.commands.arguments",
              value,
              MAX_FACTORY_COMMAND_ARGUMENT_BYTES,
            )?;
          }
          FactoryCommandArgumentV3::Any { max_bytes }
            if *max_bytes == 0
              || usize::try_from(*max_bytes).is_err()
              || usize::try_from(*max_bytes).is_ok_and(|value| value > MAX_FACTORY_COMMAND_ARGUMENT_BYTES) =>
          {
            return Err("wildcard command argument bound is outside the protocol limit".to_owned());
          }
          FactoryCommandArgumentV3::Any { .. } => {}
        }
      }
    }
    if self.resources.cpu_millis == 0
      || self.resources.memory_bytes == 0
      || self.resources.disk_bytes == 0
      || self.resources.process_count == 0
      || self.resources.elapsed_millis == 0
    {
      return Err("managed execution requires positive Factory resource ceilings".to_owned());
    }
    if self.max_descendants >= self.resources.process_count {
      return Err("descendant ceiling must leave capacity for the primary process".to_owned());
    }
    bounded_count("permissions.mounts", self.mounts.len())?;
    strictly_ordered("permissions.mounts", &self.mounts, |mount| &mount.root)?;
    for mount in &self.mounts {
      portable_absolute_path("permissions.mounts.root", &mount.root)?;
    }
    for (index, mount) in self.mounts.iter().enumerate() {
      if self
        .mounts
        .iter()
        .skip(index + 1)
        .any(|other| path_contains(&mount.root, &other.root) || path_contains(&other.root, &mount.root))
      {
        return Err("permission mount roots must not overlap".to_owned());
      }
    }
    validate_hosts(&self.network_hosts)?;
    validate_keys("permissions.secret_profiles", &self.secret_profiles)?;
    validate_keys(
      "permissions.workload_identity_profiles",
      &self.workload_identity_profiles,
    )?;
    self.outputs.validate()
  }
}

impl FactoryOutputPermissionsV3 {
  fn validate(&self) -> Result<(), String> {
    validate_keys("permissions.outputs.kinds", &self.kinds)?;
    if (self.max_artifact_count == 0) != (self.max_artifact_bytes == 0)
      || (self.max_report_count == 0) != (self.max_report_bytes == 0)
    {
      return Err("Factory output counts and bytes must be enabled together".to_owned());
    }
    let enabled = self.max_artifact_count > 0 || self.max_report_count > 0;
    if enabled != !self.kinds.is_empty() {
      return Err("Factory output kinds must be present exactly when outputs are enabled".to_owned());
    }
    Ok(())
  }
}

/// Provider-neutral enforcement vocabulary required by JobSpec v3.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryEnforcementCapabilityV3 {
  /// Exact plugin identity validation.
  PluginIdentity,
  /// Exact executable identity validation.
  ExecutableIdentity,
  /// Exact tool identity validation.
  ToolIdentity,
  /// Command and fixed-position argument enforcement.
  CommandArguments,
  /// Descendant-process control.
  DescendantProcesses,
  /// Portable filesystem boundary enforcement.
  FilesystemPaths,
  /// Read-only/read-write mount enforcement.
  MountModes,
  /// Exact network-host allowlisting.
  NetworkHosts,
  /// Secret-profile scoping.
  SecretProfiles,
  /// Workload-identity profile scoping.
  WorkloadIdentityProfiles,
  /// CPU enforcement.
  Cpu,
  /// Memory enforcement.
  Memory,
  /// Writable-disk enforcement.
  Disk,
  /// Process-count enforcement.
  ProcessCount,
  /// Elapsed-time enforcement.
  ElapsedTime,
  /// Output kind and quota enforcement.
  Outputs,
  /// Size, digest, and media verification for protected inputs.
  ProtectedInputIntegrity,
  /// Read-only protected-input publication.
  ReadOnlyProtectedInputs,
  /// Separate protected-input, source, scratch, and output roots.
  WorkspaceSeparation,
  /// Server-owned Octafile and task selection.
  ManagedOctaExecution,
}

impl FactoryEnforcementCapabilityV3 {
  /// Complete ordered v3 enforcement vocabulary.
  pub const ALL: [Self; 20] = [
    Self::PluginIdentity,
    Self::ExecutableIdentity,
    Self::ToolIdentity,
    Self::CommandArguments,
    Self::DescendantProcesses,
    Self::FilesystemPaths,
    Self::MountModes,
    Self::NetworkHosts,
    Self::SecretProfiles,
    Self::WorkloadIdentityProfiles,
    Self::Cpu,
    Self::Memory,
    Self::Disk,
    Self::ProcessCount,
    Self::ElapsedTime,
    Self::Outputs,
    Self::ProtectedInputIntegrity,
    Self::ReadOnlyProtectedInputs,
    Self::WorkspaceSeparation,
    Self::ManagedOctaExecution,
  ];
}

pub(crate) fn validate_factory_execution(
  runtime: &RuntimeSpecV2,
  outputs: &OutputLimits,
  execution: &ManagedOctaExecutionV3,
  factory: Option<&FactoryCausalityV3>,
  permissions: &FactoryPermissionSetV3,
  required: &[FactoryEnforcementCapabilityV3],
) -> Result<(), String> {
  if runtime.target.mode == ExecutionMode::Host {
    return Err("JobSpec v3 requires an isolation or virtualization boundary".to_owned());
  }
  if required != FactoryEnforcementCapabilityV3::ALL {
    return Err("JobSpec v3 requires the complete ordered enforcement vocabulary".to_owned());
  }
  let source_mode = if factory.is_some_and(|factory| factory.stage_kind == FactoryStageKindV3::Evaluation) {
    FactoryMountModeV3::ReadOnly
  } else {
    FactoryMountModeV3::ReadWrite
  };
  let required_mounts = [
    FactoryMountPermissionV3 {
      root: PROTECTED_INPUT_ROOT.to_owned(),
      mode: FactoryMountModeV3::ReadOnly,
    },
    FactoryMountPermissionV3 {
      root: FACTORY_OUTPUT_ROOT.to_owned(),
      mode: FactoryMountModeV3::ReadWrite,
    },
    FactoryMountPermissionV3 {
      root: FACTORY_SCRATCH_ROOT.to_owned(),
      mode: FactoryMountModeV3::ReadWrite,
    },
    FactoryMountPermissionV3 {
      root: FACTORY_SOURCE_ROOT.to_owned(),
      mode: source_mode,
    },
  ];
  if permissions.mounts.as_slice() != required_mounts.as_slice() {
    return Err("Factory permissions must exactly authorize the protected/source/scratch/output layout".to_owned());
  }
  let resources = permissions.resources;
  if resources.cpu_millis > runtime.cpu_millis
    || resources.memory_bytes > runtime.memory_bytes
    || resources.disk_bytes > runtime.writable_disk_bytes
    || resources.elapsed_millis > runtime.timeout_seconds.saturating_mul(1_000)
  {
    return Err("Factory permission resources exceed signed runtime limits".to_owned());
  }
  match &runtime.network {
    NetworkPolicy::Unrestricted => return Err("JobSpec v3 does not permit unrestricted network access".to_owned()),
    NetworkPolicy::Disabled if !permissions.network_hosts.is_empty() => {
      return Err("disabled runtime network cannot grant Factory hosts".to_owned());
    }
    NetworkPolicy::Restricted { allowed_hosts } if allowed_hosts != &permissions.network_hosts => {
      return Err("Factory runtime and permission network allowlists must match exactly".to_owned());
    }
    NetworkPolicy::Disabled | NetworkPolicy::Restricted { .. } => {}
  }
  if runtime
    .workload_identity_profile
    .as_ref()
    .is_some_and(|profile| !permissions.workload_identity_profiles.contains(profile))
  {
    return Err("runtime workload identity is absent from Factory permissions".to_owned());
  }
  match &execution.credential_profile {
    Some(profile) if permissions.secret_profiles.as_slice() == [profile.as_str()] => {}
    None if permissions.secret_profiles.is_empty() => {}
    Some(_) | None => {
      return Err("managed execution must select exactly its stage-scoped Factory credential profile".to_owned());
    }
  }
  let permitted = &permissions.outputs;
  if permitted.max_artifact_count > outputs.artifact_count
    || permitted.max_artifact_bytes > outputs.artifact_bytes
    || permitted.max_report_count > outputs.report_count
    || permitted.max_report_bytes > outputs.report_bytes
  {
    return Err("Factory output permissions exceed signed output limits".to_owned());
  }
  Ok(())
}

pub(crate) fn validate_factory_toolchain(
  octa: &OctaSpec,
  execution: &ManagedOctaExecutionV3,
  permissions: &FactoryPermissionSetV3,
) -> Result<(), String> {
  if octa.plugin_digests.len() != permissions.plugins.len()
    || octa.plugin_digests.iter().any(|(identity, digest)| {
      !permissions
        .plugins
        .iter()
        .any(|plugin| plugin.identity == *identity && plugin.sha256 == *digest)
    })
  {
    return Err("Factory plugin permissions do not match the signed Octa plugin set".to_owned());
  }
  match (&execution.tool_control, permissions.tools.is_empty()) {
    (None, true) => {}
    (Some(control), false)
      if permissions
        .plugins
        .iter()
        .any(|plugin| plugin.identity == control.plugin) => {}
    (None, false) => return Err("Factory tool authority requires blocking tool control".to_owned()),
    (Some(_), true) => return Err("Factory tool control is present without tool authority".to_owned()),
    (Some(_), false) => return Err("Factory tool-control plugin is absent from signed permissions".to_owned()),
  }
  Ok(())
}

fn bounded_count(name: &str, count: usize) -> Result<(), String> {
  if count > MAX_FACTORY_PERMISSION_ENTRIES {
    Err(format!("{name} exceeds the protocol entry bound"))
  } else {
    Ok(())
  }
}

fn bounded_identity(name: &str, value: &str) -> Result<(), String> {
  bounded_text(name, value, MAX_FACTORY_WIRE_IDENTITY_BYTES)
}

fn bounded_text(name: &str, value: &str, maximum: usize) -> Result<(), String> {
  if value.is_empty() || value.len() > maximum || value.trim() != value || value.chars().any(char::is_control) {
    Err(format!("{name} is empty, oversized, or diagnostic-unsafe"))
  } else {
    Ok(())
  }
}

pub(crate) fn bounded_key(name: &str, value: &str) -> Result<(), String> {
  bounded_identity(name, value)?;
  let mut bytes = value.bytes();
  if !bytes
    .next()
    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-'))
  {
    return Err(format!("{name} is not a canonical Factory key"));
  }
  Ok(())
}

fn validate_keys(name: &str, values: &[String]) -> Result<(), String> {
  bounded_count(name, values.len())?;
  strictly_ordered(name, values, |value| value)?;
  for value in values {
    bounded_key(name, value)?;
  }
  Ok(())
}

fn validate_hosts(hosts: &[String]) -> Result<(), String> {
  bounded_count("permissions.network_hosts", hosts.len())?;
  strictly_ordered("permissions.network_hosts", hosts, |host| host)?;
  for host in hosts {
    validate_host("permissions.network_hosts", host)?;
  }
  Ok(())
}

fn valid_dns_label(label: &str) -> bool {
  !label.is_empty()
    && label.len() <= 63
    && label
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    && label.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
    && label.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
}

fn digest(name: &str, value: &str) -> Result<(), String> {
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

pub(crate) fn validate_host(name: &str, value: &str) -> Result<(), String> {
  let canonical_ip = value
    .parse::<IpAddr>()
    .is_ok_and(|address| address.to_string() == value);
  let canonical_dns = value.len() <= 253 && value.split('.').all(valid_dns_label);
  if !canonical_ip && !canonical_dns {
    Err(format!("{name} is not a canonical DNS name or IP address"))
  } else {
    Ok(())
  }
}

pub(crate) fn portable_absolute_path(name: &str, value: &str) -> Result<(), String> {
  if value.is_empty()
    || value.len() > MAX_FACTORY_PATH_BYTES
    || !value.starts_with('/')
    || (value.len() > 1 && value.ends_with('/'))
    || value.contains('\\')
    || value.contains(':')
    || value.chars().any(char::is_control)
    || value
      .split('/')
      .skip(1)
      .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
  {
    Err(format!("{name} must be a canonical absolute portable path"))
  } else {
    Ok(())
  }
}

fn protected_destination(value: &str) -> Result<(), String> {
  portable_absolute_path("protected_inputs.destination", value)?;
  if value == PROTECTED_INPUT_ROOT || !path_contains(PROTECTED_INPUT_ROOT, value) {
    return Err("protected input destination is outside the reserved root".to_owned());
  }
  Ok(())
}

pub(crate) fn path_contains(root: &str, candidate: &str) -> bool {
  candidate == root
    || candidate
      .strip_prefix(root)
      .is_some_and(|suffix| suffix.starts_with('/'))
}

fn media_type(value: &str) -> Result<(), String> {
  if value.len() > 127 || value.bytes().any(|byte| !byte.is_ascii() || byte.is_ascii_control()) {
    return Err("protected input media type is invalid".to_owned());
  }
  let Some((kind, subtype)) = value.split_once('/') else {
    return Err("protected input media type is invalid".to_owned());
  };
  if subtype.contains('/') || !media_token(kind) || !media_token(subtype) {
    return Err("protected input media type is invalid".to_owned());
  }
  Ok(())
}

fn media_token(value: &str) -> bool {
  !value.is_empty()
    && value.bytes().all(|byte| {
      byte.is_ascii_lowercase()
        || byte.is_ascii_digit()
        || matches!(byte, b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-')
    })
}

fn strictly_ordered<T, K: Ord + ?Sized>(name: &str, values: &[T], key: impl Fn(&T) -> &K) -> Result<(), String> {
  if values.windows(2).any(|pair| key(&pair[0]) >= key(&pair[1])) {
    Err(format!("{name} must be canonically ordered without duplicates"))
  } else {
    Ok(())
  }
}
