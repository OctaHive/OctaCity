use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_server_application::{AgentEnrollmentSecretKey, CacheCredentialKey, JobSpecToolchainPolicy, LogRedactor};
use octacity_server_job::JobSpecSigner;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use thiserror::Error;
use tokio::io::AsyncReadExt as _;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{
  ServerConfig,
  readiness::{ReadinessCheck, ReadinessChecks},
};

const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const SIGNING_KEY_BYTES: usize = 32;
const MAX_JOB_SPEC_POLICY_BYTES: u64 = 1024 * 1024;

pub(crate) struct RuntimeResources {
  pub(crate) readiness: ReadinessChecks,
  pub(crate) postgres: sqlx::PgPool,
  pub(crate) job_spec_signer: Arc<JobSpecSigner>,
  pub(crate) job_spec_toolchain: JobSpecToolchainPolicy,
  pub(crate) agent_enrollment_secret_key: AgentEnrollmentSecretKey,
  pub(crate) cache_credential_key: CacheCredentialKey,
  pub(crate) log_redactor: LogRedactor,
  pub(crate) object_storage: Arc<S3ArtifactStore>,
}

impl RuntimeResources {
  pub(crate) async fn from_config(config: &ServerConfig) -> Result<Self, RuntimeAssemblyError> {
    let database_url = read_credential("PostgreSQL URL", &config.postgres().url_file).await?;
    let connect_options =
      database_url
        .parse::<PgConnectOptions>()
        .map_err(|_| RuntimeAssemblyError::InvalidCredential {
          purpose: "PostgreSQL URL",
        })?;
    let pool = PgPoolOptions::new()
      .max_connections(config.postgres().max_connections)
      .acquire_timeout(config.readiness_check_timeout())
      .connect_lazy_with(connect_options);

    let object_config = config.object_storage();
    let access_key = read_credential("object-store access key", &object_config.access_key_file).await?;
    let secret_key = read_credential("object-store secret key", &object_config.secret_key_file).await?;
    let signing_material = read_credential("JobSpec signing key", &config.signing().key_file).await?;
    let enrollment_material = read_credential(
      "Agent enrollment derivation key",
      &config.agent_credentials().enrollment_key_file,
    )
    .await?;
    let cache_material =
      read_credential("cache credential derivation key", &config.cache().credential_key_file).await?;
    let mut protected = vec![
      database_url.as_bytes().to_vec(),
      access_key.as_bytes().to_vec(),
      secret_key.as_bytes().to_vec(),
      signing_material.as_bytes().to_vec(),
      enrollment_material.as_bytes().to_vec(),
      cache_material.as_bytes().to_vec(),
    ];
    if let Ok(database_url) = url::Url::parse(&database_url)
      && let Some(password) = database_url.password()
      && !password.is_empty()
    {
      protected.push(password.as_bytes().to_vec());
    }
    let log_redactor = LogRedactor::new(protected).map_err(|_| RuntimeAssemblyError::InvalidCredential {
      purpose: "log redaction material",
    })?;
    let object_storage = Arc::new(
      S3ArtifactStore::new(S3ArtifactStoreConfig {
        endpoint: object_config.endpoint.clone(),
        region: object_config.region.clone(),
        bucket: object_config.bucket.clone(),
        prefix: object_config.prefix.clone(),
        access_key,
        secret_key,
        force_path_style: object_config.force_path_style,
        operation_timeout: object_config.operation_timeout(),
        capability_recheck_interval: object_config.capability_recheck_interval(),
      })
      .map_err(RuntimeAssemblyError::ObjectStorageConfig)?,
    );

    let signing_key = Arc::new(parse_signing_key(&config.signing().key_id, &signing_material)?);
    let agent_enrollment_secret_key = decode_fixed_key(
      "Agent enrollment derivation key",
      &enrollment_material,
      AgentEnrollmentSecretKey::new,
    )?;
    let cache_credential_key = decode_fixed_key(
      "cache credential derivation key",
      &cache_material,
      CacheCredentialKey::new,
    )?;
    let job_spec_toolchain = load_job_spec_toolchain(&config.job_spec().policy_file).await?;
    let readiness = ReadinessChecks::new(
      Arc::new(MigrationCheck { pool: pool.clone() }),
      Arc::new(PostgresCheck { pool: pool.clone() }),
      Arc::new(ObjectStorageCheck(Arc::clone(&object_storage))),
      Arc::new(SigningMaterialCheck(signing_key.clone())),
      std::iter::empty(),
    );
    Ok(Self {
      readiness,
      postgres: pool,
      job_spec_signer: signing_key,
      job_spec_toolchain,
      agent_enrollment_secret_key,
      cache_credential_key,
      log_redactor,
      object_storage,
    })
  }
}

