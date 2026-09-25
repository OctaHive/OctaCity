use serde_json::Value;

use crate::AuditInputError;

/// Maximum encoded bytes retained in one audit metadata object.
pub const MAX_AUDIT_METADATA_BYTES: usize = 4 * 1024;
/// Maximum top-level entries retained in one audit metadata object.
pub const MAX_AUDIT_METADATA_ENTRIES: usize = 32;
const MAX_AUDIT_METADATA_DEPTH: usize = 4;
const MAX_AUDIT_METADATA_STRING_BYTES: usize = 512;

/// Bounded JSON object containing only explicitly selected non-sensitive facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditMetadata(Value);

impl AuditMetadata {
  /// Validates one structured metadata object at the authoritative store seam.
  pub fn try_new(value: Value) -> Result<Self, AuditInputError> {
    let Value::Object(entries) = &value else {
      return Err(AuditInputError::InvalidMetadata);
    };
    if entries.len() > MAX_AUDIT_METADATA_ENTRIES
      || serde_json::to_vec(&value)
        .map_err(|_| AuditInputError::InvalidMetadata)?
        .len()
        > MAX_AUDIT_METADATA_BYTES
      || !safe_value(&value, 0)
    {
      return Err(AuditInputError::InvalidMetadata);
    }
    Ok(Self(value))
  }

  /// Returns the validated JSON object.
  #[must_use]
  pub const fn as_value(&self) -> &Value {
    &self.0
  }

  /// Consumes the wrapper and returns the validated JSON object.
  #[must_use]
  pub fn into_value(self) -> Value {
    self.0
  }
}

fn safe_value(value: &Value, depth: usize) -> bool {
  if depth > MAX_AUDIT_METADATA_DEPTH {
    return false;
  }
  match value {
    Value::Null | Value::Bool(_) | Value::Number(_) => true,
    Value::String(value) => value.len() <= MAX_AUDIT_METADATA_STRING_BYTES && !value.contains('\0'),
    Value::Array(values) => values.iter().all(|value| safe_value(value, depth + 1)),
    Value::Object(entries) => entries.iter().all(|(key, value)| {
      !key.is_empty()
        && key.len() <= 64
        && !sensitive_key(key)
        && key
          .bytes()
          .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && safe_value(value, depth + 1)
    }),
  }
}

fn sensitive_key(key: &str) -> bool {
  matches!(
    key,
    "authorization"
      | "credential"
      | "credential_hash"
      | "credential_secret"
      | "password"
      | "private_key"
      | "raw_body"
      | "request_body"
      | "secret"
      | "signature"
      | "signing_key"
      | "token"
      | "webhook_secret"
  ) || key.contains("presigned")
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn rejects_request_bodies_and_sensitive_fields() {
    for value in [
      json!({"request_body": {"safe_looking": true}}),
      json!({"credential_hash": "not-for-audit"}),
      json!({"presigned_url": "https://objects.invalid/?signature=secret"}),
    ] {
      assert_eq!(AuditMetadata::try_new(value), Err(AuditInputError::InvalidMetadata));
    }
  }

  #[test]
  fn accepts_small_explicit_fact_sets() {
    assert!(
      AuditMetadata::try_new(json!({"attempt_id": "attempt-1", "ready_job_count": 2, "replayed": false})).is_ok()
    );
  }
}
