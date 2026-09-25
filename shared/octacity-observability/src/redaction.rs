use std::fmt;

/// Stable representation used instead of protected diagnostic material.
pub const REDACTED: &str = "[REDACTED]";
/// Maximum UTF-8 bytes retained in one sanitized diagnostic.
pub const MAX_DIAGNOSTIC_BYTES: usize = 1_024;

/// Required treatment for an observability field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedactionAction {
  /// The complete field must be omitted.
  Omit,
  /// The field value must be replaced by [`REDACTED`].
  Replace,
  /// A URL may be retained only after removing its query and fragment.
  StripUrlQuery,
  /// The field is not sensitive by name; ordinary value bounds still apply.
  RetainBounded,
}

/// Returns the required redaction treatment for a structured field name.
#[must_use]
pub fn field_redaction(field: &str) -> RedactionAction {
  let normalized = field.to_ascii_lowercase().replace(['-', '.'], "_");
  if matches!(
    normalized.as_str(),
    "authorization"
      | "agent_credential"
      | "agent_credentials"
      | "credential"
      | "credentials"
      | "credential_hash"
      | "credential_secret"
      | "password"
      | "passwords"
      | "private_key"
      | "private_keys"
      | "repository_credential"
      | "repository_credentials"
      | "secret"
      | "secrets"
      | "secret_value"
      | "secret_values"
      | "signature"
      | "signatures"
      | "signing_key"
      | "signing_keys"
      | "token"
      | "tokens"
      | "webhook_secret"
      | "webhook_secrets"
  ) {
    RedactionAction::Omit
  } else if normalized.contains("presigned") {
    RedactionAction::Replace
  } else if normalized == "url" || normalized.ends_with("_url") || normalized.ends_with("_uri") {
    RedactionAction::StripUrlQuery
  } else {
    RedactionAction::RetainBounded
  }
}

/// Sanitizes a bounded diagnostic by stripping URL query and fragment data.
///
/// Raw credentials and secret values must never be supplied as diagnostics in
/// the first place; named fields are governed by [`field_redaction`]. This
/// final boundary protects query credentials embedded by dependency errors.
#[must_use]
pub fn sanitize_diagnostic(input: &str) -> String {
  let mut output = String::with_capacity(input.len().min(MAX_DIAGNOSTIC_BYTES));
  let mut remaining = input;
  while !remaining.is_empty() && output.len() < MAX_DIAGNOSTIC_BYTES {
    let Some(offset) = next_url_offset(remaining) else {
      push_bounded(&mut output, remaining);
      break;
    };
    push_bounded(&mut output, &remaining[..offset]);
    remaining = &remaining[offset..];
    let end = remaining.find(char::is_whitespace).unwrap_or(remaining.len());
    let (url, rest) = remaining.split_at(end);
    let sensitive = url.find(['?', '#']);
    match sensitive {
      Some(index) => {
        push_bounded(&mut output, &url[..index]);
        push_bounded(&mut output, REDACTED);
      }
      None => push_bounded(&mut output, url),
    }
    remaining = rest;
  }
  output
}

fn next_url_offset(value: &str) -> Option<usize> {
  [value.find("https://"), value.find("http://")]
    .into_iter()
    .flatten()
    .min()
}

fn push_bounded(output: &mut String, value: &str) {
  let remaining = MAX_DIAGNOSTIC_BYTES.saturating_sub(output.len());
  let end = value
    .char_indices()
    .map(|(index, _)| index)
    .take_while(|index| *index <= remaining)
    .last()
    .unwrap_or(0);
  let candidate_end = if value.len() <= remaining { value.len() } else { end };
  output.extend(
    value[..candidate_end]
      .chars()
      .map(|character| if character.is_control() { ' ' } else { character }),
  );
}

/// Opaque wrapper whose standard diagnostic representations never expose the value.
///
/// It deliberately does not implement the sealed metric-label trait.
pub struct Sensitive<T>(T);

impl<T> Sensitive<T> {
  /// Wraps protected material at an observability boundary.
  #[must_use]
  pub const fn new(value: T) -> Self {
    Self(value)
  }

  /// Borrows the value for the owning security boundary.
  #[must_use]
  pub const fn expose(&self) -> &T {
    &self.0
  }
}

impl<T> fmt::Debug for Sensitive<T> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(REDACTED)
  }
}

impl<T> fmt::Display for Sensitive<T> {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(REDACTED)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn protected_fields_and_wrappers_never_format_their_values() {
    let token = Sensitive::new("bearer-secret");
    assert_eq!(format!("{token}"), REDACTED);
    assert_eq!(format!("{token:?}"), REDACTED);
    assert_eq!(field_redaction("Authorization"), RedactionAction::Omit);
    assert_eq!(field_redaction("webhook.secret"), RedactionAction::Omit);
    assert_eq!(field_redaction("repository.credentials"), RedactionAction::Omit);
    assert_eq!(field_redaction("signing_keys"), RedactionAction::Omit);
    assert_eq!(field_redaction("presigned_url"), RedactionAction::Replace);
  }

  #[test]
  fn signed_url_diagnostics_lose_query_credentials_and_are_bounded() {
    let diagnostic = sanitize_diagnostic(
      "GET https://objects.invalid/build/log?X-Amz-Credential=secret&X-Amz-Signature=hidden failed",
    );
    assert_eq!(diagnostic, "GET https://objects.invalid/build/log[REDACTED] failed");
    assert!(!diagnostic.contains("secret"));
    assert!(!diagnostic.contains("Signature"));
    assert!(sanitize_diagnostic(&"x".repeat(MAX_DIAGNOSTIC_BYTES * 2)).len() <= MAX_DIAGNOSTIC_BYTES);
  }
}
