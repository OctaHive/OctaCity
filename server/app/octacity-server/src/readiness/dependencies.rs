use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_server_job::JobSpecSigner;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use thiserror::Error;
use tokio::io::AsyncReadExt as _;
use zeroize::{Zeroize as _, Zeroizing};

use super::{ReadinessCheck, ReadinessChecks};
use crate::ServerConfig;

const MAX_CREDENTIAL_BYTES: u64 = 64 * 1024;
const SIGNING_KEY_BYTES: usize = 32;

impl ReadinessChecks {
  pub(crate) async fn from_config(config: &ServerConfig) -> Result<Self, ReadinessSetupError> {
    let database_url = read_credential("PostgreSQL URL", &config.postgres().url_file).await?;
    let connect_options =
      database_url
        .parse::<PgConnectOptions>()
        .map_err(|_| ReadinessSetupError::InvalidCredential {
          purpose: "PostgreSQL URL",
        })?;
    let pool = PgPoolOptions::new()
      .max_connections(config.postgres().max_connections)
      .acquire_timeout(config.readiness_check_timeout())
      .connect_lazy_with(connect_options);

    let object_config = config.object_storage();
    let access_key = read_credential("object-store access key", &object_config.access_key_file).await?;
    let secret_key = read_credential("object-store secret key", &object_config.secret_key_file).await?;
    let object_storage = S3ArtifactStore::new(S3ArtifactStoreConfig {
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
    .map_err(ReadinessSetupError::ObjectStorageConfig)?;

    let signing_key = read_signing_key(&config.signing().key_id, &config.signing().key_file).await?;
    Ok(Self::new(
      Arc::new(MigrationCheck { pool: pool.clone() }),
      Arc::new(PostgresCheck { pool }),
      Arc::new(ObjectStorageCheck(object_storage)),
      Arc::new(SigningMaterialCheck(signing_key)),
      std::iter::empty(),
    ))
  }
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

struct ObjectStorageCheck(S3ArtifactStore);

#[async_trait]
impl ReadinessCheck for ObjectStorageCheck {
  fn name(&self) -> &'static str {
    "object-storage"
  }

  async fn check(&self) -> bool {
    self.0.health_check().await.is_ok()
  }
}

struct SigningMaterialCheck(JobSpecSigner);

#[async_trait]
impl ReadinessCheck for SigningMaterialCheck {
  fn name(&self) -> &'static str {
    "job-spec-signing-material"
  }

  async fn check(&self) -> bool {
    self.0.is_usable()
  }
}

async fn read_signing_key(key_id: &str, path: &Path) -> Result<JobSpecSigner, ReadinessSetupError> {
  let encoded = read_credential("JobSpec signing key", path).await?;
  let decoded =
    Zeroizing::new(
      STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| ReadinessSetupError::InvalidCredential {
          purpose: "JobSpec signing key",
        })?,
    );
  let mut bytes: [u8; SIGNING_KEY_BYTES] =
    decoded
      .as_slice()
      .try_into()
      .map_err(|_| ReadinessSetupError::InvalidCredential {
        purpose: "JobSpec signing key",
      })?;
  let signer = JobSpecSigner::new(key_id, bytes);
  bytes.zeroize();
  signer.map_err(|_| ReadinessSetupError::InvalidCredential {
    purpose: "JobSpec signing key",
  })
}

async fn read_credential(purpose: &'static str, path: &Path) -> Result<Zeroizing<String>, ReadinessSetupError> {
  let path_metadata =
    tokio::fs::symlink_metadata(path)
      .await
      .map_err(|source| ReadinessSetupError::ReadCredential {
        purpose,
        path: path.to_owned(),
        source,
      })?;
  if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
    return Err(ReadinessSetupError::UnsafeCredentialFile {
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
    .map_err(|reason| ReadinessSetupError::UnsafeCredentialFile {
      purpose,
      path: path.to_owned(),
      reason,
    })?;
  }

  let file = tokio::fs::File::open(path)
    .await
    .map_err(|source| ReadinessSetupError::ReadCredential {
      purpose,
      path: path.to_owned(),
      source,
    })?;
  let opened_metadata = file
    .metadata()
    .await
    .map_err(|source| ReadinessSetupError::ReadCredential {
      purpose,
      path: path.to_owned(),
      source,
    })?;
  if !opened_metadata.is_file() {
    return Err(ReadinessSetupError::UnsafeCredentialFile {
      purpose,
      path: path.to_owned(),
      reason: "changed while it was being opened",
    });
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    if (path_metadata.dev(), path_metadata.ino()) != (opened_metadata.dev(), opened_metadata.ino()) {
      return Err(ReadinessSetupError::UnsafeCredentialFile {
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
    .map_err(|reason| ReadinessSetupError::UnsafeCredentialFile {
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
    .map_err(|source| ReadinessSetupError::ReadCredential {
      purpose,
      path: path.to_owned(),
      source,
    })?;
  if bytes.len() as u64 > MAX_CREDENTIAL_BYTES {
    return Err(ReadinessSetupError::CredentialTooLarge {
      purpose,
      path: path.to_owned(),
    });
  }
  let value = std::str::from_utf8(&bytes)
    .map_err(|_| ReadinessSetupError::InvalidCredential { purpose })?
    .trim_end_matches(['\r', '\n']);
  if value.is_empty() || value.chars().any(char::is_control) {
    return Err(ReadinessSetupError::InvalidCredential { purpose });
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

/// Failure to load static material required by the readiness checks.
#[derive(Debug, Error)]
pub enum ReadinessSetupError {
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
      Err(ReadinessSetupError::UnsafeCredentialFile { .. })
    ));

    std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(read_credential("test", &credential).await.unwrap().as_str(), "secret");

    let link = directory.path().join("credential-link");
    symlink(&credential, &link).unwrap();
    assert!(matches!(
      read_credential("test", &link).await,
      Err(ReadinessSetupError::UnsafeCredentialFile { .. })
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
