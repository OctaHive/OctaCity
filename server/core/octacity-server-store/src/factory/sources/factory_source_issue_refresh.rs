//! A pending input may advance; its automatic identity and original retention never do.
use crate::*;
use octacity_server_factory::FactoryDigest;

/// Explicit compare-and-swap of a canonical snapshot before the first admission.
#[derive(Clone)]
pub struct RefreshSourceIssueInput {
  /// Original snapshot seal; stale workers cannot replace a newer preparation.
  pub expected_snapshot: FactoryDigest,
  /// Current Source security partition, also used to exclude an existing admission.
  pub security_scope: ManagementSecurityScope,
  /// Fresh canonical documents and normalized policy under the current scan lease.
  pub replacement: FreezeSourcePreparationPolicy,
}
impl RefreshSourceIssueInput {
  /// Stores no new journal: update the existing preparation and cleanup manifest together.
  pub fn retain(
    &self,
    mut preparation: WorkSourcePreparation,
    mut materialization: Option<SourceMaterialization>,
  ) -> Result<(WorkSourcePreparation, Option<SourceMaterialization>), StoreError> {
    let original = preparation
      .issue_input
      .as_ref()
      .and_then(ProjectTextDocuments::issue_snapshot)
      .ok_or_else(source_conflict)?;
    let next = self
      .replacement
      .issue_input
      .as_ref()
      .and_then(ProjectTextDocuments::issue_snapshot)
      .ok_or_else(source_conflict)?;
    if preparation.id != self.replacement.preparation_id
      || preparation.admission.is_some()
      || original.digest()? != self.expected_snapshot
      || original.issue_id != next.issue_id
      || original.target != next.target
      || original.input != next.input
    {
      return Err(source_conflict());
    }
    if let Some(manifest) = &mut materialization {
      if manifest.cleaned_at.is_some()
        || manifest.rejection.is_some()
        || self.replacement.now < manifest.reserved_at
        || self.replacement.now >= manifest.expires_at
      {
        return Err(source_conflict());
      }
      manifest.retired_objects = manifest.cleanup_objects();
      manifest.objects.clear();
      manifest.mapping_digest = FactoryDigest::content_sha256(&self.replacement.policy);
      manifest.published_at = None;
      // Superseded bytes have no new owner. Bound their cleanup metadata using
      // the existing Source input budget rather than retaining unlimited history.
      if serde_json::to_vec(manifest).map_err(|_| source_conflict())?.len() > MAX_SOURCE_INPUT_BYTES as usize {
        return Err(source_conflict());
      }
    }
    preparation.policy = None;
    preparation.issue_input = None;
    preparation.intent = None;
    preparation.waiting = None;
    let preparation = self.replacement.retain(preparation, materialization.as_ref())?;
    Ok((preparation, materialization))
  }
}
