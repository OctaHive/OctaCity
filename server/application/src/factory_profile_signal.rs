//! Purpose-specific profile consumption over the shared Signal coordinator and fenced Flow journal.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;

/// Optional installed pre-dispatch capability. It never owns execution or lifecycle transitions.
#[async_trait]
pub trait FactoryProfileSelectionDriver: Send + Sync {
  /// Advances the retained operation under the supplied snapshot's exact owner and version.
  async fn advance(
    &self,
    snapshot: &FactoryRunSnapshot,
    input: &FlowNodeInput,
    selection: &FlowProfileSelectionRecord,
    at: Timestamp,
  ) -> Result<(), ApplicationError>;
}

/// Project-scoped, operator-qualified provider binding. Provider credentials remain in the adapter.
#[derive(Clone)]
pub struct FactoryProfileSelector {
  /// Exact installed contract selected by the Flow, distinct from the signal's consuming policy.
  pub selection_policy: ImmutableReference,
  /// Project allowed to submit the explicitly projected context.
  pub project_id: ProjectId,
  /// Exact questions, calibration, rollout and context projection.
  pub plan: FactoryDecisionSignalRequestPlan,
  /// Explicit candidate used by shadow/advisory and compared in the receipt.
  pub baseline: FactoryKey,
  /// Installed adapter; no provider is inferred from profile names or model IDs.
  pub provider: Arc<dyn DecisionSignalProvider>,
  /// Dedicated server credential capability, absent only for a transport with no network I/O.
  pub credential_profile: Option<FactoryKey>,
  /// Independently installed Project policy for selection, separate from profile execution authority.
  pub project_permissions: FactoryPermissionSet,
  /// Enforceable deployment ceiling shared with other installed Signal consumers.
  pub local_permissions: LocalPermissionCeiling,
}
impl FactoryProfileSelector {
  /// Pins complete installed metadata, as ordinary action bindings do.
  /// A changed plan gets a new reference rather than retargeting earlier Runs.
  pub fn pin_policy(mut self) -> Result<Self, ApplicationError> {
    self.selection_policy = ImmutableReference::new(
      self.selection_policy.identity().clone(),
      self.selection_policy.version().clone(),
      self.policy_digest()?,
    );
    Ok(self)
  }
  fn policy_digest(&self) -> Result<FactoryDigest, ApplicationError> {
    let hosts = self
      .provider
      .network_destinations()
      .ok_or_else(ApplicationError::invalid)?;
    if hosts.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(ApplicationError::invalid());
    }
    let hosts = hosts
      .iter()
      .map(|host| host.as_str())
      .collect::<std::collections::BTreeSet<_>>();
    let enforcement = PermissionCategory::ALL
      .into_iter()
      .filter(|category| self.local_permissions.can_enforce(*category))
      .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&(
      self.selection_policy.identity(),
      self.selection_policy.version(),
      &self.plan,
      &self.baseline,
      &self.credential_profile,
      &self.project_permissions,
      self.local_permissions.grants(),
      enforcement,
      hosts,
    ))
    .map_err(|_| ApplicationError::invalid())?;
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(ApplicationError::invalid());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.profile-selector-policy.v1",
      &[&bytes],
    ))
  }
  /// Qualifies exact capability, criteria and transport authority without I/O.
  pub fn validate(&self) -> Result<(), ApplicationError> {
    let input = DecisionSignalProviderInput::new(
      DecisionSignalInputMedia::CanonicalJson,
      FactoryText::new("null").map_err(|_| ApplicationError::invalid())?,
      self.plan.questions.clone(),
      self.plan.consuming_question.clone(),
      None,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let qualified = self
      .plan
      .threshold
      .clone()
      .qualify_profile_selection(&self.plan.profile, &input)
      .map_err(|_| ApplicationError::invalid())?;
    let effective = resolve_factory_permissions(
      &self.project_permissions,
      &self.project_permissions,
      &self.project_permissions,
      &self.local_permissions,
    )
    .map_err(|_| ApplicationError::invalid())?;
    let hosts = self
      .provider
      .network_destinations()
      .ok_or_else(ApplicationError::invalid)?;
    if self.selection_policy.digest() != self.policy_digest()?
      || self.plan.profile.purpose() != DecisionSignalPurpose::ProfileSelection
      || !self.plan.capability.supports(DecisionSignalPurpose::ProfileSelection)
      || self.provider.capability() != &self.plan.capability
      || self.plan.profile.provider() != self.plan.capability.provider()
      || self.plan.profile.adapter() != self.plan.capability.adapter()
      || self.plan.profile.model() != self.plan.capability.model()
      || self.plan.threshold.probability_semantics() != self.plan.capability.probability_semantics()
      || qualified != self.plan.threshold
      || !input.choices().contains(&self.baseline)
      || !self.plan.call_budget.fits_within(self.plan.profile.budget())
      || !self.plan.call_budget.fits_within(self.plan.capability.budget_ceiling())
      || (!hosts.is_empty()
        && (self.credential_profile.is_none() || !effective.plugins().any(|p| p == self.plan.capability.adapter())))
      || hosts.iter().any(|host| !effective.network_hosts().any(|p| p == host))
      || self
        .credential_profile
        .as_ref()
        .is_some_and(|key| !effective.secret_profiles().any(|p| p == key))
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
}
/// Installed selection policies, using the existing journal rather than another request store.
pub struct FactoryProfileSelectors<S> {
  store: Arc<S>,
  profiles: Vec<FactoryProfileSelector>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S: FactoryRunStore> FactoryProfileSelectors<S> {
  /// Rejects ambiguous or unsupported exact capabilities before they can be selected.
  pub fn new(
    store: Arc<S>,
    profiles: Vec<FactoryProfileSelector>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Result<Self, ApplicationError> {
    if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(ApplicationError::invalid());
    }
    for (index, profile) in profiles.iter().enumerate() {
      profile.validate()?;
      if profiles[..index]
        .iter()
        .any(|other| other.project_id == profile.project_id && other.selection_policy == profile.selection_policy)
      {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self { store, profiles, clock })
  }
}
#[async_trait]
impl<S: FactoryRunStore + 'static> FactoryProfileSelectionDriver for FactoryProfileSelectors<S> {
  async fn advance(
    &self,
    snapshot: &FactoryRunSnapshot,
    input: &FlowNodeInput,
    selection: &FlowProfileSelectionRecord,
    at: Timestamp,
  ) -> Result<(), ApplicationError> {
    let node = snapshot
      .admitted_flow
      .closure()
      .definition(input.definition())
      .and_then(|d| d.node(input.node()))
      .ok_or_else(ApplicationError::invalid)?;
    let binding = node.profile_selection().ok_or_else(ApplicationError::invalid)?;
    let FlowProfileSelectionPolicy::DecisionApi { policy, .. } = &binding.policy else {
      return Err(ApplicationError::invalid());
    };
    let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
    let clock: Arc<dyn FactoryExecutionClock> = Arc::new(OperationClock {
      clock: self.clock.clone(),
      floor: at,
    });
    let journal = Arc::new(ProfileSignalJournal {
      store: self.store.clone(),
      ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
      clock,
    });
    let profile = self
      .profiles
      .iter()
      .find(|p| p.project_id == input.subject().project_id() && &p.selection_policy == policy);
    let Some(profile) = profile else {
      let next = selection
        .unavailable_selector(input, &snapshot.admitted_flow, at)
        .map_err(|_| ApplicationError::invalid())?;
      return journal.commit(snapshot, next).await;
    };
    let effective = resolve_factory_permissions(
      &profile.project_permissions,
      snapshot.admitted_flow.limits().permissions(),
      node.permissions(),
      &profile.local_permissions,
    )
    .map_err(|_| ApplicationError::invalid())?;
    if profile.provider.network_destinations().is_some_and(|hosts| {
      (!hosts.is_empty()
        && !effective
          .plugins()
          .any(|plugin| plugin == profile.plan.capability.adapter()))
        || hosts
          .iter()
          .any(|host| !effective.network_hosts().any(|allowed| allowed == host))
    }) || profile
      .credential_profile
      .as_ref()
      .is_some_and(|credential| !effective.secret_profiles().any(|allowed| allowed == credential))
    {
      return Err(ApplicationError::invalid());
    }
    let Some(signal) = selection.signal() else {
      let prepared = profile
        .plan
        .prepare_profile_selection(input, &snapshot.admitted_flow, selection)
        .and_then(|request| {
          selection.prepare_signal(input, &snapshot.admitted_flow, request, profile.baseline.clone(), at)
        });
      let next = match prepared {
        Ok(next) => next,
        Err(_) => selection
          .fail_selector(
            input,
            &snapshot.admitted_flow,
            FlowProfileSelectionError::InvalidInput,
            at,
          )
          .map_err(|_| ApplicationError::invalid())?,
      };
      return journal.commit(snapshot, next).await;
    };
    let service = DecisionSignalService::for_provider_with_factory_clock(
      profile.provider.clone(),
      journal.clone(),
      journal.clone(),
      journal.clone(),
      journal.clock.clone(),
    );
    let receipt_id = DecisionSignalReceiptId::from_uuid(uuid::Uuid::new_v5(
      &signal.request().id().as_uuid(),
      b"profile-selection-receipt",
    ))
    .map_err(|_| ApplicationError::invalid())?;
    match service
      .select_profile(receipt_id, signal.request(), signal.baseline().clone())
      .await
    {
      Ok(_) => (),
      Err(DecisionSignalApplicationError::AwaitingProviderObservation) => {
        // Observe the reserved operation once; this path cannot authorize another evaluation.
        match service
          .select_profile(receipt_id, signal.request(), signal.baseline().clone())
          .await
        {
          Ok(_) => (),
          Err(DecisionSignalApplicationError::AwaitingProviderObservation) => {
            let snapshot = journal
              .snapshot(signal.request())
              .await
              .map_err(|_| ApplicationError::unavailable())?;
            let current = snapshot
              .flow
              .data
              .current_profile_selection(input.digest().map_err(|_| ApplicationError::invalid())?)
              .ok_or_else(ApplicationError::invalid)?;
            let uncertain = current
              .mark_signal_unobservable(journal.clock.now()?)
              .map_err(|_| ApplicationError::invalid())?;
            journal.commit(&snapshot, uncertain).await?;
          }
          Err(_) => return Err(ApplicationError::unavailable()),
        }
      }
      Err(_) => return Err(ApplicationError::unavailable()),
    }
    Ok(())
  }
}

struct OperationClock {
  clock: Arc<dyn FactoryExecutionClock>,
  floor: Timestamp,
}
impl FactoryExecutionClock for OperationClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(self.floor.max(self.clock.now()?))
  }
}

