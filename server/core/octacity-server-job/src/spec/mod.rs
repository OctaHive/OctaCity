//! Server-owned construction of the signed Agent execution intent.

mod derive;
mod error;
mod model;
mod signer;

pub use derive::{derive_job_spec_template, sign_ready_job_spec};
pub use error::JobSpecDerivationError;
pub use model::{
  DerivedJobSpec, JobExecutionTemplate, JobPlacementPolicy, JobSpecBuildSnapshot, JobSpecPolicySnapshot,
  JobSpecTemplate, JobSpecValidity, MAX_JOB_SPEC_VALIDITY_SECONDS, SourcePluginPolicy,
};
pub use signer::{JobSpecSigner, JobSpecSigningError};
