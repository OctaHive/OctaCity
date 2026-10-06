use std::{collections::BTreeSet, net::IpAddr, str::FromStr};

use crate::{
  FactoryError, FactoryKey, FactoryText, ImmutableReference, MAX_FACTORY_ELAPSED_MILLIS, MAX_FACTORY_OUTPUT_BYTES,
  MountMode, PermissionCategory,
};

/// Maximum grants accepted in any one permission category.
pub const MAX_PERMISSION_ENTRIES_PER_CATEGORY: usize = 256;
/// Maximum arguments in one exact command pattern.
pub const MAX_COMMAND_ARGUMENTS: usize = 64;
/// Maximum combined UTF-8 bytes in one command argument pattern.
pub const MAX_COMMAND_ARGUMENT_BYTES: usize = 16 * 1024;
/// Maximum bytes in one canonical portable execution path.
pub const MAX_PERMISSION_PATH_BYTES: usize = 1024;
/// Maximum bytes in one canonical DNS name or IP address.
pub const MAX_PERMISSION_HOST_BYTES: usize = 253;
/// Maximum CPU allocation expressed in millicores.
pub const MAX_FACTORY_CPU_MILLIS: u32 = 1_000_000;
/// Maximum memory allocation accepted by the domain model.
pub const MAX_FACTORY_MEMORY_BYTES: u64 = 1024 * 1024 * 1024 * 1024 * 1024;
/// Maximum disk allocation accepted by the domain model.
pub const MAX_FACTORY_DISK_BYTES: u64 = 1024 * 1024 * 1024 * 1024 * 1024;
/// Maximum total or descendant process count.
pub const MAX_FACTORY_PROCESS_COUNT: u32 = 1_000_000;
/// Maximum Artifact or report count.
pub const MAX_FACTORY_OUTPUT_COUNT: u32 = 1_000_000;

/// One position in a bounded, fixed-length command argument pattern.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CommandArgumentPattern {
  /// The argument must equal this exact bounded value.
  Exact(FactoryText),
  /// Any single bounded argument is allowed at this position.
  Any,
}

impl CommandArgumentPattern {
  fn intersection(&self, other: &Self) -> Option<Self> {
    match (self, other) {
      (Self::Exact(left), Self::Exact(right)) if left == right => Some(Self::Exact(left.clone())),
      (Self::Exact(value), Self::Any) | (Self::Any, Self::Exact(value)) => Some(Self::Exact(value.clone())),
      (Self::Any, Self::Any) => Some(Self::Any),
      (Self::Exact(_), Self::Exact(_)) => None,
    }
  }
}

/// Exact executable identity and one fixed-length bounded argument pattern.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CommandPermission {
  executable: ImmutableReference,
  arguments: Vec<CommandArgumentPattern>,
}

impl CommandPermission {
  /// Constructs a bounded command pattern without regex or variadic matching.
  pub fn try_new(executable: ImmutableReference, arguments: Vec<CommandArgumentPattern>) -> Result<Self, FactoryError> {
    let argument_bytes = arguments.iter().try_fold(0_usize, |total, argument| {
      let bytes = match argument {
        CommandArgumentPattern::Exact(value) => value.as_str().len(),
        CommandArgumentPattern::Any => 0,
      };
      total.checked_add(bytes)
    });
    if arguments.len() > MAX_COMMAND_ARGUMENTS || argument_bytes.is_none_or(|bytes| bytes > MAX_COMMAND_ARGUMENT_BYTES)
    {
      return Err(invalid(PermissionCategory::CommandArguments));
    }
    Ok(Self { executable, arguments })
  }

  /// Returns the exact executable identity.
  #[must_use]
  pub const fn executable(&self) -> &ImmutableReference {
    &self.executable
  }

  /// Returns the fixed-length argument pattern.
  #[must_use]
  pub fn arguments(&self) -> &[CommandArgumentPattern] {
    &self.arguments
  }

  fn intersection(&self, other: &Self) -> Option<Self> {
    if self.executable != other.executable || self.arguments.len() != other.arguments.len() {
      return None;
    }
    let arguments = self
      .arguments
      .iter()
      .zip(&other.arguments)
      .map(|(left, right)| left.intersection(right))
      .collect::<Option<Vec<_>>>()?;
    Some(Self {
      executable: self.executable.clone(),
      arguments,
    })
  }
}

/// Canonical absolute path in the portable Factory execution namespace.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactoryPath(String);

