//! Shared installed read scope, independent of Factory preparation, admission and task storage.
use super::*;

/// Trusted installed Source read settings shared by intake, preview and optional task consumers.
#[derive(Clone)]
pub struct WorkSourceReadProfile {
  /// Explicit managed connection guard; absent only for existing operator-installed authority.
  pub managed_access: Option<ManagedIntegrationAccess>,
  /// Permitted OctaCity Project.
  pub project_id: ProjectId,
  /// Exact installed source settings; mutable ticket data cannot choose them.
  pub binding: WorkSourceBinding,
  /// Exact content-verified plugin executable.
  pub plugin: ImmutableReference,
  /// Frozen provider transport/configuration profile.
  pub provider_configuration: FactoryDigest,
  /// Independently scoped read handle and native ticket container.
  pub resource: ResourceAccess,
  /// Exact provider-owned selector.
  pub selection: ConfigurationReference,
  /// Actual permitted provider hosts; never derived from a ticket.
  pub network_hosts: Vec<String>,
  /// Independently accepted installation-owner attribution.
  pub context: ManagementRequestContext,
  /// Finite normalized discovery limits and reconciliation policy.
  pub polling: SourcePollingPolicy,
}
impl WorkSourceReadProfile {
  pub(super) fn for_connection(
    mut self,
    connection: Option<&WorkSourceConnectionRef>,
    connections: Option<&ManagedIntegrationConnections>,
  ) -> Result<Self, ApplicationError> {
    if let Some(connection) = connection {
      self.managed_access = Some(
        connections
          .ok_or_else(ApplicationError::capability_unavailable)?
          .for_work_binding(connection, &self.binding, IntegrationPurpose::TaskRead)?,
      );
    }
    Ok(self)
  }
  /// Adopts current managed read-grant checks without changing the installed identity.
  pub fn with_managed_access(mut self, access: ManagedIntegrationAccess) -> Self {
    self.managed_access = Some(access);
    self
  }
  pub(super) async fn authorize_access(&self) -> Result<(), ApplicationError> {
    if let Some(access) = &self.managed_access {
      access
        .authorize_work_binding(
          self.project_id,
          &self.binding,
          &self.plugin,
          &self.resource.resource_id,
          IntegrationPurpose::TaskRead,
        )
        .await?;
    }
    Ok(())
  }
}
