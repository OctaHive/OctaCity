//! Retained immutable inputs handed to an explicitly installed private connector.
use super::{FactoryDeliveryInputs, FactoryDeliveryMaterializer, FactoryRuntimeInstallationError};
use octacity_artifact_store::{ArtifactByteReader, MAX_ARTIFACT_READ_BYTES};
use octacity_server_application::{
  ApplicationError, FactoryArtifactReader, FactoryCandidatePublicationRequest, FactoryExecutionClock,
  ReadFactoryArtifact,
};
use octacity_server_factory::{FactoryArtifactReference, ImmutableReference};
use octacity_server_store::{ArtifactRecordStore, BuildQueryStore};
use sha2::{Digest as _, Sha256};
use std::{
  collections::BTreeMap,
  io::Write as _,
  path::{Path, PathBuf},
  sync::Arc,
};

/// Stages only Project-owned, digest-verified publications under opaque handle
/// hashes. Directories are trusted installation metadata, never Flow inputs.
/// Unix owner-private directories are qualified; other platforms fail closed
/// until their private-directory access boundary is qualified.
pub struct RetainedFactoryDeliveryMaterializer<A, Q, B> {
  reader: Arc<FactoryArtifactReader<A, Q, B>>,
  directories: BTreeMap<ImmutableReference, PathBuf>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader> RetainedFactoryDeliveryMaterializer<A, Q, B> {
  /// Binds exact installed target policies to existing private connector input
  /// directories. Construction reads no Artifact or external dependency.
  pub fn new(
    reader: Arc<FactoryArtifactReader<A, Q, B>>,
    mut directories: BTreeMap<ImmutableReference, PathBuf>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, FactoryRuntimeInstallationError> {
    if directories.is_empty() || directories.len() > octacity_server_application::MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(FactoryRuntimeInstallationError::Invalid);
    }
    for directory in directories.values_mut() {
      *directory = private_directory(directory).map_err(|_| FactoryRuntimeInstallationError::Invalid)?;
    }
    Ok(Self {
      reader,
      directories,
      clock,
    })
  }
}

#[async_trait::async_trait]
impl<A: ArtifactRecordStore, Q: BuildQueryStore, B: ArtifactByteReader> FactoryDeliveryMaterializer
  for RetainedFactoryDeliveryMaterializer<A, Q, B>
{
  async fn stage(
    &self,
    request: &FactoryCandidatePublicationRequest,
    inputs: &FactoryDeliveryInputs,
  ) -> Result<(), ApplicationError> {
    let directory = self
      .directories
      .get(&request.target)
      .ok_or_else(ApplicationError::invalid)?;
    let until = request.deadline.min(request.authority_expires_at);
    let now = self.clock.now()?;
    validate_window(request, now)?;
    let total = request
      .bundle
      .encoded_size()
      .checked_add(request.manifest.encoded_size())
      .ok_or_else(ApplicationError::invalid)?;
    if total > request.budget.max_output_bytes().min(MAX_ARTIFACT_READ_BYTES) {
      return Err(ApplicationError::invalid());
    }
    let selected = [
      (&request.bundle, &inputs.bundle, "bundle"),
      (&request.manifest, &inputs.manifest, "manifest"),
    ];
    for (artifact, input, role) in selected {
      if *input != super::delivery::delivery_artifact_input(request.operation_id, role, artifact) {
        return Err(ApplicationError::invalid());
      }
    }
    private_directory(directory)?;
    let read = |artifact: &FactoryArtifactReference| {
      self.reader.read(ReadFactoryArtifact {
        project_id: request.candidate.exact().project_id(),
        artifact: artifact.clone(),
        maximum_bytes: total,
        observed_at: now,
        required_until: until,
      })
    };
    // Verify both retained objects before publishing either private handle.
    let bundle = read(&request.bundle).await?;
    let manifest = read(&request.manifest).await?;
    // The native capture envelope is provider-neutral. Bind both independently
    // verified objects to the composite domain candidate before either handle
    // becomes visible to the connector. A later coding cycle may have a parent
    // different from the original Work base; cumulative ancestry retains that base.
    let captured = octacity_protocol::CapturedChangeSetManifestV1::decode_canonical(manifest.bytes())
      .map_err(|_| ApplicationError::invalid())?;
    if captured
      .candidate_digest(&request.manifest.content_digest().to_string())
      .map_err(|_| ApplicationError::invalid())?
      != request.candidate.changeset_digest().to_string()
      || captured.candidate_revision != request.candidate.candidate_revision().as_str()
      || captured.bundle.sha256 != request.bundle.content_digest().to_string()
      || captured.bundle.size_bytes != request.bundle.encoded_size()
      || captured
        .cumulative_changes
        .as_ref()
        .is_none_or(|changes| changes.base_revision != request.candidate.exact().base_revision().as_str())
    {
      return Err(ApplicationError::invalid());
    }
    validate_window(request, self.clock.now()?)?;
    private_directory(directory)?;
    for (input, bytes) in [(&inputs.bundle, bundle.bytes()), (&inputs.manifest, manifest.bytes())] {
      let path = directory.join(format!("{}.bin", hex::encode(Sha256::digest(&input.handle))));
      let mut temporary = tempfile::NamedTempFile::new_in(directory).map_err(|_| ApplicationError::unavailable())?;
      temporary
        .write_all(bytes)
        .map_err(|_| ApplicationError::unavailable())?;
      temporary
        .as_file()
        .sync_all()
        .map_err(|_| ApplicationError::unavailable())?;
      if let Err(error) = temporary.persist_noclobber(&path) {
        if error.error.kind() != std::io::ErrorKind::AlreadyExists {
          return Err(ApplicationError::unavailable());
        }
        let retained = super::assembly::read_private_bytes("Delivery input", &path, input.size_bytes)
          .await
          .map_err(|_| ApplicationError::invalid())?;
        if retained.as_slice() != bytes {
          return Err(ApplicationError::invalid());
        }
      }
    }
    Ok(())
  }
}

fn validate_window(
  request: &FactoryCandidatePublicationRequest,
  now: octacity_server_domain::Timestamp,
) -> Result<(), ApplicationError> {
  if now < request.started_at
    || now >= request.deadline.min(request.authority_expires_at)
    || u64::try_from(now.unix_millis() - request.started_at.unix_millis()).map_err(|_| ApplicationError::invalid())?
      >= request.budget.max_elapsed_millis()
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}

fn private_directory(path: &Path) -> Result<PathBuf, ApplicationError> {
  if !path.is_absolute() {
    return Err(ApplicationError::invalid());
  }
  let metadata = std::fs::symlink_metadata(path).map_err(|_| ApplicationError::invalid())?;
  if !metadata.file_type().is_dir() {
    return Err(ApplicationError::invalid());
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::MetadataExt as _;
    if metadata.mode() & 0o077 != 0 || metadata.uid() != rustix::process::geteuid().as_raw() {
      return Err(ApplicationError::invalid());
    }
    path.canonicalize().map_err(|_| ApplicationError::invalid())
  }
  #[cfg(not(unix))]
  {
    Err(ApplicationError::invalid())
  }
}
