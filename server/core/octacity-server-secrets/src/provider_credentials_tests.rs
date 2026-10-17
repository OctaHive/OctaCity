use crate::{MAX_PROVIDER_CREDENTIAL_BYTES, ProviderCredentialError, ProviderCredentialValue};

#[test]
fn provider_credentials_are_bounded_transient_and_redacted_even_on_failure() {
  let value = ProviderCredentialValue::new(b"private-fixture-provider-token".to_vec()).unwrap();
  assert_eq!(value.expose_for_installation(), b"private-fixture-provider-token");
  assert_eq!(format!("{value:?}"), "ProviderCredentialValue([REDACTED])");
  assert!(ProviderCredentialValue::new(vec![1; MAX_PROVIDER_CREDENTIAL_BYTES]).is_ok());
  for bytes in [vec![], vec![1; MAX_PROVIDER_CREDENTIAL_BYTES + 1]] {
    let error = ProviderCredentialValue::new(bytes).unwrap_err();
    assert_eq!(error, ProviderCredentialError::InvalidValue);
    assert_eq!(error.to_string(), "invalid provider credential value");
  }
}
