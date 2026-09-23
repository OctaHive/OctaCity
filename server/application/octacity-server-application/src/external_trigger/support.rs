use super::*;

pub(super) fn valid_webhook_callback_origin(value: &str) -> bool {
  let Some(authority) = value.strip_prefix("https://").or_else(|| value.strip_prefix("http://")) else {
    return false;
  };
  if authority.is_empty()
    || authority.contains(['/', '?', '#', '@'])
    || authority
      .chars()
      .any(|character| character.is_whitespace() || character.is_control())
  {
    return false;
  }
  if let Some(ipv6) = authority.strip_prefix('[') {
    let Some((address, suffix)) = ipv6.split_once(']') else {
      return false;
    };
    return address.parse::<std::net::Ipv6Addr>().is_ok() && valid_port_suffix(suffix);
  }
  if authority.matches(':').count() > 1 {
    return false;
  }
  let (host, port) = authority
    .split_once(':')
    .map_or((authority, None), |(host, port)| (host, Some(port)));
  valid_host(host) && port.is_none_or(valid_port)
}

fn valid_host(host: &str) -> bool {
  if host.parse::<std::net::Ipv4Addr>().is_ok() {
    return true;
  }
  !host.is_empty()
    && host.len() <= 253
    && host.split('.').all(|label| {
      !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn valid_port_suffix(suffix: &str) -> bool {
  suffix.is_empty() || suffix.strip_prefix(':').is_some_and(valid_port)
}

fn valid_port(port: &str) -> bool {
  !port.is_empty() && port.parse::<u16>().is_ok()
}

pub(super) fn validated_webhook_definition(
  input: WebhookDefinitionInput<'_>,
) -> Result<UnmanagedWebhookDefinition, ApplicationError> {
  let definition = UnmanagedWebhookDefinition {
    adapter_id: input.adapter_id.to_owned(),
    adapter_sha256: input.adapter_sha256.to_owned(),
    verification_material_handle: input.verification_material_handle.to_owned(),
    verification_headers: canonical_webhook_headers(input.verification_headers.to_vec())
      .map_err(|_| ApplicationError::invalid())?,
    repository_id: input.repository_id,
    event_kind: input.event_kind.clone(),
    parameters: input.parameters.clone(),
    priority: input.priority,
  };
  definition.validate().map_err(|_| ApplicationError::invalid())?;
  Ok(definition)
}

pub(super) fn ensure_callback(actual: &str, expected: &str) -> Result<(), ApplicationError> {
  if actual == expected {
    Ok(())
  } else {
    Err(ApplicationError::invalid())
  }
}

pub(crate) fn ensure_managed_result(
  registration: &ManagedWebhookRegistration,
  expected_callback: &str,
  operation: ManagedWebhookOperation,
) -> Result<(), ApplicationError> {
  ensure_callback(&registration.callback_url, expected_callback)?;
  StoredManagedWebhookRegistration::from(registration.clone())
    .validate()
    .map_err(|_| ApplicationError::invalid())?;
  if (operation == ManagedWebhookOperation::Create && registration.status == ManagedWebhookRegistrationStatus::Missing)
    || (operation == ManagedWebhookOperation::Delete
      && registration.status != ManagedWebhookRegistrationStatus::Missing)
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}

pub(super) fn managed_provider_error(error: WebhookVerificationError) -> ApplicationError {
  match error.classification() {
    WebhookVerificationFailure::Unsupported => ApplicationError::capability_unavailable(),
    WebhookVerificationFailure::Unavailable | WebhookVerificationFailure::Cancelled => ApplicationError::unavailable(),
    WebhookVerificationFailure::AuthenticationFailed
    | WebhookVerificationFailure::InvalidConfiguration
    | WebhookVerificationFailure::Permanent
    | WebhookVerificationFailure::InvalidResponse => ApplicationError::invalid(),
  }
}

pub(crate) const fn webhook_failure_code(failure: WebhookVerificationFailure) -> WebhookFailureCode {
  match failure {
    WebhookVerificationFailure::AuthenticationFailed => WebhookFailureCode::AuthenticationFailed,
    WebhookVerificationFailure::InvalidConfiguration => WebhookFailureCode::InvalidConfiguration,
    WebhookVerificationFailure::Unsupported => WebhookFailureCode::Unsupported,
    WebhookVerificationFailure::Permanent => WebhookFailureCode::Permanent,
    WebhookVerificationFailure::Cancelled => WebhookFailureCode::Cancelled,
    WebhookVerificationFailure::Unavailable => WebhookFailureCode::Unavailable,
    WebhookVerificationFailure::InvalidResponse => WebhookFailureCode::InvalidResponse,
  }
}

pub(crate) fn durable_webhook_diagnostic(error: &WebhookVerificationError) -> String {
  crate::diagnostic::bounded_diagnostic(
    &error.to_string(),
    octacity_server_store::MAX_WEBHOOK_DIAGNOSTIC_BYTES,
    "webhook operation failed",
  )
}

impl From<StoredManagedWebhookRegistration> for ManagedWebhookRegistration {
  fn from(value: StoredManagedWebhookRegistration) -> Self {
    Self {
      registration_id: value.registration_id,
      status: value.status.into(),
      callback_url: value.callback_url,
    }
  }
}

impl From<ManagedWebhookRegistration> for StoredManagedWebhookRegistration {
  fn from(value: ManagedWebhookRegistration) -> Self {
    Self {
      registration_id: value.registration_id,
      status: value.status.into(),
      callback_url: value.callback_url,
    }
  }
}

impl From<StoredManagedWebhookRegistrationStatus> for ManagedWebhookRegistrationStatus {
  fn from(value: StoredManagedWebhookRegistrationStatus) -> Self {
    match value {
      StoredManagedWebhookRegistrationStatus::Active => Self::Active,
      StoredManagedWebhookRegistrationStatus::Disabled => Self::Disabled,
      StoredManagedWebhookRegistrationStatus::Missing => Self::Missing,
    }
  }
}

impl From<ManagedWebhookRegistrationStatus> for StoredManagedWebhookRegistrationStatus {
  fn from(value: ManagedWebhookRegistrationStatus) -> Self {
    match value {
      ManagedWebhookRegistrationStatus::Active => Self::Active,
      ManagedWebhookRegistrationStatus::Disabled => Self::Disabled,
      ManagedWebhookRegistrationStatus::Missing => Self::Missing,
    }
  }
}

impl From<ApplicationFailure> for WebhookDeliveryFailure {
  fn from(value: ApplicationFailure) -> Self {
    match value {
      ApplicationFailure::NotFound => Self::NotFound,
      ApplicationFailure::Unavailable => Self::Unavailable,
      _ => Self::Invalid,
    }
  }
}
