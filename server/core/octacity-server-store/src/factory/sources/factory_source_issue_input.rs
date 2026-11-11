//! Frozen Issue input shares ordinary document/Incoming validation and Source replay identity.
use crate::*;

impl WorkSourcePreparation {
  /// Byte publication and admission must consume the same current snapshot documents.
  pub(crate) fn require_issue_input_bytes(
    &self,
    role: SourceArtifactRole,
    artifact: &octacity_server_factory::FactoryArtifactReference,
  ) -> Result<(), StoreError> {
    let Some(documents) = &self.issue_input else {
      return Ok(());
    };
    let bytes = match role {
      SourceArtifactRole::Task => documents.task(),
      SourceArtifactRole::Acceptance => documents.acceptance(),
      SourceArtifactRole::Document => return Err(source_conflict()),
    };
    if artifact.encoded_size() != bytes.len() as u64
      || artifact.content_digest() != octacity_server_factory::FactoryDigest::content_sha256(bytes)
    {
      return Err(source_conflict());
    }
    Ok(())
  }
  /// Checks ownership and target pins before retaining canonical bytes with Source policy.
  pub fn require_issue_input_target(&self, documents: &ProjectTextDocuments) -> Result<(), StoreError> {
    let snapshot = documents.issue_snapshot().ok_or_else(source_conflict)?;
    let definition = self.input.definition();
    if snapshot.project_id != self.input.project_id()
      || snapshot.target.configuration_id != definition.target.id()
      || snapshot.target.configuration_version != definition.target.version()
      || snapshot.target.repository_id != definition.repository.id
      || snapshot.target.repository_version != definition.repository.version
      || snapshot.target.predecessor_run.is_some()
    {
      return Err(source_conflict());
    }
    Ok(())
  }

  /// Verifies exact frozen input rather than accepting caller-supplied Incoming or Work text.
  pub fn validate_issue_input(&self, request: &AdmitFactoryWork) -> Result<(), StoreError> {
    let Some(documents) = &self.issue_input else {
      if request
        .incoming
        .as_ref()
        .and_then(|input| input.issue_snapshot())
        .is_some()
      {
        return Err(source_conflict());
      }
      return Ok(());
    };
    documents.validate()?;
    self.require_issue_input_target(documents)?;
    let snapshot = documents.issue_snapshot().ok_or_else(source_conflict)?;
    let intent = self.intent.as_ref().ok_or_else(source_conflict)?;
    self.require_issue_input_bytes(SourceArtifactRole::Task, request.work.artifacts().task())?;
    self.require_issue_input_bytes(SourceArtifactRole::Acceptance, request.work.artifacts().acceptance())?;
    if snapshot.target.source != *intent.source.repository()?
      || snapshot
        .admission_classification
        .as_ref()
        .is_some_and(|c| c != &intent.classification)
    {
      return Err(source_conflict());
    }
    snapshot.validate_admitted_input(request)
  }
}