fn decode_fixed_key<const N: usize, T>(
  purpose: &'static str,
  encoded: &str,
  constructor: impl FnOnce([u8; N]) -> T,
) -> Result<T, RuntimeAssemblyError> {
  let decoded = Zeroizing::new(
    STANDARD
      .decode(encoded.as_bytes())
      .map_err(|_| RuntimeAssemblyError::InvalidCredential { purpose })?,
  );
  let mut bytes: [u8; N] = decoded
    .as_slice()
    .try_into()
    .map_err(|_| RuntimeAssemblyError::InvalidCredential { purpose })?;
  let key = constructor(bytes);
  bytes.zeroize();
  Ok(key)
}

async fn load_job_spec_toolchain(path: &Path) -> Result<JobSpecToolchainPolicy, RuntimeAssemblyError> {
  let file = tokio::fs::File::open(path)
    .await
    .map_err(|source| RuntimeAssemblyError::ReadJobSpecPolicy {
      path: path.to_owned(),
      source,
    })?;
  let mut bytes = Vec::new();
  file
    .take(MAX_JOB_SPEC_POLICY_BYTES + 1)
    .read_to_end(&mut bytes)
    .await
    .map_err(|source| RuntimeAssemblyError::ReadJobSpecPolicy {
      path: path.to_owned(),
      source,
    })?;
  if bytes.len() as u64 > MAX_JOB_SPEC_POLICY_BYTES {
    return Err(RuntimeAssemblyError::JobSpecPolicyTooLarge { path: path.to_owned() });
  }
  let policy = serde_json::from_slice::<JobSpecToolchainPolicy>(&bytes)
    .map_err(|_| RuntimeAssemblyError::InvalidJobSpecPolicy { path: path.to_owned() })?;
  policy
    .validate()
    .map_err(|_| RuntimeAssemblyError::InvalidJobSpecPolicy { path: path.to_owned() })?;
  Ok(policy)
}

struct MigrationCheck {
  pool: sqlx::PgPool,
}

#[async_trait]
impl ReadinessCheck for MigrationCheck {
  fn name(&self) -> &'static str {
    "postgres-migrations"
  }

  async fn check(&self) -> bool {
    use octacity_server_store_postgres::MigrationStatus;

    match octacity_server_store_postgres::migration_status(&self.pool).await {
      Ok(MigrationStatus::Current) => true,
      Ok(MigrationStatus::Pending) => octacity_server_store_postgres::migrate(&self.pool).await.is_ok(),
      Ok(MigrationStatus::Incompatible) | Err(_) => false,
    }
  }
}

struct PostgresCheck {
  pool: sqlx::PgPool,
}

#[async_trait]
impl ReadinessCheck for PostgresCheck {
  fn name(&self) -> &'static str {
    "postgres"
  }

  async fn check(&self) -> bool {
    octacity_server_store_postgres::health_check(&self.pool).await
  }
}

struct ObjectStorageCheck(Arc<S3ArtifactStore>);

#[async_trait]
impl ReadinessCheck for ObjectStorageCheck {
  fn name(&self) -> &'static str {
    "object-storage"
  }

  async fn check(&self) -> bool {
    self.0.health_check().await.is_ok()
  }
}

