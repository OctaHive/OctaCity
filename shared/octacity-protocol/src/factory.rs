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
/// Maximum exact arguments in one command permission.
pub const MAX_FACTORY_COMMAND_ARGUMENTS: usize = 64;
/// Maximum combined UTF-8 bytes in one command argument pattern.
pub const MAX_FACTORY_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
/// Reserved read-only root for server-owned v3 inputs.
pub const PROTECTED_INPUT_ROOT: &str = "/octacity/protected";

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
  /// Optional causal predecessor.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub parent: Option<FactoryCausalReferenceV3>,
}

impl FactoryCausalityV3 {
  pub(crate) fn validate(&self) -> Result<(), String> {
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
    if let Some(parent) = &self.parent {
      bounded_identity("factory.parent.id", &parent.id)?;
      digest("factory.parent.digest", &parent.digest)?;
    }
    Ok(())
  }
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
  fn validate(&self, name: &str) -> Result<(), String> {
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
  permissions: &FactoryPermissionSetV3,
  required: &[FactoryEnforcementCapabilityV3],
) -> Result<(), String> {
  if runtime.target.mode == ExecutionMode::Host {
    return Err("JobSpec v3 requires an isolation or virtualization boundary".to_owned());
  }
  if required != FactoryEnforcementCapabilityV3::ALL {
    return Err("JobSpec v3 requires the complete ordered enforcement vocabulary".to_owned());
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
    NetworkPolicy::Restricted { allowed_hosts }
      if permissions
        .network_hosts
        .iter()
        .any(|host| !allowed_hosts.contains(host)) =>
    {
      return Err("Factory network hosts exceed the signed runtime allowlist".to_owned());
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

pub(crate) fn validate_factory_toolchain(octa: &OctaSpec, permissions: &FactoryPermissionSetV3) -> Result<(), String> {
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

fn bounded_key(name: &str, value: &str) -> Result<(), String> {
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
    let canonical_ip = host.parse::<IpAddr>().is_ok_and(|address| address.to_string() == *host);
    let canonical_dns = host.len() <= 253 && host.split('.').all(valid_dns_label);
    if !canonical_ip && !canonical_dns {
      return Err("Factory network host is not a canonical DNS name or IP address".to_owned());
    }
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

fn portable_absolute_path(name: &str, value: &str) -> Result<(), String> {
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

fn path_contains(root: &str, candidate: &str) -> bool {
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
