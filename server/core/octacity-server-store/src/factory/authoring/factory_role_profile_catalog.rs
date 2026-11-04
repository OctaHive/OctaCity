//! Bounded safe current-role/profile discovery; private authoring content stays in exact reads.
use crate::{MAX_CURRENT_DEFINITION_PAGE_SIZE, StoreError, StoreInputError, StoreOperation};
use octacity_server_factory::*;

/// Visible current definitions in stable identity order.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleCatalogPage<I, T> {
  /// Count before continuation, restricted to the authorized owner.
  pub total: u64,
  /// Bounded safe summaries.
  pub items: Vec<T>,
  /// Exclusive identity when more results exist.
  pub next_cursor: Option<I>,
}
/// Safe current AI-agent metadata without instructions or execution settings.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleSummary {
  /// Exact current owned version.
  pub reference: FactoryRoleRef,
  /// Editable display name.
  pub name: FactoryText,
  /// Optional presentation icon.
  pub icon: Option<FactoryKey>,
  /// Optional agent-wide concurrency declaration.
  pub concurrency_limit: Option<std::num::NonZeroU32>,
  /// Release provenance only, without live inheritance.
  pub template: Option<ImmutableReference>,
}
impl From<&FactoryRole> for FactoryRoleSummary {
  fn from(role: &FactoryRole) -> Self {
    Self {
      reference: role.reference().clone(),
      name: role.definition().name.clone(),
      icon: role.definition().icon.clone(),
      concurrency_limit: role.definition().concurrency_limit,
      template: role.definition().template.clone(),
    }
  }
}
/// Safe current execution profile metadata; environment, argv and instructions are absent.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryRoleProfileSummary {
  /// Exact current owned version.
  pub reference: FactoryRoleProfileRef,
  /// Explicitly pinned role, independent of later role edits.
  pub role: FactoryRoleRef,
  /// Editable profile display name.
  pub name: FactoryText,
  /// Exact public runtime/plugin/model identity.
  pub toolchain: FactoryTaskToolchain,
  /// Optional release provenance.
  pub template: Option<ImmutableReference>,
}
impl From<&FactoryRoleProfile> for FactoryRoleProfileSummary {
  fn from(profile: &FactoryRoleProfile) -> Self {
    Self {
      reference: profile.reference().clone(),
      role: profile.role().clone(),
      name: profile.definition().name.clone(),
      toolchain: profile.definition().toolchain.clone(),
      template: profile.definition().template.clone(),
    }
  }
}
/// Applies the existing current-definition page bound at either adapter boundary.
pub fn check_factory_role_catalog_limit(limit: u16, operation: StoreOperation) -> Result<usize, StoreError> {
  if limit == 0 || limit > MAX_CURRENT_DEFINITION_PAGE_SIZE {
    return Err(StoreError::InvalidInput {
      operation,
      source: StoreInputError::InvalidCurrentDefinitionPageSize,
    });
  }
  Ok(usize::from(limit))
}
