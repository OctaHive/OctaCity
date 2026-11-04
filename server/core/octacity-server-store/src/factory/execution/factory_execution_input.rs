//! Trusted execution inputs have node provenance, never fabricated Agent output provenance.
use crate::{ArtifactContentDigest, ArtifactMediaType, StoreError, StoreInputError, StoreOperation};
use async_trait::async_trait;
use octacity_protocol::{ProtectedInputManifestV3, ProtectedInputV3};
use octacity_server_domain::{ArtifactId, ArtifactUploadId, Timestamp};
use octacity_server_factory::{FactoryClaimOwnership, FlowBuildIntent};

/// Backend-neutral published byte identity used only for protected execution input transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedExecutionInput {
  /// Opaque logical identity.
  pub artifact_id: ArtifactId,
  /// Opaque immutable byte publication identity.
  pub upload_id: ArtifactUploadId,
  /// Verified exact length.
  pub size_bytes: u64,
  /// Verified exact SHA-256.
  pub digest: ArtifactContentDigest,
  /// Exact logical media type frozen in the signed protected-input contract.
  pub media_type: ArtifactMediaType,
  /// Deadline beyond which private bytes must not be granted.
  pub expires_at: Option<Timestamp>,
}

impl PublishedExecutionInput {
  /// Projects the logical contract of an ordinary immutable publication.
  /// A generic upload transport does not replace the signed input media type.
  pub fn from_artifact(upload: &crate::ArtifactUploadRecord) -> Result<Self, StoreError> {
    let identity = upload.artifact.identity();
    Ok(Self {
      artifact_id: identity.artifact_id,
      upload_id: upload.upload_id,
      size_bytes: identity.size_bytes,
      digest: identity.digest,
      media_type: identity.media_type.clone(),
      expires_at: identity.retention.delete_after(),
    })
  }
}

/// Verified staged bytes selected by the trusted compiler.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FactoryExecutionInputPublication {
  /// Manifest declaration, including its reserved destination.
  pub input: ProtectedInputV3,
  /// Byte-store publication identity.
  pub upload_id: ArtifactUploadId,
}

/// Atomic immutable publication under an existing fenced node operation.
#[derive(Clone, Eq, PartialEq)]
pub struct PublishFactoryExecutionInputs {
  /// Exact persisted intent; the caller cannot substitute its authority.
  pub intent: FlowBuildIntent,
  /// Current owner authorizing fresh publication.
  pub ownership: FactoryClaimOwnership,
  /// Complete bounded input manifest, in canonical identity order.
  pub inputs: Vec<FactoryExecutionInputPublication>,
  /// Trusted observation time. Identical replay retains the original time.
  pub published_at: Timestamp,
}
impl PublishFactoryExecutionInputs {
  /// Checks protocol bounds and typed logical byte identities before persistence.
  pub fn validate(&self) -> Result<(), StoreError> {
    let invalid = || {
      StoreError::invalid(
        StoreOperation::PublishFactoryExecutionInputs,
        StoreInputError::InvalidFactoryBuild,
      )
    };
    ProtectedInputManifestV3 {
      inputs: self.inputs.iter().map(|entry| entry.input.clone()).collect(),
    }
    .validate()
    .map_err(|_| invalid())?;
    for entry in &self.inputs {
      entry.input.artifact_id.parse::<ArtifactId>().map_err(|_| invalid())?;
      ArtifactContentDigest::from_lower_hex(&entry.input.sha256).map_err(|_| invalid())?;
      ArtifactMediaType::new(entry.input.media_type.clone()).map_err(|_| invalid())?;
    }
    Ok(())
  }
}

/// Publishes private managed inputs before Build acceptance, without an Agent Lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryExecutionInputDisposition {
  /// Durable cleanup identities exist; no read capability may be granted yet.
  Pending,
  /// Identical verified bytes have already been published.
  Published,
}

