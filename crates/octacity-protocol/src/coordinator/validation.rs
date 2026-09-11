//! Semantic validation for coordinator wire DTOs.

use std::collections::BTreeSet;

use super::*;

impl AgentInventory {
  /// Validates bounded identities and scheduler-visible capability invariants.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("agent_id", &self.agent_id)?;
    identifier("agent_version", &self.agent_version)?;
    non_empty_versions("coordinator_protocols", &self.coordinator_protocols)?;
    if !self.coordinator_protocols.contains(&COORDINATOR_PROTOCOL_VERSION) {
      return invalid("agent does not advertise coordinator protocol v1");
    }
    bounded_len("labels", self.labels.len())?;
    for (name, value) in &self.labels {
      identifier("label name", name)?;
      identifier("label value", value)?;
    }
    self.host_capacity.validate()?;
    if self.runtimes.is_empty() {
      return invalid("at least one runtime capability is required");
    }
    bounded_len("runtime capabilities", self.runtimes.len())?;
    let mut capabilities = BTreeSet::new();
    for capability in &self.runtimes {
      capability.validate()?;
      if !capabilities.insert(capability) {
        return invalid("runtime capabilities must not contain duplicates");
      }
    }
    self.octa.validate()?;
    bounded_len("source plugins", self.source_plugins.len())?;
    let mut source_names = BTreeSet::new();
    for plugin in &self.source_plugins {
      plugin.validate()?;
      if !source_names.insert(&plugin.name) {
        return invalid("source plugin names must not contain duplicates");
      }
    }
    Ok(())
  }
}

impl HostCapacity {
  /// Rejects unusable zero-valued scheduling capacity.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    if self.logical_cpu_count == 0
      || self.total_memory_bytes == 0
      || self.work_disk_total_bytes == 0
      || self.state_disk_total_bytes == 0
    {
      return invalid("host capacity values must be greater than zero");
    }
    Ok(())
  }
}

impl RuntimeCapability {
  /// Validates the relationship between execution mode and isolation tier.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("backend", &self.backend)?;
    match (self.mode, self.isolation) {
      (RuntimeMode::Native, None) | (RuntimeMode::Oci, Some(_)) => Ok(()),
      (RuntimeMode::Native, Some(_)) => invalid("Native capability must not declare OCI isolation"),
      (RuntimeMode::Oci, None) => invalid("OCI capability must declare an isolation tier"),
    }
  }
}

impl OctaInventory {
  /// Validates release identities, protocol sets, and locked task plugins.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("Octa version", &self.version)?;
    sha256("runner_sha256", &self.runner_sha256)?;
    if let Some(commit) = &self.build_commit {
      identifier("build_commit", commit)?;
    }
    non_empty_versions("runner_protocols", &self.runner_protocols)?;
    non_empty_versions("event_schemas", &self.event_schemas)?;
    non_empty_versions("plugin_protocols", &self.plugin_protocols)?;
    if self.octafile_versions.is_empty() {
      return invalid("octafile_versions must not be empty");
    }
    string_list("features", &self.features, true)?;
    bounded_len("task plugins", self.plugins.len())?;
    let mut names = BTreeSet::new();
    for plugin in &self.plugins {
      plugin.validate()?;
      if !names.insert(&plugin.name) {
        return invalid("task plugin names must not contain duplicates");
      }
    }
    Ok(())
  }
}

impl TaskPluginInventory {
  /// Validates one locked task-plugin description.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("task plugin name", &self.name)?;
    identifier("task plugin version", &self.version)?;
    if self.protocol == 0 {
      return invalid("task plugin protocol must be greater than zero");
    }
    string_list("task plugin platforms", &self.platforms, false)?;
    sha256("task plugin sha256", &self.sha256)?;
    string_list("task plugin capabilities", &self.capabilities, true)
  }
}

impl SourcePluginInventory {
  /// Validates one source-plugin description without exposing operator settings.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("source plugin name", &self.name)?;
    identifier("source plugin version", &self.version)?;
    if self.protocol_min == 0 || self.protocol_min > self.protocol_max {
      return invalid("source plugin protocol range is invalid");
    }
    string_list("source plugin platforms", &self.platforms, false)?;
    sha256("source plugin sha256", &self.sha256)
  }
}

impl RegisterAgentRequest {
  /// Validates request metadata and the complete inventory.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    request(self.protocol_version, &self.request_id)?;
    self.inventory.validate()
  }
}

