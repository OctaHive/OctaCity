//! Explicit snapshot proof uses the existing retained-command observation reader.
use crate::*;
use std::sync::Arc;

/// Independent Artifact authority and trusted time shared by snapshot preparation and admission.
pub struct IssueSnapshotObservations {
  reader: Arc<dyn IssueClassificationEvidenceReader>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl IssueSnapshotObservations {
  /// Installs existing command qualification, without granting transcript or Issue write access.
  pub fn new(reader: Arc<dyn IssueClassificationEvidenceReader>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self { reader, clock }
  }
}

use octacity_server_factory::*;
use octacity_server_store::*;

impl IssueSnapshotObservations {
  async fn qualifies(
    &self,
    context: &ManagementRequestContext,
    selection: &FlowIssueEvidenceSelection,
    references: &[IssueClassificationEvidenceReference],
  ) -> bool {
    let Some(expected) = &selection.observation else {
      return false;
    };
    if references.is_empty() || references.len() > usize::from(selection.maximum) {
      return false;
    }
    let Ok(now) = self.clock.now() else {
      return false;
    };
    for reference in references {
      if now < reference.observed_at || now >= reference.fresh_until {
        return false;
      }
      let Ok(Some(proof)) = self.reader.read(context, reference, now).await else {
        return false;
      };
      if !proof.matches_observation(reference, expected) {
        return false;
      }
    }
    true
  }
  /// Rechecks the original selected observations immediately before admission; never selects replacements.
  pub(crate) async fn require_available(
    &self,
    context: &ManagementRequestContext,
    snapshot: &IssueExecutionSnapshot,
  ) -> Result<(), ApplicationError> {
    for (alias, references) in &snapshot.evidence {
      let selection = snapshot
        .input
        .evidence
        .get(alias)
        .ok_or_else(ApplicationError::invalid)?;
      if !self.qualifies(context, selection, references).await {
        return Err(unavailable(alias));
      }
    }
    Ok(())
  }
}

pub(crate) async fn freeze(
  readers: Option<&IssueSnapshotObservations>,
  context: &ManagementRequestContext,
  issue: &IssueRevision,
  snapshot: &mut IssueExecutionSnapshot,
) -> Result<(), ApplicationError> {
  for (alias, selection) in &snapshot.input.evidence {
    let references = snapshot
      .attribution
      .get(&selection.field)
      .map(|field| {
        field
          .evidence
          .iter()
          .filter_map(|digest| {
            issue
              .classification
              .evidence
              .iter()
              .find(|reference| &reference.record == digest)
              .cloned()
          })
          .collect::<Vec<_>>()
      })
      .unwrap_or_default();
    let mut qualified = if let Some(readers) = readers {
      readers.qualifies(context, selection, &references).await
    } else {
      false
    };
    if let octacity_server_domain::RepositorySourceSelection::ExactRevision(revision) = &snapshot.target.source {
      let subject = FactoryTaskSubject::Exact(ExactSubject::new(
        snapshot.project_id,
        snapshot.target.repository_id,
        revision.clone(),
      ));
      qualified &= references.iter().all(|reference| reference.subject == subject);
    }
    if !qualified {
      if selection.required {
        return Err(unavailable(alias));
      }
      snapshot
        .omissions
        .insert(alias.clone(), IssueSnapshotOmission::Unavailable);
      continue;
    }
    snapshot.values.insert(
      alias.clone(),
      serde_json::json!(references.iter().map(|reference| reference.record).collect::<Vec<_>>()),
    );
    snapshot.evidence.insert(alias.clone(), references);
  }
  Ok(())
}

pub(crate) async fn require_available(
  readers: Option<&IssueSnapshotObservations>,
  context: &ManagementRequestContext,
  snapshot: &IssueExecutionSnapshot,
) -> Result<(), ApplicationError> {
  if snapshot.evidence.is_empty() {
    return Ok(());
  }
  let readers = readers.ok_or_else(|| unavailable(snapshot.evidence.keys().next().expect("nonempty evidence")))?;
  readers.require_available(context, snapshot).await
}
fn unavailable(alias: &FactoryKey) -> ApplicationError {
  FactoryValidationDiagnostics::unavailable_with_code(
    format!("/issue_snapshot/evidence/{alias}"),
    "issue_snapshot_evidence_unavailable",
  )
}
