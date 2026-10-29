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
  /// Closed accepted-operation receipt, constructed by the same native producer.
  pub output: FlowDataSchema,
  /// Acceptance means queued; it does not claim the provider applied the effect.
  pub outcome: FlowOutcomeDefinition,
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
      outcome: FlowOutcomeDefinition::new(key("queued")?, FlowOutcomeKind::Success, output.reference().clone()),
      output,
    })
  }
}
