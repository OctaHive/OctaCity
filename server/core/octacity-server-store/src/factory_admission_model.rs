use octacity_server_domain::{ProjectId, RepositoryId, RepositoryVersion, Timestamp};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, FactoryKey, FactoryRun,
  FactoryRunState, FactoryRunVersion, WorkEnvelope,
};
use serde::{Deserialize, Serialize};

use crate::{
  IdempotencyKey, ManagementSecurityScope, MutationDisposition, PublishedFactoryConfiguration, PublishedRepository,
  StoreError, StoreInputError, StoreOperation,
};

/// Provider-neutral namespace that scopes an external Work identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FactoryWorkSourceScope {
  /// Versioned Work Source adapter kind, such as the built-in `manual` source.
  pub source: FactoryKey,
  /// Accepted security partition for this source invocation.
  pub security_scope: ManagementSecurityScope,
}

/// Stable caller intent checked before any mutable source reference is resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryAdmissionProbe {
  /// Provider and security namespace of the external identity.
  pub source_scope: FactoryWorkSourceScope,
  /// Stable identity supplied by the Work Source.
  pub external_identity: ExternalWorkIdentity,
  /// Digest of every admission input except the later resolved revision.
  pub intent_digest: FactoryDigest,
  /// Transport replay identity supplied by the management client.
  pub idempotency_key: IdempotencyKey,
}

/// Exact immutable definitions required to validate and resolve one admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryAdmissionContext {
  /// Selected immutable Factory Configuration version.
  pub configuration: PublishedFactoryConfiguration,
  /// Selected immutable Repository version.
  pub repository: PublishedRepository,
}

/// Request for exact immutable admission definitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadFactoryAdmissionContext {
  /// Project that must own both definitions.
  pub project_id: ProjectId,
  /// Selected Factory Configuration identity.
  pub configuration_id: FactoryConfigurationId,
  /// Selected immutable Factory Configuration version.
  pub configuration_version: FactoryConfigurationVersion,
  /// Selected Repository identity.
  pub repository_id: RepositoryId,
  /// Selected immutable Repository version.
  pub repository_version: RepositoryVersion,
}

/// Atomic input that publishes one immutable Work Envelope and admitted Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitFactoryWork {
  /// Replay identity checked before VCS resolution.
  pub probe: FactoryAdmissionProbe,
  /// Exact Repository version used to resolve the immutable subject.
  pub repository_version: RepositoryVersion,
  /// Immutable normalized Work input.
  pub work: WorkEnvelope,
  /// Initial admitted Factory Run projection.
  pub run: FactoryRun,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
}

impl AdmitFactoryWork {
  /// Revalidates the complete initial admission relationship at the store seam.
  ///
  /// Mutable source resolution happens before this request exists, so adapters
  /// must accept only an admitted version-one Run that repeats the immutable
  /// Work identities and the authenticated source security scope exactly.
  pub fn validate(&self, security_scope: &ManagementSecurityScope) -> Result<(), StoreError> {
    if security_scope != &self.probe.source_scope.security_scope {
      return Err(StoreError::invalid(
        StoreOperation::AdmitFactoryWork,
        StoreInputError::InvalidMutationAuditContext,
      ));
    }
    let valid = self.work.external_identity() == &self.probe.external_identity
      && self.run.state() == FactoryRunState::Admitted
      && self.run.version() == FactoryRunVersion::INITIAL
      && self.run.configuration() == self.work.configuration()
      && self.run.work_id() == self.work.id()
      && self.run.subject() == self.work.subject();
    valid.then_some(()).ok_or_else(|| {
      StoreError::invalid(
        StoreOperation::AdmitFactoryWork,
        StoreInputError::InvalidFactoryAdmission,
      )
    })
  }
}

/// Immutable outcome of one accepted Work admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedFactoryAdmission {
  /// Immutable normalized Work input.
  pub work: WorkEnvelope,
  /// Initial durable Factory Run projection.
  pub run: FactoryRun,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
}

/// Result of applying or replaying one Work admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryAdmissionMutationOutcome {
  /// Whether this invocation applied the admission or observed its original result.
  pub disposition: MutationDisposition,
  /// Immutable result committed by the original admission.
  pub admission: PublishedFactoryAdmission,
}
