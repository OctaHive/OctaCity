//! Manual Trigger application use case and replaceable ports.

mod job_spec;
mod materialization;
mod model;
mod ports;
mod preparation;
mod retry;
mod service;

pub use model::{
  AcceptManualTriggerCommand, EffectiveProjectPolicySourceError, JobSpecToolchainPolicy, ManualSourceSelection,
  ManualTriggerCommand, ManualTriggerContext, ManualTriggerContextError, ManualTriggerError, ManualTriggerInputError,
  ManualTriggerOutcome, RevisionResolutionError, RevisionResolutionRequest,
};
pub use ports::{
  EffectiveProjectPolicySource, ExactRevisionResolver, ManualTriggerContextProvider, RevisionResolver,
  StoreBackedEffectiveProjectPolicySource, StoreBackedManualTriggerContext,
};
pub use retry::{
  DurableManualTriggerService, ManualTriggerRetryBatchOutcome, ManualTriggerRetryWorker, ManualTriggerRetryWorkerError,
};
pub use service::ManualTriggerService;
