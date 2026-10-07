//! Manual Trigger application use case and replaceable ports.

mod job_spec;
mod materialization;
mod model;
mod ports;
mod preparation;
mod retry;
mod service;

pub use model::{
  AcceptManualTriggerCommand, EffectiveProjectPolicySourceError, ManualSourceSelection, ManualTriggerCommand,
  ManualTriggerContext, ManualTriggerContextError, ManualTriggerError, ManualTriggerInputError, ManualTriggerOutcome,
  RepositorySourceSelection, RevisionResolutionError, RevisionResolutionRequest,
};
pub use octacity_server_job::JobSpecToolchainPolicy;
pub use ports::{
  EffectiveProjectPolicySource, ExactRevisionResolver, ManualTriggerContextProvider, RevisionResolver,
  StoreBackedEffectiveProjectPolicySource, StoreBackedManualTriggerContext,
};
pub(crate) use preparation::select_source;
pub use retry::{
  DurableManualTriggerService, ManualTriggerRetryBatchOutcome, ManualTriggerRetryWorker, ManualTriggerRetryWorkerError,
};
pub use service::ManualTriggerService;