struct SigningMaterialCheck(Arc<JobSpecSigner>);

#[async_trait]
impl ReadinessCheck for SigningMaterialCheck {
  fn name(&self) -> &'static str {
    "job-spec-signing-material"
  }

  async fn check(&self) -> bool {
    self.0.is_usable()
  }
}

fn parse_signing_key(key_id: &str, encoded: &str) -> Result<JobSpecSigner, RuntimeAssemblyError> {
  let decoded =
    Zeroizing::new(
      STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| RuntimeAssemblyError::InvalidCredential {
          purpose: "JobSpec signing key",
        })?,
    );
  let mut bytes: [u8; SIGNING_KEY_BYTES] =
    decoded
      .as_slice()
      .try_into()
      .map_err(|_| RuntimeAssemblyError::InvalidCredential {
        purpose: "JobSpec signing key",
      })?;
  let signer = JobSpecSigner::new(key_id, bytes);
  bytes.zeroize();
  signer.map_err(|_| RuntimeAssemblyError::InvalidCredential {
    purpose: "JobSpec signing key",
  })
}

async fn read_credential(purpose: &'static str, path: &Path) -> Result<Zeroizing<String>, RuntimeAssemblyError> {
  let path_metadata =
    tokio::fs::symlink_metadata(path)
      .await
      .map_err(|source| RuntimeAssemblyError::ReadCredential {
        purpose,
        path: path.to_owned(),
        source,
      })?;
  if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
    return Err(RuntimeAssemblyError::UnsafeCredentialFile {
      purpose,
      path: path.to_owned(),
      reason: "must be a regular file and not a symbolic link",
    });
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    validate_unix_credential_metadata(
      path_metadata.permissions().mode(),
      path_metadata.uid(),
      rustix::process::geteuid().as_raw(),
    )
    .map_err(|reason| RuntimeAssemblyError::UnsafeCredentialFile {
      purpose,
      path: path.to_owned(),
      reason,
    })?;
  }

  let file = tokio::fs::File::open(path)
    .await
    .map_err(|source| RuntimeAssemblyError::ReadCredential {
      purpose,
      path: path.to_owned(),
      source,
    })?;
  let opened_metadata = file
    .metadata()
    .await
    .map_err(|source| RuntimeAssemblyError::ReadCredential {
      purpose,
      path: path.to_owned(),
      source,
    })?;
  if !opened_metadata.is_file() {
    return Err(RuntimeAssemblyError::UnsafeCredentialFile {
      purpose,
      path: path.to_owned(),
      reason: "changed while it was being opened",
    });
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    if (path_metadata.dev(), path_metadata.ino()) != (opened_metadata.dev(), opened_metadata.ino()) {
      return Err(RuntimeAssemblyError::UnsafeCredentialFile {
        purpose,
        path: path.to_owned(),
        reason: "changed while it was being opened",
      });
    }
    validate_unix_credential_metadata(
      opened_metadata.permissions().mode(),
      opened_metadata.uid(),
      rustix::process::geteuid().as_raw(),
    )
    .map_err(|reason| RuntimeAssemblyError::UnsafeCredentialFile {
      purpose,
      path: path.to_owned(),
      reason,
    })?;
  }
  let mut bytes = Zeroizing::new(Vec::new());
  file
    .take(MAX_CREDENTIAL_BYTES + 1)
    .read_to_end(&mut bytes)
    .await
    .map_err(|source| RuntimeAssemblyError::ReadCredential {
      purpose,
      path: path.to_owned(),
      source,
    })?;
  if bytes.len() as u64 > MAX_CREDENTIAL_BYTES {
    return Err(RuntimeAssemblyError::CredentialTooLarge {
      purpose,
      path: path.to_owned(),
    });
  }
  let value = std::str::from_utf8(&bytes)
    .map_err(|_| RuntimeAssemblyError::InvalidCredential { purpose })?
    .trim_end_matches(['\r', '\n']);
  if value.is_empty() || value.chars().any(char::is_control) {
    return Err(RuntimeAssemblyError::InvalidCredential { purpose });
  }
  Ok(Zeroizing::new(value.to_owned()))
}

