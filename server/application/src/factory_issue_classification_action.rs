//! Local classification can consume only an explicitly projected, accepted result.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::{
  ApplyIssueClassification, FactoryRunStore, IssueClassificationAuthority, IssueClassificationStore,
  IssueExecutionStore,
};
use std::{collections::BTreeMap, sync::Arc};

/// Installed trusted action with a host-selected principal and current management policy.
pub struct FactoryIssueClassificationAction<S, F> {
  identity: ImmutableReference,
  associations: Arc<dyn IssueExecutionStore>,
  runs: Arc<F>,
  commands: AuthorizedCommandHandler<IssueHandlers<S>>,
  context: ManagementRequestContext,
  clock: Arc<dyn FactoryExecutionClock>,
}

impl<S, F> FactoryIssueClassificationAction<S, F> {
  /// Binds the local Issue writer and retained Factory and association readers.
  pub fn new(
    identity: ImmutableReference,
    issues: Arc<S>,
    associations: Arc<dyn IssueExecutionStore>,
    runs: Arc<F>,
    context: ManagementRequestContext,
    policy: Arc<dyn ManagementAuthorizationPolicy>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    let commands = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(issues, policy)));
    Self {
      identity,
      associations,
      runs,
      commands,
      context,
      clock,
    }
  }
}

pub(super) const RECEIPT_EXAMPLE: FactoryDigest = FactoryDigest::from_bytes([0; 32]);

fn receipt_payload(schema: &FlowDataSchema, operation: FactoryDigest) -> Result<FlowPayload, ApplicationError> {
  FlowPayload::new(schema, serde_json::json!({"receipt":operation})).map_err(|_| ApplicationError::invalid())
}

pub(super) fn authoring_receipt() -> Result<(FlowDataSchema, u64), ApplicationError> {
  let key = |value| FactoryKey::new(value).map_err(|_| ApplicationError::invalid());
  let length = RECEIPT_EXAMPLE.to_string().len();
  let schema = FlowDataSchema::new(
    key("issue-classification.receipt")?,
    key("1")?,
    FlowValueSchema::Object {
      fields: [(
        key("receipt")?,
        FlowFieldSchema::required(FlowValueSchema::String {
          min_bytes: length,
          max_bytes: length,
        }),
      )]
      .into(),
    },
  )
  .map_err(|_| ApplicationError::invalid())?;
  let bytes =
    serde_json::to_vec(&receipt_payload(&schema, RECEIPT_EXAMPLE)?).map_err(|_| ApplicationError::invalid())?;
  Ok((schema, bytes.len() as u64))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
  pub(super) source_node: FactoryKey,
  pub(super) source_outcome: FactoryKey,
  outcome: FactoryKey,
  pub(super) policy: FactoryKey,
  pub(super) size_labels: BTreeMap<FactoryKey, FactoryKey>,
}

impl Selection {
  pub(super) fn from_binding(
    binding: &FlowActionBinding,
    outcomes: &[FlowOutcomeDefinition],
  ) -> Result<Self, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let value: Self = serde_json::from_value(binding.parameters().clone()).map_err(|_| invalid())?;
    if binding.action().as_str() != "apply_issue_classification"
      || value.size_labels.is_empty()
      || value.size_labels.len() > MAX_FACTORY_CLASSIFICATION_SIZE_LABELS
      || !outcomes
        .iter()
        .any(|outcome| outcome.key() == &value.outcome && outcome.kind() == FlowOutcomeKind::Success)
    {
      return Err(invalid());
    }
    Ok(value)
  }
  pub(super) fn receipt(
    &self,
    node: &FlowNodeDefinition,
    schemas: &[FlowDataSchema],
    operation: FactoryDigest,
  ) -> Result<FlowPayload, ApplicationError> {
    let outcome = node.outcome(&self.outcome).ok_or_else(ApplicationError::invalid)?;
    let schema = schemas
      .iter()
      .find(|schema| schema.reference() == outcome.schema())
      .ok_or_else(ApplicationError::invalid)?;
    receipt_payload(schema, operation)
  }
}

