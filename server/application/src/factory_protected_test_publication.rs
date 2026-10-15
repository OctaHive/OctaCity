//! Server-derived private inventory over actual outputs of one fenced Job.
use crate::{AgentOutputFinalization, ApplicationError, FactoryExecutionClock, FactoryProtectedTestInventory};
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactObject};
use octacity_server_domain::{ArtifactId, ArtifactName, ArtifactUploadId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// Distinct from ordinary runner tasks and the reserved Agent ChangeSet producer.
const INVENTORY_PRODUCER_ID: u64 = u64::MAX - 1;

/// Installed mapping from a private logical output to its protected test role.
/// These parameters come from the pinned author profile, never model output.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProtectedTestFileOutput {
  /// Exact logical Artifact name declared by the author program.
  pub output: FactoryKey,
  /// Arbitrary required class selected by the installed coverage policy.
  pub class: FactoryKey,
  /// Portable destination within the private test bundle.
  pub path: FactoryRepositoryPath,
}

/// Trusted authoring inputs resolved from the frozen installed execution.
#[derive(Clone, Copy)]
pub struct FactoryProtectedTestInventoryPublication<'a> {
  /// Complete immutable admission needed to verify the selected contract.
  pub admitted: &'a AdmittedFlow,
  /// Exact persisted operation being finalized.
  pub intent: &'a FlowBuildIntent,
  /// Actual ordinary Build, Attempt and Job identities.
  pub execution: &'a FlowBuildExecution,
  /// Coverage and retention frozen from the accepted contract.
  pub plan: &'a ProtectedTestPlan,
  /// Exact declared private inventory output and trusted producer pins.
  pub inventory: &'a EvidenceRequirement,
  /// Finite installed private output selection.
  pub files: &'a [FactoryProtectedTestFileOutput],
  /// Authenticated completion, revalidated by the store before publication.
  pub completion: AgentOutputFinalization,
}

