use super::*;

impl ManagementInputFactory {
  /// Creates a replay-safe single-use Agent enrollment command.
  pub fn issue_agent_enrollment(
    &self,
    pool_id: &str,
    pool_version: u64,
    expected_operating_system: Option<String>,
    expected_architecture: Option<String>,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<IssueAgentEnrollmentCommand, ManagementInputError> {
    let key = parse::<octacity_server_store::IdempotencyKey>(idempotency_key, "idempotency key")?;
    let credential_id =
      EnrollmentCredentialId::from_uuid(Uuid::new_v5(&AGENT_ENROLLMENT_NAMESPACE, key.as_str().as_bytes()))
        .map_err(|_| ManagementInputError::Invalid("enrollment credential id"))?;
    let expected_platform = match (expected_operating_system, expected_architecture) {
      (None, None) => ExpectedAgentPlatform::Any,
      (Some(operating_system), Some(architecture)) => ExpectedAgentPlatform::Exact(
        AgentPlatform::new(operating_system, architecture)
          .map_err(|_| ManagementInputError::Invalid("expected agent platform"))?,
      ),
      _ => return Err(ManagementInputError::Invalid("expected agent platform")),
    };
    let issued_at = timestamp(now_unix_ms)?;
    let lifetime = i64::try_from(self.agent_enrollment_lifetime.as_millis())
      .map_err(|_| ManagementInputError::Invalid("agent enrollment lifetime"))?;
    let expires_at = now_unix_ms
      .checked_add(lifetime)
      .ok_or(ManagementInputError::Invalid("agent enrollment expiry"))?;
    let credential_identity = credential_id.to_string();
    let secret = self.agent_enrollment_secret_key.derive(&credential_identity);
    Ok(IssueAgentEnrollmentCommand {
      credential: AgentCredentialToken::enrollment(credential_identity, secret)
        .map_err(|_| ManagementInputError::Invalid("agent enrollment credential"))?,
      pool_id: parse(pool_id, "agent pool id")?,
      pool_version: version(pool_version, "agent pool version")?,
      expected_platform,
      issued_at,
      expires_at: timestamp(expires_at)?,
    })
  }
}
