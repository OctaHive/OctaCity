use std::time::Duration;

use thiserror::Error;
use zeroize::Zeroizing;

pub(crate) const MAX_OPERATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Explicit S3-compatible endpoint and server-owned credentials.
pub struct S3ArtifactStoreConfig {
  /// S3 API endpoint, including scheme and optional port.
  pub endpoint: String,
  /// Signing region accepted by the service.
  pub region: String,
  /// Existing bucket owned by OctaCity.
  pub bucket: String,
  /// Optional safe object-key prefix shared by all OctaCity objects.
  pub prefix: String,
  /// Server-side S3 access key.
  pub access_key: Zeroizing<String>,
  /// Server-side S3 secret key.
  pub secret_key: Zeroizing<String>,
  /// Enables path-style addressing required by MinIO and some compatible stores.
  pub force_path_style: bool,
  /// Deadline for each complete storage operation, including body verification.
  pub operation_timeout: Duration,
  /// Maximum age of a successful full capability qualification.
  ///
  /// Checks within this interval use the process-owned marker object. Once the
  /// interval expires, readiness repeats PUT, GET, COPY, GET, and DELETE.
  pub capability_recheck_interval: Duration,
}

/// Invalid S3 adapter configuration rejected before a client is created.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum S3ArtifactStoreConfigError {
  /// The endpoint is not a safe origin accepted by this adapter.
  #[error("S3 endpoint must be an HTTPS origin, or a loopback HTTP origin for tests")]
  InvalidEndpoint,
  /// The signing region is empty, oversized, or contains unsupported characters.
  #[error("S3 region is invalid")]
  InvalidRegion,
  /// The bucket name is outside the adapter's supported S3-compatible subset.
  #[error("S3 bucket is invalid")]
  InvalidBucket,
  /// The configured physical object prefix contains an unsafe segment.
  #[error("S3 object prefix is invalid")]
  InvalidPrefix,
  /// An adapter credential is empty, oversized, or contains control characters.
  #[error("S3 credentials are invalid")]
  InvalidCredentials,
  /// Storage operations were configured outside the adapter's safe bound.
  #[error("S3 operation timeout must be between 1 ms and 5 minutes")]
  InvalidOperationTimeout,
  /// Full capability requalification cannot be disabled.
  #[error("S3 capability recheck interval must be greater than zero")]
  InvalidCapabilityRecheckInterval,
}

/// Validates non-secret S3 settings before credential files are read.
///
/// The composition root uses the same rules as `S3ArtifactStore::new`, so
/// `octacity-server validate` cannot accept a location the adapter will later
/// reject during startup.
pub fn validate_s3_settings(
  endpoint: &str,
  region: &str,
  bucket: &str,
  prefix: &str,
  operation_timeout: Duration,
  capability_recheck_interval: Duration,
) -> Result<(), S3ArtifactStoreConfigError> {
  let endpoint = endpoint
    .parse::<http::Uri>()
    .map_err(|_| S3ArtifactStoreConfigError::InvalidEndpoint)?;
  let secure = endpoint.scheme_str() == Some("https");
  let loopback =
    endpoint.scheme_str() == Some("http") && matches!(endpoint.host(), Some("127.0.0.1" | "::1" | "localhost"));
  if (!secure && !loopback)
    || endpoint.authority().is_none()
    || endpoint
      .authority()
      .is_some_and(|authority| authority.as_str().contains('@'))
    || endpoint.path() != "/"
    || endpoint.query().is_some()
  {
    return Err(S3ArtifactStoreConfigError::InvalidEndpoint);
  }
  if !is_safe_identifier(region) {
    return Err(S3ArtifactStoreConfigError::InvalidRegion);
  }
  if !(3..=63).contains(&bucket.len())
    || !bucket
      .bytes()
      .next()
      .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    || !bucket
      .bytes()
      .next_back()
      .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    || !bucket
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
    || bucket.contains("..")
    || bucket.contains(".-")
    || bucket.contains("-.")
    || bucket.parse::<std::net::Ipv4Addr>().is_ok()
  {
    return Err(S3ArtifactStoreConfigError::InvalidBucket);
  }
  if prefix.len() > 256
    || prefix.starts_with('/')
    || prefix.ends_with('/')
    || (!prefix.is_empty() && prefix.split('/').any(|segment| !is_safe_identifier(segment)))
  {
    return Err(S3ArtifactStoreConfigError::InvalidPrefix);
  }
  if operation_timeout.is_zero() || operation_timeout > MAX_OPERATION_TIMEOUT {
    return Err(S3ArtifactStoreConfigError::InvalidOperationTimeout);
  }
  if capability_recheck_interval.is_zero() {
    return Err(S3ArtifactStoreConfigError::InvalidCapabilityRecheckInterval);
  }
  Ok(())
}

pub(crate) fn validate_config(config: &S3ArtifactStoreConfig) -> Result<(), S3ArtifactStoreConfigError> {
  validate_s3_settings(
    &config.endpoint,
    &config.region,
    &config.bucket,
    &config.prefix,
    config.operation_timeout,
    config.capability_recheck_interval,
  )?;
  if [config.access_key.as_str(), config.secret_key.as_str()]
    .iter()
    .any(|value| value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control))
  {
    return Err(S3ArtifactStoreConfigError::InvalidCredentials);
  }
  Ok(())
}

fn is_safe_identifier(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= 256
    && value
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