impl FactoryPath {
  /// Parses an absolute slash-separated path without ambiguous segments.
  pub fn new(value: impl Into<String>) -> Result<Self, FactoryError> {
    let value = value.into();
    let valid = !value.is_empty()
      && value.len() <= MAX_PERMISSION_PATH_BYTES
      && value.starts_with('/')
      && (value == "/" || !value.ends_with('/'))
      && !value.contains('\\')
      && !value.chars().any(char::is_control)
      && (value == "/"
        || value
          .split('/')
          .skip(1)
          .all(|segment| !segment.is_empty() && !matches!(segment, "." | "..")));
    if !valid {
      return Err(invalid(PermissionCategory::FilesystemPaths));
    }
    Ok(Self(value))
  }

  /// Borrows the canonical path.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  fn contains(&self, other: &Self) -> bool {
    self == other
      || self.0 == "/"
      || other
        .0
        .strip_prefix(&self.0)
        .is_some_and(|suffix| suffix.starts_with('/'))
  }
}

/// One canonical filesystem root and its maximum mount access.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MountPermission {
  root: FactoryPath,
  mode: MountMode,
}

impl MountPermission {
  /// Constructs a mount grant for one canonical execution path.
  #[must_use]
  pub const fn new(root: FactoryPath, mode: MountMode) -> Self {
    Self { root, mode }
  }

  /// Returns the canonical granted root.
  #[must_use]
  pub const fn root(&self) -> &FactoryPath {
    &self.root
  }

  /// Returns the maximum granted access mode.
  #[must_use]
  pub const fn mode(&self) -> MountMode {
    self.mode
  }

  fn intersection(&self, other: &Self) -> Option<Self> {
    let root = if self.root.contains(&other.root) {
      other.root.clone()
    } else if other.root.contains(&self.root) {
      self.root.clone()
    } else {
      return None;
    };
    let mode = if self.mode == MountMode::ReadOnly || other.mode == MountMode::ReadOnly {
      MountMode::ReadOnly
    } else {
      MountMode::ReadWrite
    };
    Some(Self { root, mode })
  }
}

/// Canonical exact DNS name or IP address allowed for network access.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NetworkHost(String);

impl NetworkHost {
  /// Parses a lowercase DNS name or canonical IP address without a scheme, port, or wildcard.
  pub fn new(value: impl Into<String>) -> Result<Self, FactoryError> {
    let value = value.into();
    let canonical_ip = IpAddr::from_str(&value).is_ok_and(|address| address.to_string() == value);
    let canonical_dns = value.len() <= MAX_PERMISSION_HOST_BYTES
      && value.bytes().any(|byte| byte.is_ascii_lowercase())
      && value.split('.').all(valid_dns_label);
    if value.is_empty() || (!canonical_ip && !canonical_dns) {
      return Err(invalid(PermissionCategory::NetworkHosts));
    }
    Ok(Self(value))
  }

  /// Borrows the canonical host.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Independent resource ceilings for one Factory execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactoryResourceLimits {
  cpu_millis: u32,
  memory_bytes: u64,
  disk_bytes: u64,
  process_count: u32,
  elapsed_millis: u64,
}

impl FactoryResourceLimits {
  /// Constructs bounded resource ceilings; zero denies use of that resource.
  pub const fn new(
    cpu_millis: u32,
    memory_bytes: u64,
    disk_bytes: u64,
    process_count: u32,
    elapsed_millis: u64,
  ) -> Result<Self, FactoryError> {
    if cpu_millis > MAX_FACTORY_CPU_MILLIS {
      return Err(invalid(PermissionCategory::Cpu));
    }
    if memory_bytes > MAX_FACTORY_MEMORY_BYTES {
      return Err(invalid(PermissionCategory::Memory));
    }
    if disk_bytes > MAX_FACTORY_DISK_BYTES {
      return Err(invalid(PermissionCategory::Disk));
    }
    if process_count > MAX_FACTORY_PROCESS_COUNT {
      return Err(invalid(PermissionCategory::ProcessCount));
    }
    if elapsed_millis > MAX_FACTORY_ELAPSED_MILLIS {
      return Err(invalid(PermissionCategory::ElapsedTime));
    }
    Ok(Self {
      cpu_millis,
      memory_bytes,
      disk_bytes,
      process_count,
      elapsed_millis,
    })
  }

  /// Returns the CPU ceiling in millicores.
  #[must_use]
  pub const fn cpu_millis(self) -> u32 {
    self.cpu_millis
  }

  /// Returns the memory ceiling in bytes.
  #[must_use]
  pub const fn memory_bytes(self) -> u64 {
    self.memory_bytes
  }

