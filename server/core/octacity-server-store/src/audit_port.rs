use async_trait::async_trait;

use crate::{AuditFactPage, AuditFactQuery, StoreError};

/// Read-only authoritative access to immutable structured audit facts.
///
/// Audit writes intentionally have no independently callable port operation.
/// Each authoritative mutation constructs and appends its fact inside the same
/// transaction as the state change.
#[async_trait]
pub trait AuditFactStore: Send + Sync {
  /// Lists one bounded deterministic page without changing audit state.
  async fn list_audit_facts(&self, query: AuditFactQuery) -> Result<AuditFactPage, StoreError>;
}

#[cfg(test)]
mod tests {
  #[test]
  fn audit_port_exposes_no_mutation_operation() {
    let source = include_str!("audit_port.rs");
    let public_surface = source.split("#[cfg(test)]").next().unwrap();
    for forbidden in ["append_audit", "create_audit", "delete_audit", "update_audit"] {
      assert!(!public_surface.contains(forbidden), "audit port exposed {forbidden}");
    }
  }
}