#[async_trait::async_trait]
impl<S: IssueClassificationStore, F: FactoryRunStore> FactoryFlowActionPlugin
  for FactoryIssueClassificationAction<S, F>
{
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }

  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.plugin() != &self.identity {
      return Err(ApplicationError::invalid());
    }
    Selection::from_binding(binding, outcomes).map(|_| ())
  }

  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    Ok(Some(BudgetUsage::default()))
  }

  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let invalid = ApplicationError::invalid;
    let run_id = request.input.factory_run_id();
    let snapshot = self.runs.factory_run_snapshot(run_id).await?;
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(request.input.definition())
      .ok_or_else(invalid)?;
    let node = definition.node(request.input.node()).ok_or_else(invalid)?;
    let attempt = snapshot
      .flow
      .attempts
      .iter()
      .find(|attempt| attempt.id() == request.node_attempt_id)
      .ok_or_else(invalid)?;
    if snapshot.work.id() != request.work_id
      || snapshot.work.subject() != request.subject
      || !snapshot.flow.data.inputs.contains(request.input)
      || attempt.input_digest() != request.input.digest().map_err(|_| invalid())?
      || node.action() != Some(request.binding)
      || request.binding.plugin() != &self.identity
      || !request.permissions.plugins().any(|plugin| plugin == &self.identity)
    {
      return Err(invalid());
    }
    let selected = Selection::from_binding(request.binding, node.outcomes())?;
    let records = &snapshot.flow.data.records;
    let mut matching = request.input.sources().iter().filter_map(|source| {
      records
        .iter()
        .find(|record| {
          record.node_attempt_id() == source.node_attempt_id()
            && record.input().node() == &selected.source_node
            && record.outcome() == &selected.source_outcome
            && record.input().definition() == request.input.definition()
            && record.input().flow_run_id() == request.input.flow_run_id()
            && record.input().cycle_id() == request.input.cycle_id()
            && record.digest().ok() == Some(source.digest())
        })
        .map(|record| (source.digest(), record))
    });
    let (record_digest, record) = matching.next().ok_or_else(invalid)?;
    let declared_source = definition
      .node(&selected.source_node)
      .and_then(|node| node.outcome(&selected.source_outcome))
      .ok_or_else(invalid)?;
    if matching.next().is_some()
      || declared_source.kind() != FlowOutcomeKind::Success
      || declared_source.schema() != record.payload().schema()
    {
      return Err(invalid());
    }
    let completion = snapshot
      .flow
      .completions
      .iter()
      .find(|completion| {
        completion.node_attempt_id() == record.node_attempt_id()
          && completion.output_digest() == record_digest
          && completion.outcome() == record.outcome()
          && completion.output_schema() == declared_source.schema()
      })
      .ok_or_else(invalid)?;
    let accepted = record.payload().value().as_object().ok_or_else(invalid)?;
    let projected = request.input.payload().value().as_object().ok_or_else(invalid)?;
    for field in ["priority", "size"] {
      if let Some(value) = projected.get(field)
        && accepted.get(field) != Some(value)
      {
        return Err(invalid());
      }
    }
    let priority = projected
      .get("priority")
      .and_then(serde_json::Value::as_u64)
      .and_then(|value| u8::try_from(value).ok())
      .filter(|value| *value <= MAX_WORK_PRIORITY);
    let size_label = projected
      .get("size")
      .and_then(serde_json::Value::as_str)
      .and_then(|value| selected.size_labels.iter().find(|(size, _)| size.as_str() == value))
      .map(|(_, label)| label.to_string());
    let project = request.subject.project_id();
    let association = self
      .associations
      .issue_work_association(project, request.work_id)
      .await?
      .filter(|a| a.run_id == run_id && a.requirements_version.is_some())
      .ok_or_else(ApplicationError::capability_unavailable)?;
    let operation_id = FactoryDigest::sha256(
      "octacity.issue-classification.v1",
      &[
        project.as_uuid().as_bytes(),
        association.issue_id.as_uuid().as_bytes(),
        selected.policy.as_str().as_bytes(),
        &record_digest.as_bytes(),
      ],
    );
    let payload = selected.receipt(node, request.schemas, operation_id)?;
    let output_bytes = serde_json::to_vec(&payload).map_err(|_| invalid())?.len() as u64;
    let budget_deadline = request
      .started_at
      .unix_millis()
      .checked_add(i64::try_from(request.budget.max_elapsed_millis()).map_err(|_| invalid())?)
      .and_then(|millis| octacity_server_domain::Timestamp::from_unix_millis(millis).ok())
      .ok_or_else(invalid)?;
    let deadline = request.deadline.min(request.authority_expires_at).min(budget_deadline);
    let applied_at = self.clock.now()?.max(request.observed_at);
    let usage_at = |now: octacity_server_domain::Timestamp| -> Result<BudgetUsage, ApplicationError> {
      let elapsed_millis = now
        .unix_millis()
        .checked_sub(request.started_at.unix_millis())
        .and_then(|elapsed| u64::try_from(elapsed).ok())
        .ok_or_else(invalid)?;
      let usage = BudgetUsage {
        output_bytes,
        elapsed_millis,
        ..Default::default()
      };
      usage.validate(request.budget).map_err(|_| invalid())?;
      if now >= deadline {
        return Err(invalid());
      }
      Ok(usage)
    };
    usage_at(applied_at)?;
    let latest_accepted =
      octacity_server_domain::Timestamp::from_unix_millis(deadline.unix_millis() - 1).map_err(|_| invalid())?;
    let remaining = u64::try_from(deadline.unix_millis() - applied_at.unix_millis()).map_err(|_| invalid())?;
    let claim = snapshot
      .current_claim
      .as_ref()
      .filter(|record| {
        record.claim.fence() == request.claim_fence
          && attempt
            .verify_observer(
              &FactoryClaimOwnership::new(record.owner.clone(), record.claim),
              applied_at,
            )
            .is_ok()
      })
      .ok_or_else(invalid)?;
    let authority = IssueClassificationAuthority {
      claim: claim.id,
      deadline: std::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(remaining))
        .ok_or_else(invalid)?,
    };
    self
      .commands
      .handle_command(
        &self.context,
        ApplyIssueClassificationCommand(ApplyIssueClassification {
          association,
          policy: selected.policy,
          record: record_digest,
          operation_id,
          accepted_at: completion.observed_at(),
          priority,
          size_label,
          managed_labels: selected.size_labels.values().map(ToString::to_string).collect(),
          applied_at,
          authority: Some(authority),
        }),
      )
      .await
      .map_err(|error| match error {
        ManagementHandlerError::Application(error) => error,
        ManagementHandlerError::Forbidden(_) => ApplicationError::capability_unavailable(),
      })?;
    Ok(FactoryFlowActionObservation {
      discovery: None,
      outcome: selected.outcome,
      payload,
      // An acknowledgement may arrive after the fenced transaction has committed.
      usage: usage_at(
        self
          .clock
          .now()
          .unwrap_or(applied_at)
          .max(applied_at)
          .min(latest_accepted),
      )?,
      outbox: vec![],
    })
  }
}
