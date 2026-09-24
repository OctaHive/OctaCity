//! Cache namespace policy and fenced session lifecycle.
//!
//! Octa action keys and task-result semantics remain opaque to this module.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::{collections::BTreeSet, fmt, fmt::Write as _, str::FromStr};

mod data_plane;

pub use data_plane::{
  CacheBlobObject, CacheBlobStore, CacheBlobStoreError, CacheBlobWrite, CacheIntegrityError, verify_blob_bytes,
};
pub use octa_cache_protocol::{
  ActionResultV1, BlobDescriptor, BlobEncoding, Digest, DigestAlgorithm, FindMissingBlobsRequestV1,
  FindMissingBlobsResponseV1, MAX_ACTION_RESULT_WIRE_BYTES, MAX_REMOTE_CACHE_METADATA_BYTES,
  REMOTE_CACHE_BLOB_CONTENT_TYPE, REMOTE_CACHE_JSON_CONTENT_TYPE, REMOTE_CACHE_PROTOCOL_HEADER,
  REMOTE_CACHE_PROTOCOL_HEADER_VALUE_V1, REMOTE_CACHE_PROTOCOL_V1, WriteActionRequestV1,
};

use hmac::{Hmac, Mac as _};
use octacity_server_domain::{CacheSessionId, EntityKind, TransitionError};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

/// Maximum UTF-8 bytes in one server-managed logical cache namespace.
///
/// The bound intentionally narrows Octa's more general wire-string ceiling so
/// namespace values remain valid Project policy references.
pub const MAX_CACHE_NAMESPACE_BYTES: usize = 128;

const CACHE_CREDENTIAL_DOMAIN: &[u8] = b"octacity.cache-credential.v1\0";
const CACHE_CREDENTIAL_DIGEST_DOMAIN: &[u8] = b"octacity.cache-credential-digest.v1\0";

/// Logical namespace whose syntax is shared with Octa's published cache protocol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheNamespace(String);

impl CacheNamespace {
  /// Constructs a namespace accepted by the Octa cache protocol.
  pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
    let value = value.into();
    if value.len() > MAX_CACHE_NAMESPACE_BYTES {
      return Err("invalid cache namespace");
    }
    octa_cache_protocol::validate_namespace(&value).map_err(|_| "invalid cache namespace")?;
    Ok(Self(value))
  }

  /// Borrows the canonical namespace.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for CacheNamespace {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl FromStr for CacheNamespace {
  type Err = &'static str;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

impl Serialize for CacheNamespace {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for CacheNamespace {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Independently narrowed read and publication authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CachePermissions {
  /// Permit action and blob lookup.
  pub read: bool,
  /// Permit immutable publication.
  pub write: bool,
}

impl CachePermissions {
  /// Constructs a useful permission set.
  pub const fn new(read: bool, write: bool) -> Result<Self, &'static str> {
    if read || write {
      Ok(Self { read, write })
    } else {
      Err("cache permissions must allow reading, writing, or both")
    }
  }

  /// Reports whether this authority is no broader than `allowed`.
  #[must_use]
  pub const fn is_subset_of(self, allowed: Self) -> bool {
    (!self.read || allowed.read) && (!self.write || allowed.write)
  }

  /// Reports whether one operation is permitted.
  #[must_use]
  pub const fn allows(self, operation: CacheOperation) -> bool {
    match operation {
      CacheOperation::Read => self.read,
      CacheOperation::Write => self.write,
    }
  }
}

/// One cache data-plane permission requested for authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheOperation {
  /// Read an action result or blob.
  Read,
  /// Publish an immutable action result or blob.
  Write,
}

/// Effective Project cache policy decoded from an immutable Build snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCachePolicy {
  /// Logical namespaces selectable by Jobs in the Project.
  pub namespaces: BTreeSet<CacheNamespace>,
  /// Maximum read authority.
  pub read: bool,
  /// Maximum publication authority.
  pub write: bool,
  /// Maximum authoritative bytes attributable to the Project.
  pub max_bytes: u64,
}

/// Complete namespace policy bound into one session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheNamespacePolicy {
  /// Single authorized namespace.
  pub namespace: CacheNamespace,
  /// Narrowed permissions.
  pub permissions: CachePermissions,
  /// Project-wide byte quota applied within the authority boundary.
  pub quota_bytes: u64,
  /// Maximum retention duration in seconds.
  pub retention_seconds: u64,
}

impl CacheNamespacePolicy {
  /// Selects and narrows one namespace from immutable Project and Job policy.
  pub fn select(
    project: &ProjectCachePolicy,
    retention_seconds: u64,
    requested_namespace: CacheNamespace,
    requested_permissions: CachePermissions,
    signed: &octacity_protocol::CachePolicy,
  ) -> Result<Self, &'static str> {
    let signed_namespace = CacheNamespace::new(signed.namespace.clone())?;
    let signed_permissions = CachePermissions::new(signed.read, signed.write)?;
    let project_permissions = CachePermissions::new(project.read, project.write)?;
    if signed_namespace != requested_namespace
      || !project.namespaces.contains(&requested_namespace)
      || !requested_permissions.is_subset_of(signed_permissions)
      || !requested_permissions.is_subset_of(project_permissions)
      || project.max_bytes == 0
      || retention_seconds == 0
    {
      return Err("cache session exceeds immutable policy");
    }
    Ok(Self {
      namespace: requested_namespace,
      permissions: requested_permissions,
      quota_bytes: project.max_bytes,
      retention_seconds,
    })
  }
}

