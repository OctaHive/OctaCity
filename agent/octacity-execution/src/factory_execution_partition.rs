//! Divides one immutable Job allocation between a workload and its private reader.

use crate::{ExecutionDiskAccounting, ExecutionError, ExecutionTarget, NetworkAccess, OciIsolation, StartExecution};

/// Requests whose combined resource ceilings fit one original Factory Job.
///
/// Helper ceilings must come from trusted installed policy. This value grants
/// no tool authority and qualifies no backend. Its owner must use the narrowed
/// main request, serialize helper executions, and enforce one original absolute
/// deadline across both requests, including startup and cleanup.
#[derive(Clone, Debug)]
pub struct FactoryExecutionPartition {
  main: StartExecution,
  helper: StartExecution,
}

impl FactoryExecutionPartition {
  /// Reserves the helper's allocation before either execution can start.
  pub fn new(mut main: StartExecution, mut helper: StartExecution) -> Result<Self, ExecutionError> {
    main.validate()?;
    helper.validate()?;
    let main_layout = main.factory.as_ref().ok_or_else(invalid_partition)?;
    let helper_layout = helper.factory.as_ref().ok_or_else(invalid_partition)?;
    let main_roots = [&main_layout.scratch, &main_layout.output, &main_layout.protected_inputs];
    let helper_roots = [
      &helper_layout.scratch,
      &helper_layout.output,
      &helper_layout.protected_inputs,
    ];
    if !matches!(
      main.root,
      ExecutionTarget::Oci {
        isolation: OciIsolation::Hypervisor,
        ..
      }
    ) || main.root != helper.root
      || main.execution_id == helper.execution_id
      || main.workspace_root != helper.workspace_root
      || main_layout.source != helper_layout.source
      || main_layout.source_visible
      || !main_layout.source_read_only
      || !main_layout.source_writable_paths.is_empty()
      || !helper_layout.source_visible
      || !helper_layout.source_read_only
      || !helper_layout.source_writable_paths.is_empty()
      || helper_layout.tool_channel.is_some()
      || helper.workload_identity.is_some()
      || helper.cache.is_some()
      || !matches!(helper.network, NetworkAccess::Disabled)
      || helper.max_duration > main.max_duration
      || main_roots
        .iter()
        .any(|main_root| helper_roots.iter().any(|helper_root| main_root == helper_root))
    {
      return Err(invalid_partition());
    }
    let processes = main_layout
      .process_limit
      .checked_sub(helper_layout.process_limit)
      .filter(|remaining| *remaining > 0)
      .ok_or_else(invalid_partition)?;
    main.cpu_millis = main
      .cpu_millis
      .checked_sub(helper.cpu_millis)
      .filter(|remaining| *remaining > 0)
      .ok_or_else(invalid_partition)?;
    main.memory_bytes = main
      .memory_bytes
      .checked_sub(helper.memory_bytes)
      .filter(|remaining| *remaining > 0)
      .ok_or_else(invalid_partition)?;
    main.writable_disk_bytes = main
      .writable_disk_bytes
      .checked_sub(helper.writable_disk_bytes)
      .filter(|remaining| *remaining > 0)
      .ok_or_else(invalid_partition)?;
    let main_layout = main.factory.as_mut().ok_or_else(invalid_partition)?;
    main_layout.process_limit = processes;
    main_layout.disk_accounting = ExecutionDiskAccounting::OwnedWritableMounts;
    helper.factory.as_mut().ok_or_else(invalid_partition)?.disk_accounting =
      ExecutionDiskAccounting::OwnedWritableMounts;
    Ok(Self { main, helper })
  }

  /// Gives the owner the narrowed workload and exact reserved helper requests.
  /// The original workload request must not be started alongside the helper.
  pub fn into_parts(self) -> (StartExecution, StartExecution) {
    (self.main, self.helper)
  }
}

fn invalid_partition() -> ExecutionError {
  ExecutionError::Invalid("Factory reader does not fit the original Job resource and isolation allocation".to_owned())
}