#[cfg(unix)]
fn validate_unix_credential_metadata(mode: u32, owner_uid: u32, effective_uid: u32) -> Result<(), &'static str> {
  if owner_uid != effective_uid {
    return Err("must be owned by the server process owner");
  }
  if mode & 0o077 != 0 {
    return Err("must not be accessible by group or other users");
  }
  Ok(())
}

/// Failure to assemble the server's static runtime resources safely.
#[derive(Debug, Error)]
pub enum RuntimeAssemblyError {
  /// The JobSpec toolchain policy file could not be opened or read.
  #[error("failed to read JobSpec policy file '{path}': {source}")]
  ReadJobSpecPolicy {
    /// Configured policy path.
    path: std::path::PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// The JobSpec toolchain policy exceeded its parser limit.
  #[error("JobSpec policy file '{path}' exceeds the {MAX_JOB_SPEC_POLICY_BYTES}-byte limit")]
  JobSpecPolicyTooLarge {
    /// Configured policy path.
    path: std::path::PathBuf,
  },
  /// The JobSpec toolchain policy was malformed or semantically invalid.
  #[error("JobSpec policy file '{path}' is invalid")]
  InvalidJobSpecPolicy {
    /// Configured policy path.
    path: std::path::PathBuf,
  },
  /// A credential file could not be opened or read.
  #[error("failed to read {purpose} file '{path}': {source}")]
  ReadCredential {
    /// Safe purpose of the credential.
    purpose: &'static str,
    /// Operator-configured credential path.
    path: std::path::PathBuf,
    /// Underlying filesystem failure.
    source: std::io::Error,
  },
  /// A credential file exceeded the bounded input limit.
  #[error("{purpose} file '{path}' exceeds the {MAX_CREDENTIAL_BYTES}-byte limit")]
  CredentialTooLarge {
    /// Safe purpose of the credential.
    purpose: &'static str,
    /// Operator-configured credential path.
    path: std::path::PathBuf,
  },
  /// A credential path was not a stable owner-only regular file.
  #[error("unsafe {purpose} file '{path}': {reason}")]
  UnsafeCredentialFile {
    /// Safe purpose of the credential.
    purpose: &'static str,
    /// Operator-configured credential path.
    path: std::path::PathBuf,
    /// Non-sensitive rejection reason.
    reason: &'static str,
  },
  /// Static credential material was malformed.
  #[error("{purpose} is invalid")]
  InvalidCredential {
    /// Safe purpose of the credential.
    purpose: &'static str,
  },
  /// Object-storage configuration was rejected before network access.
  #[error("invalid object-store configuration: {0}")]
  ObjectStorageConfig(octacity_artifact_s3::S3ArtifactStoreConfigError),
}

#[cfg(all(test, unix))]
mod tests {
  use std::os::unix::fs::{PermissionsExt as _, symlink};

  use super::*;

  #[tokio::test]
  async fn credential_files_must_be_regular_owner_only_files() {
    let directory = tempfile::tempdir().unwrap();
    let credential = directory.path().join("credential");
    std::fs::write(&credential, "secret").unwrap();
    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
      read_credential("test", &credential).await,
      Err(RuntimeAssemblyError::UnsafeCredentialFile { .. })
    ));

    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(read_credential("test", &credential).await.unwrap().as_str(), "secret");

    let link = directory.path().join("credential-link");
    symlink(&credential, &link).unwrap();
    assert!(matches!(
      read_credential("test", &link).await,
      Err(RuntimeAssemblyError::UnsafeCredentialFile { .. })
    ));
  }

  #[test]
  fn credential_metadata_rejects_a_different_owner_uid() {
    assert_eq!(validate_unix_credential_metadata(0o100600, 1000, 1000), Ok(()));
    assert_eq!(
      validate_unix_credential_metadata(0o100600, 0, 1000),
      Err("must be owned by the server process owner")
    );
  }
}