/// Reuses ordinary node publication for input, selection, request reservation and receipt facts.
struct ProfileSignalJournal<S> {
  store: Arc<S>,
  ownership: FactoryClaimOwnership,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S: FactoryRunStore> ProfileSignalJournal<S> {
  async fn snapshot(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<FactoryRunSnapshot, DecisionSignalPortError> {
    let snapshot = self
      .store
      .factory_run_snapshot(request.request().run_id())
      .await
      .map_err(|_| DecisionSignalPortError::Unavailable)?;
    let selection = snapshot
      .flow
      .data
      .current_profile_selection(request.request().frozen_input_digest())
      .ok_or(DecisionSignalPortError::Conflict)?;
    if selection.signal().is_none_or(|s| s.request() != request) {
      return Err(DecisionSignalPortError::Conflict);
    }
    Ok(snapshot)
  }
  async fn commit(
    &self,
    snapshot: &FactoryRunSnapshot,
    selection: FlowProfileSelectionRecord,
  ) -> Result<(), ApplicationError> {
    let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
    if claim.owner != *self.ownership.owner() || claim.claim != self.ownership.claim() {
      return Err(ApplicationError::invalid());
    }
    let input = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.digest().ok() == Some(selection.input_digest()))
      .ok_or_else(ApplicationError::invalid)?;
    if !factory_profile_wait_is_open(snapshot, input, self.clock.now()?) {
      return Err(ApplicationError::invalid());
    }
    let mut append = FactoryRunHistoryAppend::default();
    append.flow.data.profile_selections.push(selection);
    FactoryNodeRunner::new(self.store.clone(), Arc::new(SelectionOnly), self.clock.clone())
      .commit(snapshot, append, self.clock.now()?)
      .await
  }
}
#[async_trait]
impl<S: FactoryRunStore> DecisionSignalRequestObservation for ProfileSignalJournal<S> {
  async fn observe_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalPortError> {
    let snapshot = self.snapshot(request).await?;
    Ok(
      snapshot
        .flow
        .data
        .current_profile_selection(request.request().frozen_input_digest())
        .and_then(|s| s.signal()?.receipt().cloned()),
    )
  }
}
#[async_trait]
impl<S: FactoryRunStore> DecisionSignalDispatchAuthorization for ProfileSignalJournal<S> {
  async fn was_reserved(&self, request: &DecisionSignalProviderRequest) -> Result<bool, DecisionSignalPortError> {
    Ok(
      self
        .snapshot(request)
        .await?
        .flow
        .data
        .current_profile_selection(request.request().frozen_input_digest())
        .and_then(|s| s.signal())
        .is_some_and(FlowProfileSelectionSignal::reserved),
    )
  }
  async fn authorize_dispatch(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<FactoryDecisionSignalRequestDisposition, DecisionSignalPortError> {
    let snapshot = self.snapshot(request).await?;
    let selection = snapshot
      .flow
      .data
      .current_profile_selection(request.request().frozen_input_digest())
      .ok_or(DecisionSignalPortError::Conflict)?;
    if selection.signal().is_some_and(FlowProfileSelectionSignal::reserved) {
      return Ok(FactoryDecisionSignalRequestDisposition::Replayed);
    }
    let next = selection
      .reserve_signal(self.clock.now().map_err(|_| DecisionSignalPortError::Unavailable)?)
      .map_err(|_| DecisionSignalPortError::Conflict)?;
    self
      .commit(&snapshot, next)
      .await
      .map_err(|_| DecisionSignalPortError::Conflict)?;
    Ok(FactoryDecisionSignalRequestDisposition::Reserved)
  }
}
#[async_trait]
impl<S: FactoryRunStore> DecisionSignalReceiptPublication for ProfileSignalJournal<S> {
  async fn publish_receipt(
    &self,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalPortError> {
    let snapshot = self.snapshot(receipt.request()).await?;
    let selection = snapshot
      .flow
      .data
      .current_profile_selection(receipt.request().request().frozen_input_digest())
      .ok_or(DecisionSignalPortError::Conflict)?;
    let input = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|i| i.digest().ok() == Some(selection.input_digest()))
      .ok_or(DecisionSignalPortError::Conflict)?;
    let next = selection
      .consume_signal(
        input,
        &snapshot.admitted_flow,
        receipt.clone(),
        self.clock.now().map_err(|_| DecisionSignalPortError::Unavailable)?,
      )
      .map_err(|_| DecisionSignalPortError::Conflict)?;
    self
      .commit(&snapshot, next)
      .await
      .map_err(|_| DecisionSignalPortError::Conflict)?;
    Ok(receipt)
  }
}
struct SelectionOnly;
#[async_trait]
impl FactoryNodeExecutor for SelectionOnly {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    Err(ApplicationError::invalid())
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    Err(ApplicationError::invalid())
  }
}
