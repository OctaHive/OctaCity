use octacity_server_factory::{
  BudgetLimit, ContextManifest, ContextManifestEntry, ContextManifestId, ContextSourceKind, FactoryContextReference,
  FactoryDigest, FactoryKey, FactoryTaskSubject, MAX_MACRO_CALL_DEPTH, MacroCall, MacroCallDeclaration, MacroCallId,
  MacroCallKind, StageAttemptId, StageHandoffId,
};
use octacity_server_store::FactoryRunSnapshot;
use thiserror::Error;
use uuid::Uuid;

const CONTEXT_MANIFEST_NAMESPACE: Uuid = Uuid::from_u128(0xd732_0630_7111_5b2f_92d9_8f95_69c9_18a1);
const CONFIGURATION_POLICY_IDENTITY: &str = "factory-configuration";
const REPOSITORY_RANGE_IDENTITY: &str = "repository-range";

/// One explicitly selected immutable input and the declared dependency that authorizes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryContextSelection {
  /// Server-owned task, specification, evidence, policy, or exact repository input.
  Required(ContextManifestEntry),
  /// Typed handoff from one declared predecessor stage.
  StageHandoff {
    /// Persisted handoff selected by the successor.
    handoff_id: StageHandoffId,
    /// Exact retained bytes included in the manifest.
    entry: ContextManifestEntry,
  },
  /// Typed result or bounded summary from one explicitly declared predecessor call.
  CallOutput {
    /// Persisted predecessor call whose output was selected.
    call_id: MacroCallId,
    /// Exact retained bytes included in the manifest.
    entry: ContextManifestEntry,
  },
}

/// Immutable declaration used to construct or recover one macro call input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareFactoryCallContext {
  /// Stable logical call identity reused by transport retries.
  pub call_id: MacroCallId,
  /// Stage Attempt that owns the call.
  pub stage_attempt_id: StageAttemptId,
  /// Exact base or candidate subject visible to the call.
  pub subject: FactoryTaskSubject,
  /// Program-owned call purpose.
  pub kind: MacroCallKind,
  /// Optional visual parent in the durable DAG.
  pub parent_call_id: Option<MacroCallId>,
  /// Predecessor stages whose persisted handoffs may be selected.
  pub stage_dependencies: Vec<StageAttemptId>,
  /// Predecessor calls whose typed outputs may be selected.
  pub call_dependencies: Vec<MacroCallId>,
  /// Exact deterministic construction policy.
  pub construction_policy_digest: FactoryDigest,
  /// Immutable per-call hard budget.
  pub budget: BudgetLimit,
  /// Required and explicitly selected bounded inputs.
  pub selections: Vec<FactoryContextSelection>,
}

/// A call input prepared from durable Factory state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedFactoryCallContext {
  /// Frozen canonical manifest.
  pub manifest: ContextManifest,
  /// Durable macro call node bound to that manifest.
  pub call: MacroCall,
  /// True when a retry recovered the already-persisted manifest and node.
  pub reused: bool,
}

/// Stable failure while preparing bounded call context.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FactoryContextError {
  /// The requested owning stage is absent or belongs to another exact subject.
  #[error("factory call stage is unavailable")]
  StageUnavailable,
  /// A declared predecessor is absent, cross-subject, or otherwise inconsistent.
  #[error("factory call dependency is invalid")]
  InvalidDependency,
  /// A selected input was not authorized by its declared dependency.
  #[error("factory context selection is not declared")]
  UndeclaredSelection,
  /// The call graph exceeds its fixed depth bound.
  #[error("factory call depth exceeds the supported bound")]
  DepthExceeded,
  /// Persisted retry state conflicts with the stable logical call identity.
  #[error("factory call identity was reused for different immutable intent")]
  ReplayMismatch,
  /// A bounded domain contract rejected the constructed manifest or node.
  #[error("factory call context is invalid")]
  InvalidContext,
}