impl RegisterAgentResponse {
  /// Validates response metadata and registration policy.
  pub fn validate(&self, expected_request_id: &str) -> Result<(), CoordinatorProtocolError> {
    response(self.protocol_version, &self.request_id, expected_request_id)?;
    identifier("registration_id", &self.registration_id)?;
    if self.max_retry_delay_ms == 0 {
      return invalid("max_retry_delay_ms must be greater than zero");
    }
    Ok(())
  }
}

impl AcquireLeaseRequest {
  /// Validates long-poll metadata and its non-zero wait bound.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    request(self.protocol_version, &self.request_id)?;
    identifier("registration_id", &self.registration_id)?;
    if self.wait_seconds == 0 {
      return invalid("wait_seconds must be greater than zero");
    }
    Ok(())
  }
}

impl AcquireLeaseResponse {
  /// Validates response correlation and any returned lease.
  pub fn validate(
    &self,
    expected_request_id: &str,
    now: u64,
    lease_safety_margin_seconds: u64,
  ) -> Result<(), CoordinatorProtocolError> {
    let (protocol_version, request_id) = match self {
      Self::Lease {
        protocol_version,
        request_id,
        lease,
      } => {
        lease.validate(now, lease_safety_margin_seconds)?;
        (*protocol_version, request_id)
      }
      Self::NoWork {
        protocol_version,
        request_id,
        retry_after_ms,
      } => {
        if *retry_after_ms == 0 {
          return invalid("retry_after_ms must be greater than zero");
        }
        (*protocol_version, request_id)
      }
      Self::Drain {
        protocol_version,
        request_id,
      } => (*protocol_version, request_id),
    };
    response(protocol_version, request_id, expected_request_id)
  }
}

impl LeaseAssignment {
  /// Validates fencing identity and sufficient unexpired lease lifetime.
  pub fn validate(&self, now: u64, safety_margin_seconds: u64) -> Result<(), CoordinatorProtocolError> {
    LeaseFence::from(self).validate()?;
    if self.issued_at > now || self.issued_at >= self.expires_at {
      return invalid("lease validity interval is invalid");
    }
    let usable_until = now
      .checked_add(safety_margin_seconds)
      .ok_or_else(|| CoordinatorProtocolError::new("lease safety margin overflowed"))?;
    if usable_until >= self.expires_at {
      return invalid("lease expires within the configured safety margin");
    }
    Ok(())
  }
}

impl LeaseFence {
  /// Validates all values that fence a job attempt.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("lease_id", &self.lease_id)?;
    identifier("job_id", &self.job_id)?;
    identifier("fencing_token", &self.fencing_token)?;
    if self.attempt == 0 {
      return invalid("lease attempt must be greater than zero");
    }
    Ok(())
  }
}

impl HostSnapshot {
  /// Validates advisory values against registered static capacity.
  pub fn validate(&self, capacity: &HostCapacity) -> Result<(), CoordinatorProtocolError> {
    capacity.validate()?;
    let total_cpu_millis = u64::from(capacity.logical_cpu_count) * 1000;
    if self.available_cpu_millis > total_cpu_millis
      || self.available_memory_bytes > capacity.total_memory_bytes
      || self.work_disk_free_bytes > capacity.work_disk_total_bytes
      || self.state_disk_free_bytes > capacity.state_disk_total_bytes
    {
      return invalid("host snapshot exceeds registered capacity");
    }
    if let Some(job) = &self.active_job {
      job.validate()?;
    }
    bounded_len("backend health entries", self.backends.len())?;
    let mut names = BTreeSet::new();
    for backend in &self.backends {
      backend.validate()?;
      if !names.insert(&backend.backend) {
        return invalid("backend health entries must not contain duplicates");
      }
    }
    Ok(())
  }
}

impl ActiveJob {
  /// Validates the job and lease identities in a snapshot.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("active job_id", &self.job_id)?;
    identifier("active lease_id", &self.lease_id)?;
    if self.attempt == 0 {
      return invalid("active job attempt must be greater than zero");
    }
    Ok(())
  }
}

impl BackendHealth {
  /// Validates backend identity and bounds its optional diagnostic.
  pub fn validate(&self) -> Result<(), CoordinatorProtocolError> {
    identifier("backend health name", &self.backend)?;
    if let Some(message) = &self.message
      && (message.is_empty() || message.len() > MAX_HEALTH_MESSAGE_BYTES || message.chars().any(char::is_control))
    {
      return invalid("backend health message is invalid or too large");
    }
    Ok(())
  }
}