  /// Returns the disk ceiling in bytes.
  #[must_use]
  pub const fn disk_bytes(self) -> u64 {
    self.disk_bytes
  }

  /// Returns the total process-count ceiling.
  #[must_use]
  pub const fn process_count(self) -> u32 {
    self.process_count
  }

  /// Returns the elapsed-time ceiling in milliseconds.
  #[must_use]
  pub const fn elapsed_millis(self) -> u64 {
    self.elapsed_millis
  }

  fn intersection(self, other: Self) -> Self {
    Self {
      cpu_millis: self.cpu_millis.min(other.cpu_millis),
      memory_bytes: self.memory_bytes.min(other.memory_bytes),
      disk_bytes: self.disk_bytes.min(other.disk_bytes),
      process_count: self.process_count.min(other.process_count),
      elapsed_millis: self.elapsed_millis.min(other.elapsed_millis),
    }
  }
}

/// Bounded Artifact/report publication authority for one Factory execution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryOutputPermissions {
  kinds: BTreeSet<FactoryKey>,
  max_artifact_count: u32,
  max_artifact_bytes: u64,
  max_report_count: u32,
  max_report_bytes: u64,
}

impl FactoryOutputPermissions {
  /// Constructs output allowlists and ceilings; zero denies the corresponding output.
  pub fn try_new(
    kinds: Vec<FactoryKey>,
    max_artifact_count: u32,
    max_artifact_bytes: u64,
    max_report_count: u32,
    max_report_bytes: u64,
  ) -> Result<Self, FactoryError> {
    if max_artifact_count > MAX_FACTORY_OUTPUT_COUNT
      || max_report_count > MAX_FACTORY_OUTPUT_COUNT
      || max_artifact_bytes > MAX_FACTORY_OUTPUT_BYTES
      || max_report_bytes > MAX_FACTORY_OUTPUT_BYTES
    {
      return Err(invalid(PermissionCategory::Outputs));
    }
    Ok(Self {
      kinds: bounded_set(kinds, PermissionCategory::Outputs)?,
      max_artifact_count,
      max_artifact_bytes,
      max_report_count,
      max_report_bytes,
    })
  }

  /// Iterates over allowed output kinds in canonical order.
  pub fn kinds(&self) -> impl ExactSizeIterator<Item = &FactoryKey> {
    self.kinds.iter()
  }

  /// Returns the Artifact-count ceiling.
  #[must_use]
  pub const fn max_artifact_count(&self) -> u32 {
    self.max_artifact_count
  }

  /// Returns the aggregate Artifact-byte ceiling.
  #[must_use]
  pub const fn max_artifact_bytes(&self) -> u64 {
    self.max_artifact_bytes
  }

  /// Returns the report-count ceiling.
  #[must_use]
  pub const fn max_report_count(&self) -> u32 {
    self.max_report_count
  }

  /// Returns the aggregate report-byte ceiling.
  #[must_use]
  pub const fn max_report_bytes(&self) -> u64 {
    self.max_report_bytes
  }

  fn intersection(&self, other: &Self) -> Self {
    Self {
      kinds: set_intersection(&self.kinds, &other.kinds),
      max_artifact_count: self.max_artifact_count.min(other.max_artifact_count),
      max_artifact_bytes: self.max_artifact_bytes.min(other.max_artifact_bytes),
      max_report_count: self.max_report_count.min(other.max_report_count),
      max_report_bytes: self.max_report_bytes.min(other.max_report_bytes),
    }
  }
}

/// Untrusted permission input normalized into a deny-by-default set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryPermissionDraft {
  /// Exact permitted plugin identities.
  pub plugins: Vec<ImmutableReference>,
  /// Exact permitted executable identities.
  pub executables: Vec<ImmutableReference>,
  /// Exact permitted tool identities.
  pub tools: Vec<ImmutableReference>,
  /// Permitted exact executable and bounded argument patterns.
  pub commands: Vec<CommandPermission>,
  /// Maximum child processes spawned by the primary process.
  pub max_descendants: u32,
  /// Portable filesystem roots and maximum mount access.
  pub mounts: Vec<MountPermission>,
  /// Exact permitted network hosts.
  pub network_hosts: Vec<NetworkHost>,
  /// Logical secret-delivery profile allowlist.
  pub secret_profiles: Vec<FactoryKey>,
  /// Logical workload-identity profile allowlist.
  pub workload_identity_profiles: Vec<FactoryKey>,
  /// Independent execution resource ceilings.
  pub resources: FactoryResourceLimits,
  /// Permitted output kinds and bounds.
  pub outputs: FactoryOutputPermissions,
}

