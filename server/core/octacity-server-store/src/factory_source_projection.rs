//! Durable technical ordering facts; accepted Flow/outbox history remains the authority.
use crate::{FactoryOutboxRecord, FactoryOutboxState, FactoryRunSnapshot, StoreError, source_conflict};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryClaim, FactoryDigest, FlowActionBinding, FlowNodeDefinition, FlowNodeKind, ImmutableReference, NodeAttempt,
  NodeExecutionIdentity, WorkEnvelopeId,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One scoped, fenced coordination request. Native field semantics stay inside the adapter.
#[derive(Clone, Debug)]
pub struct SourceProjectionRequest {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Original source-associated Work.
  pub work_id: WorkEnvelopeId,
  /// Original accepted status operation.
  pub operation_id: FactoryDigest,
  /// Independent outbox claim, never the Flow owner claim.
  pub claim: FactoryClaim,
  /// Trusted observer time.
  pub observed_at: Timestamp,
  /// Immutable opaque managed-field scope returned by the pinned adapter.
  pub fields: Vec<FactoryDigest>,
  /// Technical coordination step.
  pub action: SourceProjectionAction,
}
/// No operation here creates Work or changes a Flow transition.
#[derive(Clone, Debug)]
pub enum SourceProjectionAction {
  /// Pin field identities and inspect the accepted order.
  Register,
  /// Fence a read-only native observation against intervening technical writes.
  Observe {
    /// Revision captured before the native read, absent when opening the read window.
    revision: Option<FactoryDigest>,
  },
  /// Check current ownership before admitting exactly one native write.
  Begin {
    /// Content identity of the conditional provider step.
    step_id: FactoryDigest,
    /// All managed fields that may be affected by that write.
    fields: Vec<FactoryDigest>,
  },
  /// Record whether the original write completed or can still land late.
  Finish {
    /// Original conditional step identity.
    step_id: FactoryDigest,
    /// Unknown results retain their hazard; expiry without known completion is a conflict.
    uncertain: bool,
  },
}
/// Classified technical progress; no provider diagnostics or lifecycle outcomes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceProjectionState {
  /// Opaque fence around a native read; only stable, quiescent reads can confirm delivery.
  Observation {
    /// Durable technical generation, independent of Work/Flow versions.
    revision: FactoryDigest,
    /// Whether the supplied pre-read revision still matches.
    stable: bool,
    /// Fields still owned by this accepted operation.
    fields: Vec<FactoryDigest>,
    /// Earlier overlapping writes have known completion.
    quiescent: bool,
  },
  /// Fields still owned by this accepted intent and whether earlier writes have drained.
  Ready {
    /// Remaining opaque managed fields.
    fields: Vec<FactoryDigest>,
    /// No unresolved older overlapping write remains.
    quiescent: bool,
  },
  /// Exactly one conditional write may start under this permit.
  Granted {
    /// Durable identity for this operation/claim/step combination.
    permit: FactoryDigest,
  },
  /// A newer accepted overlapping intent owns these fields.
  Superseded,
  /// Newer accepted intent metadata is not yet available; no write may start.
  Blocked,
  /// The original permit was completed without granting another write.
  Released,
}
/// Minimal ordering evidence derived from authoritative accepted status records.
#[derive(Clone, Debug)]
pub struct SourceProjectionCandidate {
  /// Original accepted operation.
  pub operation_id: FactoryDigest,
  /// Original source-associated Work.
  pub work_id: WorkEnvelopeId,
  /// Immutable enqueue time for provenance; ordering uses the store-assigned sequence.
  pub accepted_at: Timestamp,
  /// Exact accepted action capability; unrelated status adapters do not share this source order.
  pub capability: ImmutableReference,
  /// Original action deadline, never renewed by a claim or retry.
  pub deadline: Timestamp,
  /// Current live outbox claim, absent for pending/terminal rows.
  pub claim: Option<FactoryClaim>,
}
/// Immutable provenance of one configured status action in the admitted Flow closure.
/// This proof carries no live claim or authority to invoke a provider.
pub struct AcceptedFactoryStatusAction<'a> {
  /// Exact accepted attempt, including its original deadline.
  pub attempt: &'a NodeAttempt,
  /// Exact node from the accepted definition, including permissions and budget.
  pub node: &'a FlowNodeDefinition,
  /// Configured action capability and parameters.
  pub binding: &'a FlowActionBinding,
}
/// Validates status-action provenance shared by ordering and outbox dispatch.
/// Nested stages resolve through the record's exact immutable definition.
pub fn accepted_factory_status_action<'a>(
  snapshot: &'a FactoryRunSnapshot,
  row: &FactoryOutboxRecord,
) -> Result<AcceptedFactoryStatusAction<'a>, StoreError> {
  let record = snapshot
    .flow
    .data
    .records
    .iter()
    .find(|r| r.digest().ok() == Some(row.input_digest))
    .ok_or_else(source_conflict)?;
  let attempt = snapshot
    .flow
    .attempts
    .iter()
    .find(|a| a.id() == record.node_attempt_id())
    .ok_or_else(source_conflict)?;
  let node = snapshot
    .admitted_flow
    .closure()
    .definition(record.input().definition())
    .and_then(|d| d.node(record.input().node()))
    .ok_or_else(source_conflict)?;
  let binding = node.action().ok_or_else(source_conflict)?;
  if node.kind() != FlowNodeKind::TrustedAction
    || binding.action().as_str() != "set_status"
    || attempt.execution() != &NodeExecutionIdentity::External(binding.plugin().clone())
    || record.payload().value()["receipt"] != serde_json::json!(row.operation_id)
    || !node.permissions().plugins().any(|p| p == binding.plugin())
  {
    return Err(source_conflict());
  }
  Ok(AcceptedFactoryStatusAction { attempt, node, binding })
}
/// Derives ordering only from accepted configured actions, including already delivered intents.
pub fn source_projection_candidates(
  snapshots: &[FactoryRunSnapshot],
) -> Result<Vec<SourceProjectionCandidate>, StoreError> {
  let mut result = Vec::new();
  for snapshot in snapshots {
    for initial in snapshot.outbox.iter().filter(|r| {
      r.kind.as_str() == "work.status.projection" && r.attempt == 0 && r.state == FactoryOutboxState::Pending
    }) {
      let AcceptedFactoryStatusAction { attempt, binding, .. } = accepted_factory_status_action(snapshot, initial)?;
      let latest = snapshot
        .outbox
        .iter()
        .filter(|r| r.operation_id == initial.operation_id)
        .max_by_key(|r| {
          let rank = match r.state {
            FactoryOutboxState::Pending => 0,
            FactoryOutboxState::Claimed => 1,
            _ => 2,
          };
          (r.attempt, rank, r.recorded_at, r.id)
        })
        .ok_or_else(source_conflict)?;
      result.push(SourceProjectionCandidate {
        operation_id: initial.operation_id,
        work_id: snapshot.work.id(),
        accepted_at: initial.recorded_at,
        capability: binding.plugin().clone(),
        deadline: attempt.deadline(),
        claim: if latest.state == FactoryOutboxState::Claimed {
          latest.claim
        } else {
          None
        },
      });
      if result.len() > crate::MAX_FACTORY_RUN_SNAPSHOT_RECORDS {
        return Err(source_conflict());
      }
    }
  }
  Ok(result)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lease {
  operation: FactoryDigest,
  claim: FactoryClaim,
  fields: Vec<FactoryDigest>,
  deadline: Timestamp,
}
/// Store-assigned acceptance order and immutable field metadata of one original outbox operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceProjectionOrder {
  /// Positive per-source-item sequence assigned atomically with accepted action publication.
  pub sequence: u64,
  /// None until the pinned adapter has described its fields; Some(empty) is an explicit no-op.
  pub fields: Option<Vec<FactoryDigest>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
  operation: FactoryDigest,
  sequence: u64,
  generation: u64,
}
/// Private durable bounded latest-field and hazard journal; no Work/Flow authority lives here.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProjectionJournal {
  heads: BTreeMap<FactoryDigest, Head>,
  leases: BTreeMap<FactoryDigest, Lease>,
}
impl SourceProjectionJournal {
  fn advance(&mut self, fields: &[FactoryDigest]) -> Result<(), StoreError> {
    for field in fields {
      let head = self.heads.get_mut(field).ok_or_else(source_conflict)?;
      head.generation = head.generation.checked_add(1).ok_or_else(source_conflict)?;
    }
    Ok(())
  }
  /// Coordinates one step under the backend's issue lock using freshly read accepted facts.
  /// `newer_unknown` must include every later accepted source intent lacking pinned field metadata.
  pub fn coordinate(
    &mut self,
    request: &SourceProjectionRequest,
    candidates: &[SourceProjectionCandidate],
    order: &mut SourceProjectionOrder,
    newer_unknown: bool,
  ) -> Result<SourceProjectionState, StoreError> {
    let scope = checked_fields(&request.fields, true)?;
    if order.sequence == 0 || order.fields.as_ref().is_some_and(|original| original != &scope) {
      return Err(source_conflict());
    }
    let candidate = candidates
      .iter()
      .find(|c| c.operation_id == request.operation_id && c.work_id == request.work_id)
      .ok_or_else(source_conflict)?;
    let permit = |step: FactoryDigest| {
      FactoryDigest::sha256(
        "octacity.factory.source-projection-permit.v1",
        &[
          &request.operation_id.as_bytes(),
          &request.claim.fence().digest().as_bytes(),
          &step.as_bytes(),
        ],
      )
    };
    if let SourceProjectionAction::Finish { step_id, uncertain } = request.action {
      let id = permit(step_id);
      if let Some(lease) = self.leases.get(&id) {
        if lease.operation != request.operation_id
          || lease.claim != request.claim
          || order.fields.as_ref() != Some(&scope)
        {
          return Err(source_conflict());
        }
        if !uncertain {
          let fields = lease.fields.clone();
          self.leases.remove(&id);
          self.advance(&fields)?;
        }
      }
      return Ok(SourceProjectionState::Released);
    }
    if candidate.claim != Some(request.claim)
      || request.observed_at >= candidate.deadline
      || request
        .claim
        .verify_fence(request.claim.fence(), request.observed_at)
        .is_err()
    {
      return Err(source_conflict());
    }
    if order.fields.is_none() {
      if self.heads.len() + scope.iter().filter(|field| !self.heads.contains_key(field)).count() > 4096 {
        return Err(source_conflict());
      }
      order.fields = Some(scope.clone());
      for field in &scope {
        if self.heads.get(field).is_none_or(|head| head.sequence < order.sequence) {
          let generation = self
            .heads
            .get(field)
            .map_or(Some(1), |head| head.generation.checked_add(1))
            .ok_or_else(source_conflict)?;
          self.heads.insert(
            *field,
            Head {
              operation: request.operation_id,
              sequence: order.sequence,
              generation,
            },
          );
        }
      }
    }
    if newer_unknown {
      return Ok(SourceProjectionState::Blocked);
    }
    let owned: Vec<_> = scope
      .iter()
      .copied()
      .filter(|field| {
        self
          .heads
          .get(field)
          .is_some_and(|head| head.operation == request.operation_id && head.sequence == order.sequence)
      })
      .collect();
    if !scope.is_empty() && owned.is_empty() {
      return Ok(SourceProjectionState::Superseded);
    }
    // A client deadline cannot fence a request already accepted remotely.
    // Keep unresolved hazards; after their original window, fail explicitly
    // instead of certifying a false quiet period. Known late completion can
    // still clear its own permit through Finish.
    if self.leases.values().any(|lease| {
      lease.operation != request.operation_id
        && lease.deadline <= request.observed_at
        && lease.fields.iter().any(|field| owned.contains(field))
    }) {
      return Err(source_conflict());
    }
    let quiescent = !self
      .leases
      .values()
      .any(|lease| lease.operation != request.operation_id && lease.fields.iter().any(|field| owned.contains(field)));
    match &request.action {
      SourceProjectionAction::Observe { revision } => {
        let view = owned
          .iter()
          .map(|field| {
            self
              .heads
              .get(field)
              .map(|head| (field, head.operation, head.sequence, head.generation))
              .ok_or_else(source_conflict)
          })
          .collect::<Result<Vec<_>, _>>()?;
        let bytes = serde_json::to_vec(&view).map_err(|_| StoreError::Unavailable)?;
        let current = FactoryDigest::sha256("octacity.factory.source-projection-observation.v1", &[&bytes]);
        Ok(SourceProjectionState::Observation {
          revision: current,
          stable: *revision == Some(current),
          fields: owned,
          quiescent,
        })
      }
      SourceProjectionAction::Register => Ok(SourceProjectionState::Ready {
        fields: owned,
        quiescent,
      }),
      SourceProjectionAction::Begin { step_id, fields } => {
        let step_fields = checked_fields(fields, false)?;
        if step_fields.iter().any(|f| !scope.contains(f)) || self.leases.len() >= 128 {
          return Err(source_conflict());
        }
        if step_fields.iter().any(|f| !owned.contains(f)) {
          return Ok(SourceProjectionState::Superseded);
        }
        let id = permit(*step_id);
        if self.leases.contains_key(&id) {
          return Err(source_conflict());
        }
        self.advance(&step_fields)?;
        self.leases.insert(
          id,
          Lease {
            operation: request.operation_id,
            claim: request.claim,
            fields: step_fields,
            deadline: candidate.deadline,
          },
        );
        Ok(SourceProjectionState::Granted { permit: id })
      }
      SourceProjectionAction::Finish { .. } => unreachable!(),
    }
  }
}
fn checked_fields(fields: &[FactoryDigest], empty: bool) -> Result<Vec<FactoryDigest>, StoreError> {
  let set = fields.iter().copied().collect::<BTreeSet<_>>();
  if (!empty && fields.is_empty()) || fields.len() > if empty { 512 } else { 256 } || set.len() != fields.len() {
    return Err(source_conflict());
  }
  Ok(set.into_iter().collect())
}
