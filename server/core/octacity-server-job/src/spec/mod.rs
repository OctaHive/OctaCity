//! Server-owned construction of the signed Agent execution intent.

mod derive;
mod error;
mod model;
mod signer;

pub use derive::{derive_job_spec_template, derive_managed_job_spec_template, sign_ready_job_spec};
pub use error::JobSpecDerivationError;
pub use model::{
  DerivedJobSpec, JobExecutionTemplate, JobPlacementPolicy, JobRuntimePolicy, JobSpecBuildSnapshot,
  JobSpecPolicySnapshot, JobSpecTemplate, JobSpecToolchainPolicy, JobSpecValidity, MAX_JOB_SPEC_TOOLCHAIN_POLICY_BYTES,
  MAX_JOB_SPEC_VALIDITY_SECONDS, ManagedJobSpecIntent, SourcePluginPolicy,
};
pub use signer::{JobSpecSigner, JobSpecSigningError};