/// Canonical deny-by-default Factory authority that can only be intersected.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactoryPermissionSet {
  plugins: BTreeSet<ImmutableReference>,
  executables: BTreeSet<ImmutableReference>,
  tools: BTreeSet<ImmutableReference>,
  commands: BTreeSet<CommandPermission>,
  max_descendants: u32,
  mounts: BTreeSet<MountPermission>,
  network_hosts: BTreeSet<NetworkHost>,
  secret_profiles: BTreeSet<FactoryKey>,
  workload_identity_profiles: BTreeSet<FactoryKey>,
  resources: FactoryResourceLimits,
  outputs: FactoryOutputPermissions,
}

impl FactoryPermissionSet {
  /// Returns a set granting no authority in any category.
  #[must_use]
  pub fn deny_all() -> Self {
    Self::default()
  }

  /// Validates bounded grants and produces their canonical ordering.
  pub fn try_new(draft: FactoryPermissionDraft) -> Result<Self, FactoryError> {
    if draft.max_descendants > MAX_FACTORY_PROCESS_COUNT {
      return Err(invalid(PermissionCategory::DescendantProcesses));
    }
    let executables = bounded_set(draft.executables, PermissionCategory::ExecutableIdentity)?;
    if draft
      .commands
      .iter()
      .any(|command| !executables.contains(command.executable()))
    {
      return Err(invalid(PermissionCategory::CommandArguments));
    }
    let mounts = bounded_set(draft.mounts, PermissionCategory::FilesystemPaths)?;
    if mounts.iter().enumerate().any(|(index, mount)| {
      mounts
        .iter()
        .skip(index + 1)
        .any(|other| mount.root.contains(&other.root) || other.root.contains(&mount.root))
    }) {
      return Err(invalid(PermissionCategory::FilesystemPaths));
    }
    Ok(Self {
      plugins: bounded_set(draft.plugins, PermissionCategory::PluginIdentity)?,
      executables,
      tools: bounded_set(draft.tools, PermissionCategory::ToolIdentity)?,
      commands: bounded_set(draft.commands, PermissionCategory::CommandArguments)?,
      max_descendants: draft.max_descendants,
      mounts,
      network_hosts: bounded_set(draft.network_hosts, PermissionCategory::NetworkHosts)?,
      secret_profiles: bounded_set(draft.secret_profiles, PermissionCategory::SecretProfiles)?,
      workload_identity_profiles: bounded_set(
        draft.workload_identity_profiles,
        PermissionCategory::WorkloadIdentityProfiles,
      )?,
      resources: draft.resources,
      outputs: draft.outputs,
    })
  }

  /// Computes the pure, commutative intersection of two permission sets.
  #[must_use]
  pub fn intersection(&self, other: &Self) -> Self {
    Self {
      plugins: set_intersection(&self.plugins, &other.plugins),
      executables: set_intersection(&self.executables, &other.executables),
      tools: set_intersection(&self.tools, &other.tools),
      commands: cross_intersection(&self.commands, &other.commands, CommandPermission::intersection),
      max_descendants: self.max_descendants.min(other.max_descendants),
      mounts: cross_intersection(&self.mounts, &other.mounts, MountPermission::intersection),
      network_hosts: set_intersection(&self.network_hosts, &other.network_hosts),
      secret_profiles: set_intersection(&self.secret_profiles, &other.secret_profiles),
      workload_identity_profiles: set_intersection(&self.workload_identity_profiles, &other.workload_identity_profiles),
      resources: self.resources.intersection(other.resources),
      outputs: self.outputs.intersection(&other.outputs),
    }
  }

  /// Reports whether this set grants no authority beyond an enclosing set.
  #[must_use]
  pub fn is_no_broader_than(&self, enclosing: &Self) -> bool {
    self.intersection(enclosing) == *self
  }

  /// Iterates over exact plugin identities in canonical order.
  pub fn plugins(&self) -> impl ExactSizeIterator<Item = &ImmutableReference> {
    self.plugins.iter()
  }

  /// Iterates over exact executable identities in canonical order.
  pub fn executables(&self) -> impl ExactSizeIterator<Item = &ImmutableReference> {
    self.executables.iter()
  }

  /// Iterates over exact tool identities in canonical order.
  pub fn tools(&self) -> impl ExactSizeIterator<Item = &ImmutableReference> {
    self.tools.iter()
  }

  /// Iterates over command patterns in canonical order.
  pub fn commands(&self) -> impl ExactSizeIterator<Item = &CommandPermission> {
    self.commands.iter()
  }

