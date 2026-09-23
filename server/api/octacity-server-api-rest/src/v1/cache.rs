use serde::{Deserialize, Serialize};

/// Secret-free diagnostic facts for one fenced cache session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheSessionResource {
  /// Opaque session identity.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Build whose immutable policy authorized the session.
  pub build_id: String,
  /// Leased Job identity.
  pub job_id: String,
  /// Agent that owns the bound registration.
  pub agent_id: String,
  /// Positive registration epoch bound to the Lease.
  pub registration_epoch: u64,
  /// Lease identity without its private fence.
  pub lease_id: String,
  /// Single logical cache namespace.
  pub namespace: String,
  /// Whether lookup was granted.
  pub read: bool,
  /// Whether publication was granted.
  pub write: bool,
  /// Project quota applied to this namespace authority.
  pub quota_bytes: u64,
  /// Authoritative session creation time.
  pub created_at_unix_ms: i64,
  /// Exclusive credential expiry.
  pub expires_at_unix_ms: i64,
  /// Latest retention deadline for new entries.
  pub retention_until_unix_ms: i64,
  /// Effective diagnostic state.
  pub state: CacheSessionState,
  /// Explicit revocation time, when present.
  pub revoked_at_unix_ms: Option<i64>,
}

/// Effective cache-session state projected at management-query time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheSessionState {
  /// Session, registration, Lease, and expiry are current.
  Active,
  /// Agent explicitly revoked the session.
  Revoked,
  /// Session reached its server-controlled expiry.
  Expired,
  /// Bound Lease or registration is no longer current.
  Fenced,
}

/// Bounded cache-session diagnostics for one Build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheSessionPage {
  /// Secret-free session diagnostics.
  pub items: Vec<CacheSessionResource>,
}
