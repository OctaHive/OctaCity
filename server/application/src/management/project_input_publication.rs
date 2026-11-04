//! Shared immutable byte verification for every purpose in the Project input journal.
use super::*;
use octacity_server_store::FactoryInputObject;

pub(super) fn input_object(input: &FactoryInputObject) -> Result<ArtifactObject, ApplicationError> {
  ArtifactObject::new(
    input.artifact.artifact_id(),
    input.upload_id,
    input.artifact.encoded_size(),
    input.artifact.content_digest().to_string(),
    input.media_type.as_str(),
  )
  .map_err(|_| ApplicationError::invalid())
}

/// Missing bytes are recoverable; present but corrupt bytes are never silently replaced.
pub(super) async fn is_input_published(
  reader: &impl ArtifactByteReader,
  input: &FactoryInputObject,
) -> Result<bool, ApplicationError> {
  let object = input_object(input)?;
  let bytes = match reader.read_published(&object, object.size_bytes()).await {
    Ok(bytes) => bytes,
    Err(octacity_artifact_store::ArtifactStoreError::NotFound) => return Ok(false),
    Err(_) => return Err(ApplicationError::unavailable()),
  };
  if bytes.len() as u64 != input.artifact.encoded_size()
    || octacity_server_factory::FactoryDigest::content_sha256(&bytes) != input.artifact.content_digest()
  {
    return Err(ApplicationError::unavailable());
  }
  Ok(true)
}