/// Constructs a new frozen input or recovers the prior one for the same logical call.
///
/// Only persisted declared predecessors can authorize stage handoffs or call outputs.
/// Full provider transcripts have no selectable source kind and therefore cannot
/// become implicit sibling or evaluator context.
pub fn prepare_factory_call_context(
  snapshot: &FactoryRunSnapshot,
  mut request: PrepareFactoryCallContext,
) -> Result<PreparedFactoryCallContext, FactoryContextError> {
  let stage = snapshot
    .stage_attempts
    .iter()
    .find(|stage| stage.id() == request.stage_attempt_id)
    .filter(|stage| stage.run_id() == snapshot.run.id() && request.subject.exact() == stage.subject())
    .ok_or(FactoryContextError::StageUnavailable)?;

  request.stage_dependencies.sort_unstable();
  request.stage_dependencies.dedup();
  request.call_dependencies.sort_unstable();
  request.call_dependencies.dedup();

  if let Some(existing) = snapshot.macro_calls.iter().find(|call| call.id() == request.call_id) {
    let manifest = snapshot
      .context_manifests
      .iter()
      .find(|manifest| manifest.id() == existing.context_manifest_id())
      .filter(|manifest| manifest.digest().ok() == Some(existing.context_digest()))
      .ok_or(FactoryContextError::ReplayMismatch)?;
    if existing.stage_attempt_id() != stage.id()
      || existing.subject() != &request.subject
      || existing.kind() != request.kind
      || existing.parent_id() != request.parent_call_id
      || existing.stage_dependencies() != request.stage_dependencies
      || existing.call_dependencies() != request.call_dependencies
      || existing.budget() != request.budget
      || manifest.construction_policy_digest() != request.construction_policy_digest
    {
      return Err(FactoryContextError::ReplayMismatch);
    }
    return Ok(PreparedFactoryCallContext {
      manifest: manifest.clone(),
      call: existing.clone(),
      reused: true,
    });
  }

  let declared_stages = request
    .stage_dependencies
    .iter()
    .map(|id| {
      let dependency = snapshot
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == *id)
        .filter(|stage| stage.run_id() == snapshot.run.id() && stage.subject() == request.subject.exact())
        .ok_or(FactoryContextError::InvalidDependency)?;
      let handoff = snapshot
        .stage_handoffs
        .iter()
        .find(|handoff| handoff.stage_attempt_id() == dependency.id())
        .filter(|handoff| handoff.subject() == &request.subject)
        .ok_or(FactoryContextError::InvalidDependency)?;
      Ok((dependency.id(), handoff))
    })
    .collect::<Result<Vec<_>, _>>()?;

  let declared_calls = request
    .call_dependencies
    .iter()
    .map(|id| {
      snapshot
        .macro_calls
        .iter()
        .find(|call| call.id() == *id)
        .filter(|call| call.run_id() == snapshot.run.id() && call.subject() == &request.subject)
        .ok_or(FactoryContextError::InvalidDependency)
    })
    .collect::<Result<Vec<_>, _>>()?;
  let parent = request
    .parent_call_id
    .map(|id| {
      declared_calls
        .iter()
        .copied()
        .find(|call| call.id() == id)
        .ok_or(FactoryContextError::InvalidDependency)
    })
    .transpose()?;
  let depth = declared_calls
    .iter()
    .map(|call| call.depth())
    .max()
    .unwrap_or(0)
    .checked_add(1)
    .filter(|depth| *depth <= MAX_MACRO_CALL_DEPTH)
    .ok_or(FactoryContextError::DepthExceeded)?;

  let entries = request
    .selections
    .into_iter()
    .map(|selection| validate_selection(snapshot, selection, &declared_stages, &declared_calls))
    .collect::<Result<Vec<_>, _>>()?;
  let manifest_id = ContextManifestId::from_uuid(Uuid::new_v5(
    &CONTEXT_MANIFEST_NAMESPACE,
    request.call_id.as_uuid().as_bytes(),
  ))
  .map_err(|_| FactoryContextError::InvalidContext)?;
  let manifest = ContextManifest::new(
    manifest_id,
    request.subject.clone(),
    request.construction_policy_digest,
    entries,
  )
  .map_err(|_| FactoryContextError::InvalidContext)?;
  let call = MacroCall::new(
    MacroCallDeclaration::new(
      request.call_id,
      request.subject,
      request.kind,
      request.budget,
      request.stage_dependencies,
      request.call_dependencies,
      depth,
    )
    .map_err(|_| FactoryContextError::InvalidContext)?,
    stage,
    &manifest,
    parent,
  )
  .map_err(|_| FactoryContextError::InvalidContext)?;
  Ok(PreparedFactoryCallContext {
    manifest,
    call,
    reused: false,
  })
}

