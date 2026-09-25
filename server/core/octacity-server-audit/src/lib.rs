//! Structured, immutable, and secret-safe server audit vocabulary.
//!
//! This crate defines facts and bounded read queries. It deliberately exposes
//! no append, update, or delete port: authoritative mutations create their
//! audit facts inside the same store transaction as the state change.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod metadata;
mod model;
mod query;

pub use metadata::{AuditMetadata, MAX_AUDIT_METADATA_BYTES, MAX_AUDIT_METADATA_ENTRIES};
pub use model::{AuditActor, AuditActorKind, AuditFact, AuditOutcome};
pub use query::{
  AuditCursor, AuditFactPage, AuditFactQuery, AuditInputError, MAX_AUDIT_ACTOR_IDENTITY_BYTES,
  MAX_AUDIT_OPERATION_BYTES, MAX_AUDIT_PAGE_SIZE, MAX_AUDIT_REQUEST_IDENTITY_BYTES, MAX_AUDIT_TARGET_IDENTITY_BYTES,
  MAX_AUDIT_TARGET_KIND_BYTES,
};