impl HeartbeatRequest {
  /// Validates request metadata, lease fencing, and the advisory snapshot.
  pub fn validate(&self, capacity: &HostCapacity) -> Result<(), CoordinatorProtocolError> {
    request(self.protocol_version, &self.request_id)?;
    identifier("registration_id", &self.registration_id)?;
    self.lease.validate()?;
    self.snapshot.validate(capacity)
  }
}

impl HeartbeatResponse {
  /// Validates response correlation and renewed lease lifetime.
  pub fn validate(
    &self,
    expected_request_id: &str,
    now: u64,
    lease_safety_margin_seconds: u64,
  ) -> Result<(), CoordinatorProtocolError> {
    response(self.protocol_version, &self.request_id, expected_request_id)?;
    if let HeartbeatDirective::Continue { expires_at } | HeartbeatDirective::Drain { expires_at } = self.directive {
      let usable_until = now
        .checked_add(lease_safety_margin_seconds)
        .ok_or_else(|| CoordinatorProtocolError::new("lease safety margin overflowed"))?;
      if expires_at <= usable_until {
        return invalid("renewed lease expires within the configured safety margin");
      }
    }
    Ok(())
  }
}

impl CoordinatorErrorResponse {
  /// Validates response correlation and bounded retry metadata.
  pub fn validate(&self, expected_request_id: &str) -> Result<(), CoordinatorProtocolError> {
    response(self.protocol_version, &self.request_id, expected_request_id)?;
    identifier("coordinator error code", &self.code)?;
    if self.message.is_empty()
      || self.message.len() > MAX_HEALTH_MESSAGE_BYTES
      || self.message.chars().any(char::is_control)
    {
      return invalid("coordinator error message is invalid or too large");
    }
    if self.retry_after_ms == Some(0) {
      return invalid("retry_after_ms must be greater than zero when present");
    }
    Ok(())
  }
}

fn request(protocol_version: u16, request_id: &str) -> Result<(), CoordinatorProtocolError> {
  if protocol_version != COORDINATOR_PROTOCOL_VERSION {
    return invalid("unsupported coordinator protocol version");
  }
  identifier("request_id", request_id)
}

fn response(
  protocol_version: u16,
  request_id: &str,
  expected_request_id: &str,
) -> Result<(), CoordinatorProtocolError> {
  request(protocol_version, request_id)?;
  if request_id != expected_request_id {
    return invalid("response request_id does not match the request");
  }
  Ok(())
}

fn identifier(name: &str, value: &str) -> Result<(), CoordinatorProtocolError> {
  if value.is_empty() || value.len() > MAX_COORDINATOR_IDENTIFIER_BYTES || value.chars().any(char::is_control) {
    return invalid(format!(
      "{name} must contain 1 to {MAX_COORDINATOR_IDENTIFIER_BYTES} bytes without control characters"
    ));
  }
  Ok(())
}

fn bounded_len(name: &str, len: usize) -> Result<(), CoordinatorProtocolError> {
  if len > MAX_INVENTORY_ENTRIES {
    invalid(format!("{name} exceeds the {MAX_INVENTORY_ENTRIES}-entry limit"))
  } else {
    Ok(())
  }
}

fn non_empty_versions<T>(name: &str, values: &[T]) -> Result<(), CoordinatorProtocolError>
where
  T: Copy + Ord + From<u8>,
{
  if values.is_empty() || values.iter().any(|value| *value == T::from(0)) {
    return invalid(format!("{name} must contain non-zero versions"));
  }
  let unique: BTreeSet<_> = values.iter().copied().collect();
  if unique.len() != values.len() {
    return invalid(format!("{name} must not contain duplicates"));
  }
  Ok(())
}

fn string_list(name: &str, values: &[String], allow_empty: bool) -> Result<(), CoordinatorProtocolError> {
  bounded_len(name, values.len())?;
  if !allow_empty && values.is_empty() {
    return invalid(format!("{name} must not be empty"));
  }
  let mut unique = BTreeSet::new();
  for value in values {
    identifier(name, value)?;
    if !unique.insert(value) {
      return invalid(format!("{name} must not contain duplicates"));
    }
  }
  Ok(())
}

fn sha256(name: &str, value: &str) -> Result<(), CoordinatorProtocolError> {
  if value.len() != 64
    || !value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
  {
    return invalid(format!("{name} must be a lowercase SHA-256 digest"));
  }
  Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, CoordinatorProtocolError> {
  Err(CoordinatorProtocolError::new(message))
}
