//! Trusted configured publication bridge to the provider-neutral process host.
use octacity_delivery_protocol::{
  ArtifactInput, Command, CredentialClass, CredentialHandle, DELIVERY_PROTOCOL_VERSION, DeliveryIntent,
  DeliveryReceipt, FailureClass, Outcome, RepositoryAccess, Request, ReviewMetadata, ReviewState, TargetPolicy,
};
use octacity_server_application::{
  ApplicationError, FactoryCandidatePublication, FactoryCandidatePublicationRequest, FactoryCandidatePublisher,
  FactoryExecutionClock,
};
use octacity_server_delivery::DeliveryAdapterRegistry;
use octacity_server_domain::{ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::*;
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
  time::Duration,
};
use tokio_util::sync::CancellationToken;

/// Reusable operator-owned branch policy; exact references are frozen per operation.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryDeliveryTargetPolicy {
  /// Operator-selected base reference, unchanged across replay.
  pub base_reference: String,
  /// Prefix for candidate references; the stable operation digest is appended.
  /// No Flow or node name participates in materialization.
  pub candidate_reference_prefix: String,
}
impl FactoryDeliveryTargetPolicy {
  fn materialize(&self, operation_id: FactoryDigest) -> TargetPolicy {
    TargetPolicy {
      base_reference: self.base_reference.clone(),
      candidate_reference: format!("{}{}", self.candidate_reference_prefix, operation_id),
    }
  }
}
/// Operator-owned provider policy. This draft has no Debug implementation:
/// locators and protected credential handles are confined to the trusted host.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryDeliveryPolicyDraft {
  /// Exact installed provider plugin, not a Flow or node identity.
  pub plugin: ImmutableReference,
  /// Verified immutable provider configuration excluding token rotation.
  pub configuration_digest: FactoryDigest,
  /// Admitted Project scope.
  pub project_id: ProjectId,
  /// Admitted Repository scope.
  pub repository_id: RepositoryId,
  /// Provider-owned repository selection.
  pub repository_locator: String,
  /// Required frozen secret-delivery ceiling.
  pub credential_profile: FactoryKey,
  /// Opaque installed handle; the server never resolves the token.
  pub credential_handle: String,
  /// Provider profile's declared exact network hosts.
  #[serde(serialize_with = "serialize_hosts", deserialize_with = "deserialize_hosts")]
  pub network_hosts: Vec<NetworkHost>,
  /// Reusable operator-selected branch strategy.
  pub target: FactoryDeliveryTargetPolicy,
  /// Bounded operator-selected review metadata.
  pub review: ReviewMetadata,
}
fn serialize_hosts<S: serde::Serializer>(hosts: &[NetworkHost], serializer: S) -> Result<S::Ok, S::Error> {
  serde::Serialize::serialize(&hosts.iter().map(NetworkHost::as_str).collect::<Vec<_>>(), serializer)
}
fn deserialize_hosts<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<NetworkHost>, D::Error> {
  use serde::Deserialize as _;
  Vec::<String>::deserialize(deserializer)?
    .into_iter()
    .map(|host| NetworkHost::new(host).map_err(|_| serde::de::Error::custom("invalid Delivery host")))
    .collect()
}
/// Immutable, scope-bound provider policy selected by action parameters.
pub struct FactoryDeliveryPolicy {
  reference: ImmutableReference,
  draft: FactoryDeliveryPolicyDraft,
}
impl FactoryDeliveryPolicy {
  /// Freezes the complete operator profile under a content-derived identity.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    mut draft: FactoryDeliveryPolicyDraft,
  ) -> Result<Self, ApplicationError> {
    if draft.network_hosts.is_empty()
      || draft.network_hosts.len() > 256
      || draft.repository_locator.is_empty()
      || draft.repository_locator.len() > 1024
      || draft.credential_handle.is_empty()
      || draft.credential_handle.len() > 1024
      || draft.repository_locator.chars().any(char::is_control)
      || draft.credential_handle.chars().any(char::is_control)
      || draft.target.base_reference.is_empty()
      || draft.target.base_reference.len() > 1024
      || draft.target.base_reference.chars().any(char::is_control)
      || draft.target.candidate_reference_prefix.is_empty()
      || draft.target.candidate_reference_prefix.len() > 1024 - 64
      || draft.target.candidate_reference_prefix.chars().any(char::is_control)
    {
      return Err(ApplicationError::invalid());
    }
    draft.network_hosts.sort();
    draft.network_hosts.dedup();
    let bytes = serde_json::to_vec(&draft).map_err(|_| ApplicationError::invalid())?;
    let reference = ImmutableReference::new(
      identity,
      version,
      FactoryDigest::sha256("octacity.factory.delivery-policy.v1", &[&bytes]),
    );
    Ok(Self { reference, draft })
  }
  /// Exact content identity to publish in the action's configured target.
  pub fn reference(&self) -> &ImmutableReference {
    &self.reference
  }
}
/// Protected retained inputs; no server-side or model-side forge token.
pub struct FactoryDeliveryInputs {
  /// Exact authorized capture bundle handle and integrity metadata.
  pub bundle: ArtifactInput,
  /// Exact authorized manifest handle and integrity metadata.
  pub manifest: ArtifactInput,
}
pub(super) fn delivery_artifact_input(
  operation: FactoryDigest,
  role: &str,
  artifact: &FactoryArtifactReference,
) -> ArtifactInput {
  ArtifactInput {
    handle: format!("factory.delivery:{operation}:{role}"),
    sha256: artifact.content_digest().to_string(),
    size_bytes: artifact.encoded_size(),
  }
}
/// Deployment materializer for already-authorized immutable capture artifacts.
/// Implementations verify Artifact ownership and bytes before staging them in
/// the provider's private input directory under the supplied hashed handles.
#[async_trait::async_trait]
pub trait FactoryDeliveryMaterializer: Send + Sync {
  /// Stages only the exact selected artifacts, never source-provided paths or tokens.
  async fn stage(
    &self,
    request: &FactoryCandidatePublicationRequest,
    inputs: &FactoryDeliveryInputs,
  ) -> Result<(), ApplicationError>;
}
/// Trusted publication capability backed by the common Delivery process host.
pub struct HostedFactoryCandidatePublisher {
  registry: Arc<DeliveryAdapterRegistry>,
  policies: BTreeMap<ImmutableReference, FactoryDeliveryPolicy>,
  material: Arc<dyn FactoryDeliveryMaterializer>,
  clock: Arc<dyn FactoryExecutionClock>,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
  active: Mutex<BTreeMap<FactoryDigest, (CancellationToken, u32)>>,
}
impl HostedFactoryCandidatePublisher {
  /// Installs exact policies and protected materialization without resolving credentials.
  pub fn new(
    registry: Arc<DeliveryAdapterRegistry>,
    policies: Vec<FactoryDeliveryPolicy>,
    material: Arc<dyn FactoryDeliveryMaterializer>,
    clock: Arc<dyn FactoryExecutionClock>,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, ApplicationError> {
    if policies.is_empty() || policies.len() > 256 || timeout.is_zero() || grace.is_zero() {
      return Err(ApplicationError::invalid());
    }
    let mut selected = BTreeMap::new();
    for policy in policies {
      if selected.insert(policy.reference.clone(), policy).is_some() {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self {
      registry,
      policies: selected,
      material,
      clock,
      timeout,
      grace,
      cancellation,
      active: Mutex::new(BTreeMap::new()),
    })
  }
  fn active_operation(&self, id: FactoryDigest) -> Result<ActiveDelivery<'_>, ApplicationError> {
    let mut active = self.active.lock().map_err(|_| ApplicationError::unavailable())?;
    let (token, count) = active.entry(id).or_insert_with(|| (self.cancellation.child_token(), 0));
    *count = count.checked_add(1).ok_or_else(ApplicationError::invalid)?;
    Ok(ActiveDelivery {
      id,
      token: token.clone(),
      active: &self.active,
    })
  }
  fn authorize(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<&FactoryDeliveryPolicy, ApplicationError> {
    let policy = self
      .policies
      .get(&request.target)
      .ok_or_else(ApplicationError::invalid)?;
    let draft = &policy.draft;
    if request.candidate.exact().project_id() != draft.project_id
      || request.candidate.exact().repository_id() != draft.repository_id
      || !request.permissions.plugins().any(|plugin| plugin == &draft.plugin)
      || !request
        .permissions
        .secret_profiles()
        .any(|profile| profile == &draft.credential_profile)
      || draft
        .network_hosts
        .iter()
        .any(|host| !request.permissions.network_hosts().any(|allowed| allowed == host))
    {
      return Err(ApplicationError::invalid());
    }
    Ok(policy)
  }
  fn intent(&self, request: &FactoryCandidatePublicationRequest, policy: &FactoryDeliveryPolicy) -> DeliveryIntent {
    let input =
      |role: &str, artifact: &FactoryArtifactReference| delivery_artifact_input(request.operation_id, role, artifact);
    DeliveryIntent {
      delivery_id: request.operation_id.to_string(),
      repository: RepositoryAccess {
        repository_id: policy.draft.repository_id.to_string(),
        repository_locator: policy.draft.repository_locator.clone(),
        credential: CredentialHandle {
          class: CredentialClass::DeliveryWrite,
          handle: policy.draft.credential_handle.clone(),
        },
      },
      base_revision: request.candidate.exact().base_revision().to_string(),
      candidate_revision: request.candidate.candidate_revision().to_string(),
      // Delivery v1 names the retained manifest SHA here. The domain candidate
      // keeps its separate composite identity, verified before staging bytes.
      changeset_digest: request.manifest.content_digest().to_string(),
      decision_digest: request.accepted_input_digest.to_string(),
      configuration_digest: policy.draft.configuration_digest.to_string(),
      network_hosts: policy
        .draft
        .network_hosts
        .iter()
        .map(|host| host.as_str().to_owned())
        .collect(),
      target: policy.draft.target.materialize(request.operation_id),
      bundle: input("bundle", &request.bundle),
      manifest: input("manifest", &request.manifest),
      review: policy.draft.review.clone(),
    }
  }
  async fn execute(
    &self,
    request: &FactoryCandidatePublicationRequest,
    publish: bool,
  ) -> Result<Option<FactoryCandidatePublication>, ApplicationError> {
    let policy = self.authorize(request)?;
    let now = self.clock.now()?;
    let authority_end = request.deadline.min(request.authority_expires_at);
    if now >= authority_end || now < request.started_at {
      return Err(ApplicationError::invalid());
    }
    let elapsed =
      u64::try_from(now.unix_millis() - request.started_at.unix_millis()).map_err(|_| ApplicationError::invalid())?;
    let remaining = request
      .budget
      .max_elapsed_millis()
      .checked_sub(elapsed)
      .filter(|left| *left > 0)
      .ok_or_else(ApplicationError::invalid)?;
    let timeout = self
      .timeout
      .min(Duration::from_millis(remaining))
      .min(Duration::from_millis(
        (authority_end.unix_millis() - now.unix_millis()) as u64,
      ));
    let adapter = self
      .registry
      .resolve(
        policy.draft.plugin.identity().as_str(),
        &policy.draft.plugin.digest().to_string(),
      )
      .map_err(|_| ApplicationError::invalid())?;
    if adapter.manifest.capabilities.configuration_digest != policy.draft.configuration_digest.to_string() {
      return Err(ApplicationError::invalid());
    }
    let intent = self.intent(request, policy);
    let wire = Request {
      protocol_version: DELIVERY_PROTOCOL_VERSION,
      request_id: request.operation_id.to_string(),
      command: if publish {
        Command::Publish(intent.clone())
      } else {
        Command::Observe(intent.clone())
      },
    };
    wire.validate().map_err(|_| ApplicationError::invalid())?;
    let active = self.active_operation(request.operation_id)?;
    if publish {
      let inputs = FactoryDeliveryInputs {
        bundle: intent.bundle,
        manifest: intent.manifest,
      };
      tokio::select! {
        biased;
        _=active.token.cancelled()=>return Err(ApplicationError::unavailable()),
        staged=tokio::time::timeout(timeout,self.material.stage(request,&inputs))=> {
          staged.map_err(|_|ApplicationError::unavailable())??;
        }
      }
    }
    let end = self.clock.now()?;
    if end >= authority_end {
      return Err(ApplicationError::invalid());
    }
    let timeout = timeout.min(Duration::from_millis(
      (authority_end.unix_millis() - end.unix_millis()) as u64,
    ));
    let outcome = adapter
      .execute(wire, timeout, self.grace, active.token.clone())
      .await
      .map_err(|error| match error.class() {
        octacity_server_delivery::HostFailureClass::Transient
        | octacity_server_delivery::HostFailureClass::Cancelled => ApplicationError::unavailable(),
        _ => ApplicationError::invalid(),
      })?;
    match outcome {
      Outcome::Absent | Outcome::RepairRequired(_) if !publish => Ok(None),
      Outcome::Observed(receipt) if receipt.review.is_some() => self.publication(request, *receipt).map(Some),
      // A partially published branch is repairable by the adapter's original identity.
      Outcome::Observed(_) if !publish => Ok(None),
      Outcome::Failure(failure) => Err(match failure.class {
        FailureClass::Transient | FailureClass::Cancelled | FailureClass::Unsupported => {
          ApplicationError::unavailable()
        }
        FailureClass::Conflict => ApplicationError::PreconditionFailed,
        _ => ApplicationError::invalid(),
      }),
      _ => Err(ApplicationError::invalid()),
    }
  }
  fn publication(
    &self,
    request: &FactoryCandidatePublicationRequest,
    receipt: DeliveryReceipt,
  ) -> Result<FactoryCandidatePublication, ApplicationError> {
    let now = self.clock.now()?;
    if now >= request.deadline.min(request.authority_expires_at) || now < request.started_at {
      return Err(ApplicationError::invalid());
    }
    let bytes = serde_json::to_vec(&receipt).map_err(|_| ApplicationError::invalid())?;
    let review = receipt.review.ok_or_else(ApplicationError::invalid)?;
    let state = match review.state {
      ReviewState::Open => octacity_server_application::FactoryReviewState::Open,
      ReviewState::MergedExternally => octacity_server_application::FactoryReviewState::MergedExternally,
      ReviewState::Closed => return Err(ApplicationError::PreconditionFailed),
    };
    let usage = BudgetUsage {
      elapsed_millis: (now.unix_millis() - request.started_at.unix_millis()) as u64,
      output_bytes: bytes.len() as u64,
      ..Default::default()
    };
    usage
      .validate(request.budget)
      .map_err(|_| ApplicationError::invalid())?;
    Ok(FactoryCandidatePublication {
      operation_id: request.operation_id,
      candidate: request.candidate.clone(),
      receipt: FactoryDigest::sha256("octacity.factory.delivery-receipt.v1", &[&bytes]),
      facts: octacity_server_application::FactoryPublicationFacts::new(
        receipt.branch_reference,
        octacity_server_domain::ImmutableRevision::new(receipt.remote_head).map_err(|_| ApplicationError::invalid())?,
        review.identity,
        state,
      )?,
      usage,
    })
  }
}
#[async_trait::async_trait]
impl FactoryCandidatePublisher for HostedFactoryCandidatePublisher {
  async fn observe(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<Option<FactoryCandidatePublication>, ApplicationError> {
    self.execute(request, false).await
  }
  async fn publish(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<FactoryCandidatePublication, ApplicationError> {
    self.execute(request, true).await?.ok_or_else(ApplicationError::invalid)
  }
  async fn cancel(
    &self,
    request: &FactoryCandidatePublicationRequest,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    let policy = self.authorize(request)?;
    let now = self.clock.now()?;
    if requested_at < request.started_at || requested_at > now || now >= request.authority_expires_at {
      return Err(ApplicationError::invalid());
    }
    if let Some((token, _)) = self
      .active
      .lock()
      .map_err(|_| ApplicationError::unavailable())?
      .get(&request.operation_id)
    {
      token.cancel();
      return Ok(None);
    }
    let adapter = self
      .registry
      .resolve(
        policy.draft.plugin.identity().as_str(),
        &policy.draft.plugin.digest().to_string(),
      )
      .map_err(|_| ApplicationError::invalid())?;
    if adapter.manifest.capabilities.configuration_digest != policy.draft.configuration_digest.to_string() {
      return Err(ApplicationError::invalid());
    }
    let outcome = adapter
      .execute(
        Request {
          protocol_version: DELIVERY_PROTOCOL_VERSION,
          request_id: request.operation_id.to_string(),
          command: Command::Cancel {
            target_delivery_id: request.operation_id.to_string(),
          },
        },
        self.timeout,
        self.grace,
        self.cancellation.child_token(),
      )
      .await
      .map_err(|_| ApplicationError::unavailable())?;
    if !matches!(outcome, Outcome::Acknowledged { .. }) {
      return Err(ApplicationError::unavailable());
    }
    // The original deadline closes the authorized execution window even when
    // this replacement observer acknowledges the stop after that deadline.
    let end = self.clock.now()?.min(request.deadline);
    let usage = BudgetUsage {
      elapsed_millis: u64::try_from(end.unix_millis() - request.started_at.unix_millis())
        .map_err(|_| ApplicationError::invalid())?,
      ..Default::default()
    };
    usage
      .validate(request.budget)
      .map_err(|_| ApplicationError::invalid())?;
    Ok(Some(usage))
  }
}
struct ActiveDelivery<'a> {
  id: FactoryDigest,
  token: CancellationToken,
  active: &'a Mutex<BTreeMap<FactoryDigest, (CancellationToken, u32)>>,
}
impl Drop for ActiveDelivery<'_> {
  fn drop(&mut self) {
    if let Ok(mut active) = self.active.lock()
      && let Some((token, count)) = active.get_mut(&self.id)
    {
      *count -= 1;
      if *count == 0 {
        token.cancel();
        active.remove(&self.id);
      }
    }
  }
}