/// Private package metadata owned by the existing fenced input publication operation.
/// Static adapters may omit this capability; role dispatch then fails closed.
#[async_trait]
pub trait FactoryRoleInvocationStore: Send + Sync {
  /// Exact private package snapshot, including an interrupted publication. No catalog lookup.
  async fn factory_role_invocation(
    &self,
    _operation: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<RetainedFactoryRoleInvocation>, StoreError> {
    Ok(None)
  }
  /// Freezes the package under the same node fence before compiler object writes.
  /// Identical replay preserves the original content and never selects newer definitions.
  async fn retain_factory_role_invocation(
    &self,
    _intent: &FlowBuildIntent,
    _ownership: &FactoryClaimOwnership,
    _invocation: RetainedFactoryRoleInvocation,
    _observed_at: Timestamp,
  ) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }
}

/// Durable pre-write reservation and independently verified publication.
#[async_trait]
pub trait FactoryExecutionInputStore: Send + Sync {
  /// Reserves cleanup metadata before storage I/O; unpublished inputs stay invisible.
  async fn reserve_factory_execution_inputs(
    &self,
    request: PublishFactoryExecutionInputs,
  ) -> Result<FactoryExecutionInputDisposition, StoreError>;
  /// Atomically binds the complete verified manifest to the persisted operation.
  /// Fresh publication is fenced; an exact replay remains read-only after expiry.
  async fn publish_factory_execution_inputs(&self, request: PublishFactoryExecutionInputs) -> Result<(), StoreError>;
}

/// Exact acknowledgement after idempotent physical deletion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteFactoryExecutionInputDeletion {
  /// Immutable publication or reservation selected for cleanup.
  pub expected: PublishedExecutionInput,
  /// Trusted time, at or after the private input deadline.
  pub deleted_at: Timestamp,
}

/// Bounded cleanup for expired private inputs, including interrupted publications.
#[async_trait]
pub trait FactoryExecutionInputRetentionStore: Send + Sync {
  /// Lists at most 100 immutable cleanup identities. Deletion is idempotent, so
  /// concurrent replicas may safely process the same expired identity.
  async fn expired_factory_execution_inputs(
    &self,
    observed_at: Timestamp,
    limit: u16,
  ) -> Result<Vec<PublishedExecutionInput>, StoreError>;
  /// Removes input visibility only after the exact expired physical object was deleted.
  async fn complete_factory_execution_input_deletion(
    &self,
    request: CompleteFactoryExecutionInputDeletion,
  ) -> Result<(), StoreError>;
}

/// Private immutable compilation input. Dedicated recovery only; never generic diagnostics.
#[derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedFactoryRoleInvocation {
  /// Canonical exact rules, instructions, profile, task and skill inventories.
  pub package: octacity_server_factory::FactoryRoleExecutionPackage,
  /// Exact read-only imported skill destinations, without download grants.
  pub skills: Option<ProtectedInputManifestV3>,
  /// Qualified existing logical credentials, never secret values.
  pub secret_variables: std::collections::BTreeMap<String, octacity_server_secrets::LogicalSecretReference>,
}
impl RetainedFactoryRoleInvocation {
  /// Revalidates the exact ordinary operation before retaining or recovering content.
  pub fn verify_intent(&self, intent: &FlowBuildIntent) -> Result<(), StoreError> {
    let invalid = || {
      StoreError::invalid(
        StoreOperation::PublishFactoryExecutionInputs,
        StoreInputError::InvalidFactoryBuild,
      )
    };
    let inputs = self.package.inputs();
    inputs.task.verify_build_intent(intent).map_err(|_| invalid())?;
    if intent
      .selected_profile()
      .is_some_and(|profile| profile != inputs.profile.reference())
      || inputs.profile.definition().build_configuration != intent.profile().build_configuration
      || inputs.task.permissions() != intent.permissions()
      || inputs.task.budget() != intent.budget()
    {
      return Err(invalid());
    }
    Ok(())
  }
}
