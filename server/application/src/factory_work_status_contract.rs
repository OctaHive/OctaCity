//! Native Work-backed Reporter producer contract; delivery remains an independent outbox observation.
use crate::ApplicationError;
use octacity_server_factory::*;

/// Exact installed technical action metadata, independent of operator budgets and retry policy.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryWorkStatusActionContract {
  /// Exact installed Reporter capability.
  pub plugin: ImmutableReference,
  /// Native producer operation, distinct from provider status vocabulary.
  pub action: FactoryKey,
  /// Bounded native parameter grammar; current connection/effect ceilings remain separate choices.
  pub parameters: FactoryWorkStatusParameterContract,
  /// Closed accepted-operation receipt, constructed by the same native producer.
  pub output: FlowDataSchema,
  /// Acceptance means queued; it does not claim the provider applied the effect.
  pub outcome: FlowOutcomeDefinition,
}
/// Native queued producer requirements, projected from its existing validation owner.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryWorkStatusParameterContract {
  /// Versioned typed form corresponding to FactoryWorkStatusActionParameters in REST.
  pub contract: &'static str,
  /// UTF-8 ceiling of the independently configured local status.
  pub max_status_bytes: usize,
  /// Native installation requires exact Source/Reporter/connection pins.
  pub source_update_required: bool,
  /// Native installation requires the durable delivery outbox.
  pub outbox_required: bool,
  /// Inclusive bound from the existing retry policy.
  pub max_attempts: u16,
  /// Inclusive bound from the existing retry policy.
  pub max_retry_delay_millis: u64,
}
impl FactoryWorkStatusActionContract {
  /// Declares the owner-defined queued receipt for one explicitly installed capability.
  pub fn new(plugin: ImmutableReference) -> Result<Self, ApplicationError> {
    let key = |value| FactoryKey::new(value).map_err(|_| ApplicationError::invalid());
    let output = FlowDataSchema::new(
      key("work.status.receipt")?,
      key("v1")?,
      FlowValueSchema::Object {
        fields: [(
          key("receipt")?,
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        )]
        .into(),
      },
    )
    .map_err(|_| ApplicationError::invalid())?;
    Ok(Self {
      plugin,
      action: key("set_status")?,
      parameters: FactoryWorkStatusParameterContract {
        contract: "work_status_v1",
        max_status_bytes: MAX_FACTORY_SAFE_TEXT_BYTES,
        source_update_required: true,
        outbox_required: true,
        max_attempts: crate::FactoryReporterRetryPolicy::MAX_ATTEMPTS,
        max_retry_delay_millis: crate::FactoryReporterRetryPolicy::MAX_RETRY_DELAY_MILLIS,
      },
      outcome: FlowOutcomeDefinition::new(key("queued")?, FlowOutcomeKind::Success, output.reference().clone()),
      output,
    })
  }
}