/// Publishes private inventory only after the actual file IDs are known.
/// This capability does not complete a Job or select a Flow transition.
pub struct FactoryProtectedTestInventoryPublisher<A, B> {
  artifacts: Arc<A>,
  bytes: Arc<B>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<A: ArtifactRecordStore, B: ArtifactBytePublisher> FactoryProtectedTestInventoryPublisher<A, B> {
  /// Connects current Lease fencing, immutable object publication and trusted time.
  pub fn new(artifacts: Arc<A>, bytes: Arc<B>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self {
      artifacts,
      bytes,
      clock,
    }
  }

  /// Reuses already published file identities; no producer predicts future IDs.
  /// Exact retries reuse the original inventory and perform no additional writes.
  pub async fn publish(
    &self,
    request: FactoryProtectedTestInventoryPublication<'_>,
  ) -> Result<FactoryArtifactReference, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let FactoryProtectedTestInventoryPublication {
      admitted,
      intent,
      execution,
      plan,
      inventory,
      files,
      completion,
    } = request;
    let node = admitted
      .closure()
      .definition(intent.input().definition())
      .and_then(|definition| definition.node(intent.input().node()))
      .ok_or_else(invalid)?;
    let binding = node.build().ok_or_else(invalid)?;
    if files.is_empty()
      || files.len() >= usize::from(MAX_ARTIFACT_PAGE_SIZE)
      || inventory.output_kind() != EvidenceOutputKind::Artifact
      || inventory.tool() != intent.profile().result_tool()
      || inventory.plugin() != &intent.profile().plugin
      || !binding.evidence.contains(inventory)
      || intent
        .input()
        .context()
        .is_none_or(|context| context.subject() != plan.subject())
      || execution.jobs() != [completion.job_id]
      || FlowBuildExecution::new(
        intent,
        execution.build_id(),
        execution.attempt_id(),
        execution.jobs().to_vec(),
      )
      .map_err(|_| invalid())?
        != *execution
      || FlowBuildIntent::new(
        intent.input().clone(),
        admitted,
        intent.node().clone(),
        intent.usage_before(),
        intent.priority(),
      )
      .map_err(|_| invalid())?
        != *intent
    {
      return Err(invalid());
    }
    let records = self
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: execution.build_id(),
        attempt_id: Some(execution.attempt_id()),
        limit: MAX_ARTIFACT_PAGE_SIZE,
        visibility: ArtifactListVisibility::all(),
      })
      .await?;
    let at = self.publication_time(request)?;
    let mut selected = Vec::with_capacity(files.len());
    for configured in files {
      let record = records
        .iter()
        .find(|record| record.artifact.identity().logical_name.as_str() == configured.output.as_str())
        .ok_or_else(invalid)?;
      let identity = record.artifact.identity();
      if identity.build_id != execution.build_id()
        || identity.attempt_id != execution.attempt_id()
        || identity.job_id != completion.job_id
        || identity.lease_id != completion.lease.lease_id
        || identity.artifact_type != ArtifactType::Artifact
        || record.artifact.state() != ArtifactState::Published
        || record.artifact.published_at().is_none_or(|published| published > at)
        || identity
          .retention
          .delete_after()
          .is_some_and(|expiry| expiry < plan.retain_until())
        || !intent
          .permissions()
          .outputs()
          .kinds()
          .any(|name| name == &configured.output)
      {
        return Err(invalid());
      }
      selected.push(ProtectedTestFile {
        class: configured.class.clone(),
        path: configured.path.clone(),
        artifact: FactoryArtifactReference::new(
          identity.artifact_id,
          FactoryDigest::from_bytes(identity.digest.as_bytes()),
          identity.size_bytes,
        )
        .map_err(|_| invalid())?,
      });
    }
    selected.sort_by(|left, right| left.path.cmp(&right.path));
    let data = FactoryProtectedTestInventory {
      plan_digest: plan.digest().map_err(|_| invalid())?,
      input_digest: intent.input().digest().map_err(|_| invalid())?,
      node_attempt_id: intent.node().id(),
      files: selected.clone(),
    };
    let raw = serde_json::to_vec(&data).map_err(|_| invalid())?;
    let digest = FactoryDigest::content_sha256(&raw);
    let operation = intent.operation_id().map_err(|_| invalid())?;
    let identity = FactoryDigest::sha256(
      "octacity.factory.test-inventory.publication.v1",
      &[
        &operation.as_bytes(),
        completion.lease.lease_id.to_string().as_bytes(),
        inventory.kind().as_str().as_bytes(),
        &digest.as_bytes(),
      ],
    );
    let artifact_id =
      ArtifactId::from_uuid(Uuid::new_v5(&Uuid::NAMESPACE_OID, &identity.as_bytes())).map_err(|_| invalid())?;
    let reference = FactoryArtifactReference::new(artifact_id, digest, raw.len() as u64).map_err(|_| invalid())?;
    // Reuse the domain's closed coverage, file/path uniqueness and aggregate byte
    // validation before making even a logical reservation.
    ProtectedTestBundle::new(
      plan.clone(),
      reference.clone(),
      selected,
      EvidenceProducer::new(
        execution.build_id(),
        execution.attempt_id(),
        completion.job_id,
        inventory.tool().clone(),
        inventory.plugin().clone(),
      ),
      at,
      plan.retain_until(),
    )
    .map_err(|_| invalid())?;
    self
      .publish_document(request, inventory, ArtifactType::Artifact, raw, identity)
      .await
  }

  /// Publishes the private inventory and closed public handoff before successful
  /// Job completion. The exact admitted outcome schema is checked before writes.
  pub async fn publish_outputs(
    &self,
    request: FactoryProtectedTestInventoryPublication<'_>,
  ) -> Result<(), ApplicationError> {
    let invalid = ApplicationError::invalid;
    let node = request
      .admitted
      .closure()
      .definition(request.intent.input().definition())
      .and_then(|definition| definition.node(request.intent.input().node()))
      .ok_or_else(invalid)?;
    let binding = node.build().ok_or_else(invalid)?;
    let schema_ref = node.outcome(&binding.result_outcome).ok_or_else(invalid)?.schema();
    let schema = request.admitted.data_schema(schema_ref).ok_or_else(invalid)?;
    if binding.result_output_kind != EvidenceOutputKind::Report {
      return Err(invalid());
    }
    let value = serde_json::to_value(ProtectedTestHandoff {
      accepted_contract: request.plan.accepted_contract(),
      classes: request.plan.classes().to_vec(),
    })
    .map_err(|_| invalid())?;
    FlowPayload::new(schema, value.clone()).map_err(|_| invalid())?;
    let raw = serde_json::to_vec(&value).map_err(|_| invalid())?;
    let output = EvidenceRequirement::new(
      binding.result_output.clone(),
      EvidenceOutputKind::Report,
      schema_ref.clone(),
      binding.result_tool().clone(),
      binding.plugin.clone(),
    );
    let operation = request.intent.operation_id().map_err(|_| invalid())?;
    let identity = FactoryDigest::sha256(
      "octacity.factory.test-handoff.publication.v1",
      &[
        &operation.as_bytes(),
        request.completion.lease.lease_id.to_string().as_bytes(),
        output.kind().as_str().as_bytes(),
        &FactoryDigest::content_sha256(&raw).as_bytes(),
      ],
    );
    self.publish(request).await?;
    self
      .publish_document(
        request,
        &output,
        ArtifactType::Report(ArtifactReportFormat::new(schema_ref.identity().as_str()).map_err(|_| invalid())?),
        raw,
        identity,
      )
      .await?;
    Ok(())
  }

  async fn publish_document(
    &self,
    request: FactoryProtectedTestInventoryPublication<'_>,
    output: &EvidenceRequirement,
    artifact_type: ArtifactType,
    raw: Vec<u8>,
    identity: FactoryDigest,
  ) -> Result<FactoryArtifactReference, ApplicationError> {
    let (upload_domain, idempotency_prefix): (&[u8], &str) = if artifact_type == ArtifactType::Artifact {
      (b"private-inventory:v1", "factory.inventory")
    } else {
      (b"public-handoff:v1", "factory.test-handoff")
    };
    let invalid = ApplicationError::invalid;
    let intent = request.intent;
    let completion = request.completion;
    let plan = request.plan;
    let artifact_id =
      ArtifactId::from_uuid(Uuid::new_v5(&Uuid::NAMESPACE_OID, &identity.as_bytes())).map_err(|_| invalid())?;
    let digest = FactoryDigest::content_sha256(&raw);
    let reference = FactoryArtifactReference::new(artifact_id, digest, raw.len() as u64).map_err(|_| invalid())?;
    let records = self
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: request.execution.build_id(),
        attempt_id: Some(request.execution.attempt_id()),
        limit: MAX_ARTIFACT_PAGE_SIZE,
        visibility: ArtifactListVisibility::all(),
      })
      .await?;
    let outputs = intent.permissions().outputs();
    // Replay replaces this exact document in quota accounting. The store still
    // validates the current fence even when its bytes are already published.
    let others = records
      .iter()
      .filter(|record| record.artifact.identity().artifact_id != artifact_id);
    let mut artifact_bytes = 0_u64;
    let mut report_bytes = 0_u64;
    let mut artifact_count = 0_u64;
    let mut report_count = 0_u64;
    for (kind, size) in others
      .map(|record| {
        (
          &record.artifact.identity().artifact_type,
          record.artifact.identity().size_bytes,
        )
      })
      .chain(std::iter::once((&artifact_type, raw.len() as u64)))
    {
      let (bytes, count) = if *kind == ArtifactType::Artifact {
        (&mut artifact_bytes, &mut artifact_count)
      } else {
        (&mut report_bytes, &mut report_count)
      };
      *bytes = bytes.checked_add(size).ok_or_else(invalid)?;
      *count += 1;
    }
    self.publication_time(request)?;
    if !outputs.kinds().any(|name| name == output.kind())
      || artifact_count > u64::from(outputs.max_artifact_count())
      || artifact_bytes > outputs.max_artifact_bytes()
      || report_count > u64::from(outputs.max_report_count())
      || report_bytes > outputs.max_report_bytes()
      || artifact_bytes.checked_add(report_bytes).ok_or_else(invalid)? > intent.budget().max_output_bytes()
    {
      return Err(invalid());
    }
    let upload_id =
      ArtifactUploadId::from_uuid(Uuid::new_v5(&artifact_id.as_uuid(), upload_domain)).map_err(|_| invalid())?;
    let media_type = ArtifactMediaType::new("application/json").map_err(|_| invalid())?;
    let reserved = self
      .artifacts
      .begin_artifact_upload(BeginArtifactUpload {
        artifact_id,
        upload_id,
        idempotency_key: IdempotencyKey::new(format!("{idempotency_prefix}:{identity}")).map_err(|_| invalid())?,
        lease: completion.lease,
        job_id: completion.job_id,
        attempt: completion.attempt,
        logical_name: ArtifactName::new(output.kind().as_str()).map_err(|_| invalid())?,
        producer_run_id: INVENTORY_PRODUCER_ID,
        producer_task_id: INVENTORY_PRODUCER_ID,
        artifact_type,
        media_type: media_type.clone(),
        transport_media_type: media_type.clone(),
        size_bytes: raw.len() as u64,
        digest: ArtifactContentDigest::from_bytes(digest.as_bytes()),
        reserved_at: self.publication_time(request)?,
        capability_expires_at: intent.node().deadline(),
      })
      .await?;
    // The generic store deliberately ignores newly proposed IDs on replay.
    // A reserved key belonging to another publication must not redirect this
    // trusted writer to unrelated Artifact or upload storage identities.
    if reserved.upload.artifact.identity().artifact_id != artifact_id
      || reserved.upload.upload_id != upload_id
      || reserved
        .upload
        .artifact
        .identity()
        .retention
        .delete_after()
        .is_some_and(|expiry| expiry < plan.retain_until())
    {
      return Err(invalid());
    }
    if reserved.upload.artifact.state() == ArtifactState::Published {
      return Ok(reference);
    }
    let verify = |observed_at| VerifyArtifactUpload {
      upload_id: reserved.upload.upload_id,
      lease: completion.lease,
      job_id: completion.job_id,
      attempt: completion.attempt,
      observed_at,
    };
    self
      .artifacts
      .begin_artifact_verification(verify(self.publication_time(request)?))
      .await?;
    let object = ArtifactObject::new(
      artifact_id,
      reserved.upload.upload_id,
      raw.len() as u64,
      digest.to_string(),
      media_type.as_str(),
    )
    .map_err(|_| invalid())?;
    self.publication_time(request)?;
    self
      .bytes
      .publish_verified(&object, raw)
      .await
      .map_err(|_| ApplicationError::unavailable())?;
    self
      .artifacts
      .finish_artifact_verification(
        verify(self.publication_time(request)?),
        ArtifactVerificationResult::Verified,
      )
      .await?;
    Ok(reference)
  }

  // A renewable ordinary Lease never extends the original immutable node
  // deadline. Recheck after each awaited operation, including object I/O, before
  // granting visibility to bytes whose transfer may have outlived that window.
  fn publication_time(
    &self,
    request: FactoryProtectedTestInventoryPublication<'_>,
  ) -> Result<Timestamp, ApplicationError> {
    let now = self.clock.now()?.max(request.completion.observed_at);
    if now >= request.intent.node().deadline() {
      return Err(ApplicationError::invalid());
    }
    Ok(now)
  }
}
