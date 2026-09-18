//! Manual Trigger application use case and replaceable ports.

mod job_spec;
mod materialization;
mod model;
mod ports;
mod preparation;
mod service;

pub use model::{
  AcceptManualTriggerCommand, EffectiveProjectPolicySourceError, JobSpecToolchainPolicy, ManualSourceSelection,
  ManualTriggerCommand, ManualTriggerContext, ManualTriggerContextError, ManualTriggerError, ManualTriggerInputError,
  ManualTriggerOutcome, RevisionResolutionError, RevisionResolutionRequest,
};
pub use ports::{
  EffectiveProjectPolicySource, ManualTriggerContextProvider, RevisionResolver, StoreBackedManualTriggerContext,
};
pub use service::ManualTriggerService;