  /// Returns the descendant-process ceiling.
  #[must_use]
  pub const fn max_descendants(&self) -> u32 {
    self.max_descendants
  }

  /// Iterates over filesystem roots in canonical order.
  pub fn mounts(&self) -> impl ExactSizeIterator<Item = &MountPermission> {
    self.mounts.iter()
  }

  /// Iterates over exact network hosts in canonical order.
  pub fn network_hosts(&self) -> impl ExactSizeIterator<Item = &NetworkHost> {
    self.network_hosts.iter()
  }

  /// Iterates over logical secret profiles in canonical order.
  pub fn secret_profiles(&self) -> impl ExactSizeIterator<Item = &FactoryKey> {
    self.secret_profiles.iter()
  }

  /// Iterates over logical workload-identity profiles in canonical order.
  pub fn workload_identity_profiles(&self) -> impl ExactSizeIterator<Item = &FactoryKey> {
    self.workload_identity_profiles.iter()
  }

  /// Returns independent resource ceilings.
  #[must_use]
  pub const fn resources(&self) -> FactoryResourceLimits {
    self.resources
  }

  /// Returns bounded output authority.
  #[must_use]
  pub const fn outputs(&self) -> &FactoryOutputPermissions {
    &self.outputs
  }
}

/// Agent-local grants plus semantic enforcement capabilities of the selected backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPermissionCeiling {
  grants: FactoryPermissionSet,
  enforceable: BTreeSet<PermissionCategory>,
}

impl LocalPermissionCeiling {
  /// Constructs a local ceiling and rejects duplicate or oversized capability declarations.
  pub fn try_new(grants: FactoryPermissionSet, enforceable: Vec<PermissionCategory>) -> Result<Self, FactoryError> {
    if enforceable.len() > PermissionCategory::ALL.len() {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "permission capabilities",
      });
    }
    let mut capabilities = BTreeSet::new();
    for category in enforceable {
      if !capabilities.insert(category) {
        return Err(FactoryError::DuplicatePermission { category });
      }
    }
    Ok(Self {
      grants,
      enforceable: capabilities,
    })
  }

  /// Constructs a ceiling that advertises every known semantic capability.
  #[must_use]
  pub fn fully_enforced(grants: FactoryPermissionSet) -> Self {
    Self {
      grants,
      enforceable: PermissionCategory::ALL.into_iter().collect(),
    }
  }

  /// Returns Agent-local grants.
  #[must_use]
  pub const fn grants(&self) -> &FactoryPermissionSet {
    &self.grants
  }

  /// Reports whether one semantic permission category can be enforced.
  #[must_use]
  pub fn can_enforce(&self, category: PermissionCategory) -> bool {
    self.enforceable.contains(&category)
  }
}

/// Resolves effective authority across every trusted boundary and fails closed
/// when the local Agent/backend cannot enforce the complete vocabulary.
pub fn resolve_factory_permissions(
  project_policy: &FactoryPermissionSet,
  configuration: &FactoryPermissionSet,
  task_envelope: &FactoryPermissionSet,
  local: &LocalPermissionCeiling,
) -> Result<FactoryPermissionSet, FactoryError> {
  for category in PermissionCategory::ALL {
    if !local.can_enforce(category) {
      return Err(FactoryError::UnenforceablePermission { category });
    }
  }
  Ok(
    project_policy
      .intersection(configuration)
      .intersection(task_envelope)
      .intersection(local.grants()),
  )
}

const fn invalid(category: PermissionCategory) -> FactoryError {
  FactoryError::InvalidPermission { category }
}

fn bounded_set<T: Ord>(values: Vec<T>, category: PermissionCategory) -> Result<BTreeSet<T>, FactoryError> {
  if values.len() > MAX_PERMISSION_ENTRIES_PER_CATEGORY {
    return Err(FactoryError::CollectionLimitExceeded {
      collection: "permission grants",
    });
  }
  let count = values.len();
  let values = values.into_iter().collect::<BTreeSet<_>>();
  if values.len() != count {
    return Err(FactoryError::DuplicatePermission { category });
  }
  Ok(values)
}

fn set_intersection<T: Clone + Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> BTreeSet<T> {
  left.intersection(right).cloned().collect()
}

fn cross_intersection<T: Ord, F>(left: &BTreeSet<T>, right: &BTreeSet<T>, intersect: F) -> BTreeSet<T>
where
  F: Fn(&T, &T) -> Option<T>,
{
  left
    .iter()
    .flat_map(|left| right.iter().filter_map(|right| intersect(left, right)))
    .collect()
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
