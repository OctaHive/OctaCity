//! Pure mapping of private normalized ticket data; no provider I/O or authority.
use octacity_server_factory::WorkSourceBinding;
use octacity_server_store::{
  MAX_SOURCE_BODY_BYTES, MAX_SOURCE_INPUT_BYTES, MAX_SOURCE_RETENTION_MILLIS, MAX_SOURCE_TITLE_BYTES, WorkSourceItem,
  WorkSourcePreparation,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Provider-normalized untrusted ticket data, without URLs, comments or attachments.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceSnapshot {
  /// Provider selector observations bound to the verified page; never Factory target authority.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub selection_matches: Vec<octacity_server_factory::FactoryDigest>,
  /// Verified native container and item identity.
  pub item: WorkSourceItem,
  /// Human-facing ticket number or key, used only as provenance.
  pub display_id: String,
  /// Provider update identity, used only as observation evidence.
  pub update_identity: String,
  /// Untrusted task title.
  pub title: String,
  /// Untrusted task body; absence is preserved.
  pub body: Option<String>,
  /// Explicit adapter-normalized text fields, including configured acceptance fields.
  pub fields: BTreeMap<String, String>,
  /// Literal observed label evidence.
  pub labels: Vec<String>,
  /// Literal observed provider state, with no Flow meaning.
  pub state: String,
}
/// Explicit acceptance source; no implicit default or generated criteria.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceAcceptanceMapping {
  /// One configured normalized field, including Jira custom fields.
  Field(String),
  /// Entire body, only when explicitly configured as the acceptance contract.
  Body,
  /// Exact Markdown heading; retain its text through the next peer/parent heading.
  BodySection(String),
}
/// Trusted installed configuration pinned by its exact Source binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkSourceMapping {
  /// Exact immutable installed input settings; never selected by ticket text.
  pub source: WorkSourceBinding,
  /// Explicit required acceptance source.
  pub acceptance: SourceAcceptanceMapping,
  /// Whether an absent/empty body is a rejected candidate.
  pub require_body: bool,
  /// Maximum UTF-8 title length.
  pub max_title_bytes: u32,
  /// Maximum UTF-8 body or mapped field length.
  pub max_body_bytes: u32,
  /// Maximum encoded bytes in either complete artifact.
  pub max_artifact_bytes: u32,
  /// Private input lifetime from its original reservation.
  pub retention_millis: u64,
}
impl WorkSourceMapping {
  /// Validates installed limits and acceptance selection before any native read or preparation.
  pub fn validate(&self) -> Result<(), SourceMappingFailure> {
    if !self.valid_limits() {
      return Err(SourceMappingFailure::InvalidMapping);
    }
    match &self.acceptance {
      SourceAcceptanceMapping::Field(field) => validate_acceptance_field(field),
      SourceAcceptanceMapping::Body => Ok(()),
      SourceAcceptanceMapping::BodySection(heading) => acceptance_heading_level(heading).map(|_| ()),
    }
  }
  fn valid_limits(&self) -> bool {
    (1..=MAX_SOURCE_TITLE_BYTES).contains(&self.max_title_bytes)
      && (1..=MAX_SOURCE_BODY_BYTES).contains(&self.max_body_bytes)
      && (1..=MAX_SOURCE_INPUT_BYTES).contains(&self.max_artifact_bytes)
      && (1..=MAX_SOURCE_RETENTION_MILLIS).contains(&self.retention_millis)
  }
}
fn validate_acceptance_field(field: &str) -> Result<(), SourceMappingFailure> {
  if field.is_empty() || field.len() > 128 {
    Err(SourceMappingFailure::InvalidMapping)
  } else {
    Ok(())
  }
}
fn acceptance_heading_level(heading: &str) -> Result<usize, SourceMappingFailure> {
  let level = heading.bytes().take_while(|v| *v == b'#').count();
  if !(1..=6).contains(&level)
    || heading.len() > 128
    || heading.as_bytes().get(level) != Some(&b' ')
    || heading.contains(['\r', '\n'])
  {
    Err(SourceMappingFailure::InvalidMapping)
  } else {
    Ok(level)
  }
}
/// Safe classified mapping error; no title, body or provider diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SourceMappingFailure {
  /// Explicit acceptance content is absent.
  #[error("missing mapped acceptance")]
  MissingAcceptance,
  /// Required title is absent.
  #[error("missing mapped title")]
  MissingTitle,
  /// Required body is absent.
  #[error("missing mapped body")]
  MissingBody,
  /// A configured or hard byte ceiling was exceeded.
  #[error("source input exceeds its byte limit")]
  TooLarge,
  /// Mapping is invalid or does not match the pinned installed configuration.
  #[error("invalid source input mapping")]
  InvalidMapping,
  /// Snapshot is not the normalized closed data contract.
  #[error("invalid source snapshot")]
  InvalidSnapshot,
}
/// Exact private bytes to publish as distinct ordinary Work inputs.
pub struct MappedWorkSource {
  /// Complete task document, including original provenance.
  pub task: Vec<u8>,
  /// Complete explicit acceptance document with the same provenance.
  pub acceptance: Vec<u8>,
}
/// Maps only retained data, without interpreting instructions or fetching linked resources.
pub fn map_work_source_snapshot(
  preparation: &WorkSourcePreparation,
  mapping: &WorkSourceMapping,
) -> Result<MappedWorkSource, SourceMappingFailure> {
  if mapping.source != preparation.input.definition().source || !mapping.valid_limits() {
    return Err(SourceMappingFailure::InvalidMapping);
  }
  if preparation.snapshot.len() > MAX_SOURCE_INPUT_BYTES as usize {
    return Err(SourceMappingFailure::TooLarge);
  }
  let snapshot: WorkSourceSnapshot =
    serde_json::from_slice(&preparation.snapshot).map_err(|_| SourceMappingFailure::InvalidSnapshot)?;
  if snapshot.item != preparation.item
    || snapshot.display_id.trim().is_empty()
    || snapshot.display_id.len() > 256
    || snapshot.update_identity.trim().is_empty()
    || snapshot.update_identity.len() > 256
    || snapshot.state.is_empty()
    || snapshot.state.len() > 128
  {
    return Err(SourceMappingFailure::InvalidSnapshot);
  }
  if snapshot.title.trim().is_empty() {
    return Err(SourceMappingFailure::MissingTitle);
  }
  if mapping.require_body && snapshot.body.as_ref().is_none_or(|body| body.trim().is_empty()) {
    return Err(SourceMappingFailure::MissingBody);
  }
  if snapshot.title.len() > mapping.max_title_bytes as usize
    || snapshot
      .body
      .as_ref()
      .is_some_and(|body| body.len() > mapping.max_body_bytes as usize)
    || snapshot.fields.len() > 32
    || snapshot.labels.len() > 64
    || snapshot
      .labels
      .iter()
      .any(|label| label.is_empty() || label.len() > 256)
    || snapshot
      .fields
      .iter()
      .any(|(name, value)| name.is_empty() || name.len() > 128 || value.len() > mapping.max_body_bytes as usize)
  {
    return Err(SourceMappingFailure::TooLarge);
  }
  let criteria = match &mapping.acceptance {
    SourceAcceptanceMapping::Field(field) => {
      validate_acceptance_field(field)?;
      snapshot.fields.get(field).cloned()
    }
    SourceAcceptanceMapping::Body => snapshot.body.clone(),
    SourceAcceptanceMapping::BodySection(heading) => {
      let level = acceptance_heading_level(heading)?;
      let body = snapshot.body.as_deref().unwrap_or("");
      let mut selected = None;
      let lines = body.lines().collect::<Vec<_>>();
      for (index, _) in lines.iter().enumerate().filter(|(_, line)| line.trim() == heading) {
        if selected.is_some() {
          return Err(SourceMappingFailure::InvalidSnapshot);
        }
        let text = lines[index + 1..]
          .iter()
          .take_while(|line| {
            let count = line.bytes().take_while(|v| *v == b'#').count();
            !(count > 0 && count <= level && line.as_bytes().get(count) == Some(&b' '))
          })
          .copied()
          .collect::<Vec<_>>()
          .join("\n");
        selected = Some(text.trim().to_owned());
      }
      selected
    }
  }
  .filter(|text| !text.trim().is_empty())
  .ok_or(SourceMappingFailure::MissingAcceptance)?;
  let provenance = serde_json::json!({"installation":preparation.input.definition().source.installation,"input_id":preparation.input.id(),"input_version":preparation.input.version(),"mapping":mapping.source.configuration,"item":preparation.item,"display_id":snapshot.display_id,"update_identity":snapshot.update_identity,"labels":snapshot.labels,"state":snapshot.state,"configuration":preparation.input.definition().target,"repository":preparation.input.definition().repository,"root_flow":preparation.input.root_flow(),"selection":preparation.target,"snapshot_digest":octacity_server_factory::FactoryDigest::content_sha256(&preparation.snapshot)});
  let task =
    serde_json::to_vec(&serde_json::json!({"title":snapshot.title,"body":snapshot.body,"provenance":provenance}))
      .map_err(|_| SourceMappingFailure::InvalidSnapshot)?;
  let acceptance = serde_json::to_vec(&serde_json::json!({"criteria":criteria,"provenance":provenance}))
    .map_err(|_| SourceMappingFailure::InvalidSnapshot)?;
  if task.len() > mapping.max_artifact_bytes as usize || acceptance.len() > mapping.max_artifact_bytes as usize {
    return Err(SourceMappingFailure::TooLarge);
  }
  Ok(MappedWorkSource { task, acceptance })
}