fn validate_selection(
  snapshot: &FactoryRunSnapshot,
  selection: FactoryContextSelection,
  stages: &[(StageAttemptId, &octacity_server_factory::StageHandoff)],
  calls: &[&MacroCall],
) -> Result<ContextManifestEntry, FactoryContextError> {
  match selection {
    FactoryContextSelection::Required(entry) => match entry.source_kind() {
      ContextSourceKind::Task => validate_work_artifact(snapshot, entry, |reference| {
        let artifacts = snapshot.work.artifacts();
        reference == artifacts.task() || reference == artifacts.acceptance()
      }),
      ContextSourceKind::Specification => validate_work_artifact(snapshot, entry, |reference| {
        snapshot.work.artifacts().specifications().contains(reference)
      }),
      ContextSourceKind::Evidence => validate_evidence(snapshot, entry),
      ContextSourceKind::Policy => validate_configuration_policy(snapshot, entry),
      ContextSourceKind::RepositoryRange => validate_repository_range(snapshot, entry),
      ContextSourceKind::StageHandoff
      | ContextSourceKind::TypedResult
      | ContextSourceKind::BoundedSummary
      | ContextSourceKind::RepositoryFragment => Err(FactoryContextError::UndeclaredSelection),
    },
    FactoryContextSelection::StageHandoff { handoff_id, entry } => {
      let handoff = stages
        .iter()
        .map(|(_, handoff)| *handoff)
        .find(|handoff| handoff.id() == handoff_id)
        .ok_or(FactoryContextError::UndeclaredSelection)?;
      let bytes = handoff
        .canonical_bytes()
        .map_err(|_| FactoryContextError::InvalidContext)?;
      let identity = FactoryKey::new(handoff_id.to_string()).map_err(|_| FactoryContextError::InvalidContext)?;
      if entry.source_kind() != ContextSourceKind::StageHandoff
        || entry.logical_identity() != &identity
        || entry.content_digest() != FactoryDigest::content_sha256(&bytes)
        || entry.encoded_size() != u64::try_from(bytes.len()).map_err(|_| FactoryContextError::InvalidContext)?
      {
        return Err(FactoryContextError::UndeclaredSelection);
      }
      Ok(entry)
    }
    FactoryContextSelection::CallOutput { call_id, entry } => {
      let identity = FactoryKey::new(call_id.to_string()).map_err(|_| FactoryContextError::InvalidContext)?;
      let call = calls
        .iter()
        .copied()
        .find(|call| call.id() == call_id)
        .ok_or(FactoryContextError::UndeclaredSelection)?;
      let completion = snapshot
        .macro_call_completions
        .iter()
        .find(|completion| completion.call_id() == call.id())
        .filter(|completion| completion.subject() == call.subject())
        .ok_or(FactoryContextError::UndeclaredSelection)?;
      if !matches!(
        entry.source_kind(),
        ContextSourceKind::TypedResult | ContextSourceKind::BoundedSummary
      ) || entry.logical_identity() != &identity
        || !call_output_matches_completion(&entry, completion)
      {
        return Err(FactoryContextError::UndeclaredSelection);
      }
      Ok(entry)
    }
  }
}

fn validate_configuration_policy(
  snapshot: &FactoryRunSnapshot,
  entry: ContextManifestEntry,
) -> Result<ContextManifestEntry, FactoryContextError> {
  let FactoryContextReference::Artifact(reference) = entry.source() else {
    return Err(FactoryContextError::UndeclaredSelection);
  };
  let expected_digest = snapshot.work.configuration().definition_digest();
  if entry.subject().exact() != snapshot.work.subject()
    || entry.logical_identity().as_str() != CONFIGURATION_POLICY_IDENTITY
    || reference.content_digest() != expected_digest
  {
    return Err(FactoryContextError::UndeclaredSelection);
  }
  Ok(entry)
}

fn validate_repository_range(
  snapshot: &FactoryRunSnapshot,
  entry: ContextManifestEntry,
) -> Result<ContextManifestEntry, FactoryContextError> {
  let FactoryContextReference::RepositoryRange(range) = entry.source() else {
    return Err(FactoryContextError::UndeclaredSelection);
  };
  if entry.subject().exact() != snapshot.work.subject()
    || entry.logical_identity().as_str() != REPOSITORY_RANGE_IDENTITY
    || range.repository_id() != snapshot.work.subject().repository_id()
    || (range.revision() != entry.subject().exact().base_revision()
      && entry
        .subject()
        .candidate()
        .is_none_or(|candidate| range.revision() != candidate.candidate_revision()))
  {
    return Err(FactoryContextError::UndeclaredSelection);
  }
  Ok(entry)
}

fn call_output_matches_completion(
  entry: &ContextManifestEntry,
  completion: &octacity_server_factory::MacroCallCompletion,
) -> bool {
  let FactoryContextReference::Artifact(reference) = entry.source() else {
    return false;
  };
  let expected = match entry.source_kind() {
    ContextSourceKind::TypedResult => completion.result(),
    ContextSourceKind::BoundedSummary => completion.summary_artifact(),
    _ => return false,
  };
  expected == Some(reference)
}

fn validate_work_artifact(
  snapshot: &FactoryRunSnapshot,
  entry: ContextManifestEntry,
  permits: impl FnOnce(&octacity_server_factory::FactoryArtifactReference) -> bool,
) -> Result<ContextManifestEntry, FactoryContextError> {
  let FactoryContextReference::Artifact(reference) = entry.source() else {
    return Err(FactoryContextError::UndeclaredSelection);
  };
  if snapshot.work.subject() != entry.subject().exact() || !permits(reference) {
    return Err(FactoryContextError::UndeclaredSelection);
  }
  Ok(entry)
}

fn validate_evidence(
  snapshot: &FactoryRunSnapshot,
  entry: ContextManifestEntry,
) -> Result<ContextManifestEntry, FactoryContextError> {
  let FactoryContextReference::Artifact(reference) = entry.source() else {
    return Err(FactoryContextError::UndeclaredSelection);
  };
  let persisted = snapshot.evidence.iter().any(|manifest| {
    FactoryTaskSubject::Candidate(manifest.subject().clone()) == *entry.subject()
      && manifest.items().iter().any(|item| item.artifact() == reference)
  });
  if !persisted {
    return Err(FactoryContextError::UndeclaredSelection);
  }
  Ok(entry)
}
