//! Source observations select declared direct edges through the existing Issue policy.
use crate::*;
use octacity_server_factory::{FactoryDigest, ImmutableReference};
use serde::{Deserialize, Serialize};

/// Private normalized native data; no intrinsic status/provider meaning or diagnostic body exposure.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceStatusObservation {
  /// Opaque adapter-normalized state.
  pub state: String,
  /// Literal observed labels, including unmanaged values.
  pub labels: Vec<String>,
}
/// Exact original mapping and safe observation references for status history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueSourceStatusProvenance {
  /// Immutable link mapping, separately versioned from the workflow.
  pub mapping: ImmutableReference,
  /// Original bounded receipt identity; no task text enters history.
  pub observation: FactoryDigest,
  /// Original Project/security/native association.
  pub native_correlation: FactoryDigest,
}
/// Explicit translation result; never a Run result or execution permission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSourceStatusCondition {
  /// Exactly one configured direct move committed.
  Applied,
  /// The declared target already matches the Issue.
  Echo,
  /// Native data matches no configured target.
  Unmapped,
  /// Native data matches several distinct internal targets.
  Ambiguous,
  /// The exact current workflow does not match the mapping.
  WorkflowMismatch,
  /// There is no allowed direct Source edge.
  ForbiddenTransition,
  /// The direct edge requires independently qualified execution evidence.
  EvidenceRequired,
  /// Observation freshness does not authorize a new move.
  NotFresh,
  /// Local status owns the field; native divergence is data only.
  Unmanaged,
}
/// Safe retained status condition, without native values or task bodies.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueSourceStatusOutcome {
  /// Original translation authority.
  pub provenance: IssueSourceStatusProvenance,
  /// Explicit condition rather than a guessed move.
  pub condition: IssueSourceStatusCondition,
}
/// Existing authoritative mapping and workflow resolved by either transaction adapter.
pub struct IssueSourceMappingContext<'a> {
  /// Original accepted Project/security/native key from the retained link.
  pub native_correlation: FactoryDigest,
  /// Exact current link policy.
  pub mapping: &'a PublishedIssueSourceMapping,
  /// Retained business workflow, independent of native values.
  pub workflow: &'a PublishedIssueWorkflow,
}
/// One atomic ordinary revision/history publication plus safe observation metadata.
pub struct IssueSourceRefreshPlan {
  /// Original refresh result to retain in existing replay.
  pub outcome: IssueSourceRefreshOutcome,
  /// Optional next immutable Issue revision.
  pub revision: Option<IssueRevision>,
  /// Optional status event committed with that same revision.
  pub transition: Option<IssueStatusHistoryEntry>,
}
impl IssueSourceRefreshState {
  /// Shared freshness, ownership and business-transition planning; adapters own atomic publication.
  pub fn plan(
    &mut self,
    current: &IssueRevision,
    request: &RefreshIssueSource,
    policy: &IssueFieldPolicy,
    mapping: Option<IssueSourceMappingContext<'_>>,
    actor: &AuditActor,
  ) -> Result<IssueSourceRefreshPlan, StoreError> {
    if mapping
      .as_ref()
      .is_some_and(|m| m.mapping.definition.source != request.selection.source)
    {
      return Ok(IssueSourceRefreshPlan {
        outcome: IssueSourceRefreshOutcome {
          issue_id: current.id,
          disposition: IssueSourceRefreshDisposition::ReadProfileMismatch,
          status: None,
        },
        revision: None,
        transition: None,
      });
    }
    let disposition = self.observe(&request.observation);
    let mapping_changed = mapping
      .as_ref()
      .is_some_and(|m| self.mapping.as_ref() != Some(&m.mapping.reference()));
    let applicable = (disposition == IssueSourceRefreshDisposition::Fresh
      || (disposition == IssueSourceRefreshDisposition::Unchanged && mapping_changed))
      && mapping
        .as_ref()
        .is_none_or(|m| request.observation.observed_at > m.mapping.published_at);
    let mut revision = if applicable {
      current.refreshed_source(request, policy)?
    } else {
      None
    };
    let mut transition = None;
    let mut status = None;
    if let Some(context) = mapping {
      if context.mapping.definition.status.is_some() {
        let (result, moved) = context.source_move(current, request, policy, applicable, actor)?;
        if let Some(moved) = moved {
          let mut next = moved.issue;
          if let Some(text) = revision.take() {
            next.content = text.content;
          }
          revision = Some(next);
          transition = Some(moved.transition);
        }
        status = Some(result);
      }
      if applicable {
        self.mapping = Some(context.mapping.reference());
      }
      self.status = status.clone();
    }
    Ok(IssueSourceRefreshPlan {
      outcome: IssueSourceRefreshOutcome {
        issue_id: current.id,
        disposition,
        status,
      },
      revision,
      transition,
    })
  }
}
impl IssueSourceMappingContext<'_> {
  fn source_move(
    &self,
    current: &IssueRevision,
    request: &RefreshIssueSource,
    policy: &IssueFieldPolicy,
    applicable: bool,
    actor: &AuditActor,
  ) -> Result<(IssueSourceStatusOutcome, Option<IssueTransitionOutcome>), StoreError> {
    use IssueSourceStatusCondition::*;
    let status = self
      .mapping
      .definition
      .status
      .as_ref()
      .expect("selected status mapping");
    let provenance = IssueSourceStatusProvenance {
      mapping: self.mapping.reference(),
      observation: request.observation.id(self.native_correlation)?,
      native_correlation: self.native_correlation,
    };
    let blocked = |condition| {
      Ok((
        IssueSourceStatusOutcome {
          provenance: provenance.clone(),
          condition,
        },
        None,
      ))
    };
    if !policy.source_owned.contains(&IssueField::Status) {
      return blocked(Unmanaged);
    }
    if status.workflow
      != (IssueWorkflowPin {
        id: current.workflow_id,
        version: current.workflow_version,
      })
      || self.workflow.id != current.workflow_id
      || self.workflow.version != current.workflow_version
    {
      return blocked(WorkflowMismatch);
    }
    let Some(facts) = &request.status else {
      return blocked(Unmapped);
    };
    let targets: std::collections::BTreeSet<_> = status
      .pull
      .iter()
      .filter(|r| r.native.mismatches(&facts.state, &facts.labels).is_empty())
      .map(|r| r.status_id)
      .collect();
    if targets.is_empty() {
      return blocked(Unmapped);
    }
    if targets.len() != 1 {
      return blocked(Ambiguous);
    }
    let target = *targets.first().expect("one target");
    if target == current.status_id {
      return blocked(Echo);
    }
    if !applicable {
      return blocked(NotFresh);
    }
    let Some(edge) = self
      .workflow
      .definition
      .transitions
      .iter()
      .find(|e| e.from == current.status_id && e.to == target && e.origins.contains(&IssueTransitionOrigin::Source))
    else {
      return blocked(ForbiddenTransition);
    };
    if edge
      .move_block_reason(policy, IssueTransitionOrigin::Source, |_| false)
      .is_some()
    {
      return blocked(EvidenceRequired);
    }
    let mut command = TransitionIssue::manual(
      TransitionIssueIntent {
        project_id: current.project_id,
        id: current.id,
        expected_version: current.version,
        workflow_id: current.workflow_id,
        workflow_version: current.workflow_version,
        expected_status_id: current.status_id,
        transition_id: edge.id,
        reason: None,
      },
      request.idempotency_key.clone(),
      request.refreshed_at,
    );
    command.source = Some(provenance.clone());
    Ok((
      IssueSourceStatusOutcome {
        provenance,
        condition: Applied,
      },
      Some(current.transitioned(&command, self.workflow, policy, actor)?),
    ))
  }
}
