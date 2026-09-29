use super::*;

pub(super) fn valid_webhook_callback_origin(value: &str) -> bool {
  if value.ends_with('/') {
    return false;
  }
  let Ok(origin) = url::Url::parse(value) else {
    return false;
  };
  let loopback_http = origin.scheme() == "http" && origin.host().is_some_and(loopback_host);
  (origin.scheme() == "https" || loopback_http)
    && origin.host().is_some()
    && origin.path() == "/"
    && origin.query().is_none()
    && origin.fragment().is_none()
    && origin.username().is_empty()
    && origin.password().is_none()
}

fn loopback_host(host: url::Host<&str>) -> bool {
  match host {
    url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
    url::Host::Ipv4(address) => address.is_loopback(),
    url::Host::Ipv6(address) => address.is_loopback(),
  }
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
