//! Resolves only approved exact sources and freezes their bytes in the ordinary Incoming payload.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::*;

pub(crate) async fn freeze(
  reader: Option<&IssueSnapshotMaterialReaders>,
  context: &ManagementRequestContext,
  issue: &IssueRevision,
  snapshot: &mut IssueExecutionSnapshot,
) -> Result<(), ApplicationError> {
  for (alias, contract) in &snapshot.input.materials {
    let result = load(reader, context, issue, contract).await;
    match result {
      Ok((texts, materials)) => {
        snapshot.values.insert(alias.clone(), serde_json::json!(texts));
        snapshot.materials.insert(alias.clone(), materials);
      }
      Err(MaterialFailure::Limit) => {
        return Err(FactoryValidationDiagnostics::unavailable_with_code(
          format!("/issue_snapshot/materials/{alias}"),
          "issue_snapshot_material_limit_exceeded",
        ));
      }
      Err(_) if !contract.required => {
        snapshot
          .omissions
          .insert(alias.clone(), IssueSnapshotOmission::Unavailable);
      }
      Err(_) => {
        return Err(FactoryValidationDiagnostics::unavailable_with_code(
          format!("/issue_snapshot/materials/{alias}"),
          "issue_snapshot_material_unavailable",
        ));
      }
    }
  }
  Ok(())
}
// Unavailable optional content can be omitted; exceeding a reviewed bound cannot.
enum MaterialFailure {
  Unavailable,
  Limit,
}
impl From<ApplicationError> for MaterialFailure {
  fn from(_: ApplicationError) -> Self {
    Self::Unavailable
  }
}

async fn load(
  reader: Option<&IssueSnapshotMaterialReaders>,
  context: &ManagementRequestContext,
  issue: &IssueRevision,
  contract: &FlowIssueMaterialSelection,
) -> Result<(Vec<String>, Vec<IssueSnapshotMaterial>), MaterialFailure> {
  let reader = reader.ok_or(MaterialFailure::Unavailable)?;
  let selections = match &contract.source {
    FlowIssueMaterialSource::Exact { references } => references
      .iter()
      .map(|source| {
        let reference = match source {
          FlowIssueMaterialReference::Comment {
            comment_id,
            body_digest,
          } => IssueMaterialReference::Comment {
            project_id: issue.project_id,
            issue_id: issue.id,
            comment_id: *comment_id,
            body_digest: *body_digest,
          },
          FlowIssueMaterialReference::IssueDocument { artifact } => IssueMaterialReference::IssueDocument {
            project_id: issue.project_id,
            issue_id: issue.id,
            artifact: artifact.clone(),
          },
          FlowIssueMaterialReference::ProjectDocument { artifact } => IssueMaterialReference::ProjectDocument {
            project_id: issue.project_id,
            artifact: artifact.clone(),
          },
        };
        issue
          .materials
          .entries()
          .iter()
          .find(|s| s.reference == reference)
          .cloned()
          .ok_or(MaterialFailure::Unavailable)
      })
      .collect::<Result<Vec<_>, _>>()?,
    FlowIssueMaterialSource::Selected { max_entries } => {
      let entries = issue.materials.entries();
      if entries.len() > usize::from(*max_entries) {
        return Err(MaterialFailure::Limit);
      }
      entries.to_vec()
    }
  };
  if selections.is_empty() {
    return Err(MaterialFailure::Unavailable);
  }
  if selections.len() > MAX_ISSUE_MATERIALS {
    return Err(MaterialFailure::Limit);
  }
  let mut texts = Vec::new();
  let mut materials = Vec::new();
  let mut bytes = 0usize;
  for selection in selections {
    let owns = match &selection.reference {
      IssueMaterialReference::Comment {
        project_id, issue_id, ..
      }
      | IssueMaterialReference::IssueDocument {
        project_id, issue_id, ..
      } => *project_id == issue.project_id && *issue_id == issue.id,
      IssueMaterialReference::ProjectDocument { project_id, .. } => *project_id == issue.project_id,
    };
    if !owns {
      return Err(MaterialFailure::Unavailable);
    }
    let (text, source) = reader.read(context, &selection.reference).await?;
    bytes = bytes.checked_add(text.len()).ok_or(MaterialFailure::Limit)?;
    if bytes > contract.max_bytes {
      return Err(MaterialFailure::Limit);
    }
    texts.push(text);
    materials.push(IssueSnapshotMaterial { selection, source });
  }
  Ok((texts, materials))
}

/// Before material readers existed every optional material retained `Unavailable`.
/// Restore that earlier projection only when its complete, already reviewed hash matches.
/// Required materials never qualify for this compatibility path.
pub(crate) fn restore_reviewed_omissions(snapshot: &mut IssueExecutionSnapshot, expected: FactoryDigest) -> bool {
  if snapshot.materials.is_empty() || snapshot.input.materials.values().any(|s| s.required) {
    return false;
  }
  let mut legacy = snapshot.clone();
  for alias in snapshot.input.materials.keys() {
    legacy.values.remove(alias);
    legacy.materials.remove(alias);
    legacy
      .omissions
      .insert(alias.clone(), IssueSnapshotOmission::Unavailable);
  }
  if !legacy.digest().is_ok_and(|digest| digest == expected) {
    return false;
  }
  *snapshot = legacy;
  true
}

/// Pending recovery keeps frozen values, but does not inherit an old body-read grant.
/// Check the same exact sources immediately before admission; never rebuild selections
/// or replace an included optional value with an omission at this boundary.
pub(crate) async fn require_available(
  reader: Option<&IssueSnapshotMaterialReaders>,
  context: &ManagementRequestContext,
  snapshot: &IssueExecutionSnapshot,
) -> Result<(), ApplicationError> {
  for (alias, entries) in &snapshot.materials {
    let unavailable = || {
      FactoryValidationDiagnostics::unavailable_with_code(
        format!("/issue_snapshot/materials/{alias}"),
        "issue_snapshot_material_unavailable",
      )
    };
    let reader = reader.ok_or_else(unavailable)?;
    let texts = snapshot
      .values
      .get(alias)
      .and_then(serde_json::Value::as_array)
      .filter(|values| values.len() == entries.len())
      .ok_or_else(unavailable)?;
    for (entry, expected) in entries.iter().zip(texts) {
      let (text, source) = reader
        .read(context, &entry.selection.reference)
        .await
        .map_err(|_| unavailable())?;
      if expected.as_str() != Some(text.as_str()) || source != entry.source {
        return Err(unavailable());
      }
    }
  }
  Ok(())
}
