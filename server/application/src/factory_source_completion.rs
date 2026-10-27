//! Explicit reviewed process criteria, independent of ticket-authored implementation acceptance.
use crate::SourceMappingFailure;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest, ImmutableReference};
use serde::{Deserialize, Serialize};

/// Bundled versioned process-completion draft; review is explicit and creates no publication.
pub const TRIAGE_COMPLETION_PRESET: &[u8] = include_bytes!("../../../config/factory/presets/triage/completion-v1.json");

/// Reviewed immutable completion content; ticket data cannot select this policy.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceCompletionDocument {
  /// Versioned preset content reviewed in the owning Project before publication.
  Draft {
    /// Project that reviewed these process criteria.
    project_id: ProjectId,
    /// Exact version and digest of the reviewed UTF-8 content.
    preset: ImmutableReference,
    /// Exact private process criteria; never source-authored acceptance.
    content: String,
  },
  /// Exact previously published private document selected for this Project's process.
  ProjectInput {
    /// Owning Project; never supplied by ticket data.
    project_id: ProjectId,
    /// Exact private logical identity, content digest and encoded byte length.
    artifact: FactoryArtifactReference,
  },
}
impl std::fmt::Debug for SourceCompletionDocument {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("SourceCompletionDocument").finish_non_exhaustive()
  }
}
impl SourceCompletionDocument {
  /// Reviews a bounded versioned preset document without publishing an input or starting a Build.
  /// The ordinary Source preparation path publishes the exact bytes only after explicit selection.
  pub fn review_preset(project_id: ProjectId, bytes: &[u8]) -> Result<Self, SourceMappingFailure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Preset {
      preset: ImmutableReference,
      content: String,
    }
    if bytes.len() > octacity_server_store::MAX_SOURCE_INPUT_BYTES as usize {
      return Err(SourceMappingFailure::TooLarge);
    }
    let preset: Preset = serde_json::from_slice(bytes).map_err(|_| SourceMappingFailure::InvalidMapping)?;
    let document = Self::Draft {
      project_id,
      preset: preset.preset,
      content: preset.content,
    };
    document.validate(octacity_server_store::MAX_SOURCE_BODY_BYTES)?;
    Ok(document)
  }
  pub(crate) fn validate(&self, maximum: u32) -> Result<(), SourceMappingFailure> {
    match self {
      Self::Draft { preset, content, .. } => {
        validate_content(content, maximum)?;
        if FactoryDigest::content_sha256(content.as_bytes()) != preset.digest() {
          return Err(SourceMappingFailure::InvalidMapping);
        }
      }
      Self::ProjectInput { artifact, .. } => {
        if artifact.encoded_size() > u64::from(maximum) {
          return Err(SourceMappingFailure::TooLarge);
        }
      }
    }
    Ok(())
  }
  pub(crate) fn project_id(&self) -> ProjectId {
    match self {
      Self::Draft { project_id, .. } | Self::ProjectInput { project_id, .. } => *project_id,
    }
  }
  pub(crate) fn criteria<'a>(
    &'a self,
    project: ProjectId,
    maximum: u32,
    resolved: Option<&'a crate::FactoryRetainedInput>,
  ) -> Result<&'a str, SourceMappingFailure> {
    self.validate(maximum)?;
    if self.project_id() != project {
      return Err(SourceMappingFailure::InvalidMapping);
    }
    match self {
      Self::Draft { content, .. } => Ok(content),
      Self::ProjectInput { artifact, .. } => {
        let bytes = resolved.ok_or(SourceMappingFailure::InvalidMapping)?.bytes();
        if bytes.len() as u64 != artifact.encoded_size()
          || FactoryDigest::content_sha256(bytes) != artifact.content_digest()
        {
          return Err(SourceMappingFailure::InvalidMapping);
        }
        let content = std::str::from_utf8(bytes).map_err(|_| SourceMappingFailure::InvalidMapping)?;
        validate_content(content, maximum)?;
        Ok(content)
      }
    }
  }
  pub(crate) fn provenance(&self, resolved: Option<&crate::FactoryRetainedInput>) -> serde_json::Value {
    match self {
      Self::Draft { project_id, preset, .. } => serde_json::json!({"project_id":project_id,"preset":preset}),
      Self::ProjectInput { project_id, artifact } => serde_json::json!({"project_id":project_id,"artifact":artifact,
        "publication_provenance":resolved.map(crate::FactoryRetainedInput::provenance_digest)}),
    }
  }
}
fn validate_content(content: &str, maximum: u32) -> Result<(), SourceMappingFailure> {
  if content.len() > maximum as usize {
    return Err(SourceMappingFailure::TooLarge);
  }
  if content.trim().is_empty()
    || content
      .chars()
      .any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
  {
    return Err(SourceMappingFailure::InvalidMapping);
  }
  Ok(())
}