/// Server-owned key used to derive replay-safe short-lived bearer credentials.
#[derive(Clone)]
pub struct CacheCredentialKey(Zeroizing<[u8; 32]>);

impl CacheCredentialKey {
  /// Wraps independently generated 256-bit credential material.
  #[must_use]
  pub fn new(bytes: [u8; 32]) -> Self {
    Self(Zeroizing::new(bytes))
  }

  /// Derives one domain-separated credential for a stable session identity.
  #[must_use]
  pub fn derive(&self, session_id: CacheSessionId) -> CacheCredential {
    let mut mac = Hmac::<Sha256>::new_from_slice(self.0.as_slice())
      .expect("HMAC accepts independently generated keys of every size");
    mac.update(CACHE_CREDENTIAL_DOMAIN);
    mac.update(session_id.to_string().as_bytes());
    CacheCredential(Zeroizing::new(mac.finalize().into_bytes().into()))
  }
}

impl fmt::Debug for CacheCredentialKey {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("CacheCredentialKey([REDACTED])")
  }
}

/// Plain bearer credential retained only at trusted application and protocol seams.
#[derive(Clone)]
pub struct CacheCredential(Zeroizing<[u8; 32]>);

impl CacheCredential {
  /// Parses the canonical lowercase hexadecimal bearer representation.
  pub fn from_token(value: &str) -> Result<Self, &'static str> {
    if value.len() != 64 || !value.is_ascii() {
      return Err("invalid cache credential");
    }
    let mut bytes = [0_u8; 32];
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    for (target, pair) in bytes.iter_mut().zip(pairs) {
      let high = hex_nibble(pair[0]).ok_or("invalid cache credential")?;
      let low = hex_nibble(pair[1]).ok_or("invalid cache credential")?;
      *target = (high << 4) | low;
    }
    Ok(Self(Zeroizing::new(bytes)))
  }

  /// Returns the canonical wire representation.
  #[must_use]
  pub fn token(&self) -> String {
    let mut token = String::with_capacity(64);
    for byte in self.0.iter() {
      write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    token
  }

  /// Derives the irreversible value persisted by authoritative stores.
  #[must_use]
  pub fn digest(&self) -> CacheCredentialDigest {
    let mut digest = Sha256::new();
    digest.update(CACHE_CREDENTIAL_DIGEST_DOMAIN);
    digest.update(self.0.as_slice());
    CacheCredentialDigest(digest.finalize().into())
  }
}

impl fmt::Debug for CacheCredential {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("CacheCredential([REDACTED])")
  }
}

/// Fixed-size irreversible verifier stored for one session.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct CacheCredentialDigest([u8; 32]);

impl CacheCredentialDigest {
  /// Restores a digest from trusted durable state.
  #[must_use]
  pub const fn from_bytes(bytes: [u8; 32]) -> Self {
    Self(bytes)
  }

  /// Returns bytes suitable for persistence.
  #[must_use]
  pub const fn as_bytes(self) -> [u8; 32] {
    self.0
  }

  /// Compares digests without data-dependent early exit.
  #[must_use]
  pub fn matches(self, other: Self) -> bool {
    self
      .0
      .iter()
      .zip(other.0)
      .fold(0_u8, |difference, (left, right)| difference | (*left ^ right))
      == 0
  }
}

impl fmt::Debug for CacheCredentialDigest {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("CacheCredentialDigest([REDACTED])")
  }
}

const fn hex_nibble(value: u8) -> Option<u8> {
  match value {
    b'0'..=b'9' => Some(value - b'0'),
    b'a'..=b'f' => Some(value - b'a' + 10),
    _ => None,
  }
}

/// Authorization lifecycle of one short-lived remote-cache session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheSessionState {
  /// The current lease and credential may authorize cache operations.
  Active,
  /// Explicit revocation or lease fencing permanently removed authority.
  Revoked,
  /// The session deadline elapsed and permanently removed authority.
  Expired,
}

/// Fact applied to a [`CacheSessionState`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheSessionEvent {
  /// Revoke authority; repeated revocation is idempotent.
  Revoke,
  /// Record authoritative expiry; repeated expiry is idempotent.
  Expire,
}

impl CacheSessionState {
  /// Applies one fact without issuing, storing, or checking credentials.
  pub fn transition(self, event: CacheSessionEvent) -> Result<Self, TransitionError<Self, CacheSessionEvent>> {
    match (self, event) {
      (Self::Active, CacheSessionEvent::Revoke) => Ok(Self::Revoked),
      (Self::Active, CacheSessionEvent::Expire) => Ok(Self::Expired),
      (Self::Revoked, CacheSessionEvent::Revoke) => Ok(Self::Revoked),
      (Self::Expired, CacheSessionEvent::Expire) => Ok(Self::Expired),
      _ => Err(TransitionError::new(EntityKind::CacheSession, self, event)),
    }
  }

  /// Reports whether this session may authorize a cache request.
  #[must_use]
  pub const fn is_authorized(self) -> bool {
    matches!(self, Self::Active)
  }
}
