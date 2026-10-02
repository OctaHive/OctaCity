use serde::{Deserialize, Serialize};

use super::CursorPage;

/// REST summary of one current Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineSummaryResource {
  /// Stable Pipeline identity shared by all versions.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Project-local Pipeline name.
  pub name: String,
  /// Current positive immutable version.
  pub version: u64,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Bounded current-Pipeline page ordered by stable Pipeline identity.
pub type PipelineSummaryPage = CursorPage<PipelineSummaryResource>;

/// REST summary of one current Repository version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositorySummaryResource {
  /// Stable Repository identity shared by all versions.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Project-local Repository name.
  pub name: String,
  /// Current positive immutable version.
  pub version: u64,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Bounded current-Repository page ordered by stable Repository identity.
pub type RepositorySummaryPage = CursorPage<RepositorySummaryResource>;

/// REST summary of one current Build Configuration version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfigurationSummaryResource {
  /// Stable Build Configuration identity shared by all versions.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Project-local Build Configuration name.
  pub name: String,
  /// Current positive immutable version.
  pub version: u64,
  /// Whether this version accepts new Trigger occurrences.
  pub enabled: bool,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Bounded current-Build-Configuration page ordered by stable identity.
pub type BuildConfigurationSummaryPage = CursorPage<BuildConfigurationSummaryResource>;

/// Closed operator-facing Trigger definition kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerDefinitionKind {
  /// Trusted-network manual Trigger definition.
  Manual,
  /// Durable scheduled Trigger definition.
  Scheduled,
  /// Server-generated internal Trigger definition.
  Internal,
}

/// Credential-free REST summary of one current Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDefinitionSummaryResource {
  /// Stable Trigger identity shared by all versions.
  pub id: String,
  /// Owning Project identity derived from the selected Build Configuration.
  pub project_id: String,
  /// Selected Build Configuration identity.
  pub configuration_id: String,
  /// Exact selected Build Configuration version.
  pub configuration_version: u64,
  /// Current positive immutable Trigger version.
  pub version: u64,
  /// Closed Trigger origin safe for operator discovery.
  pub kind: TriggerDefinitionKind,
  /// Whether this definition may accept new occurrences.
  pub enabled: bool,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Bounded credential-free current-Trigger-definition page ordered by stable identity.
pub type TriggerDefinitionSummaryPage = CursorPage<TriggerDefinitionSummaryResource>;
