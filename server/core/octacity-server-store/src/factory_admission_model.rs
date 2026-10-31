use octacity_server_domain::{ProjectId, RepositoryId, RepositoryVersion, Timestamp};
use octacity_server_factory::{
  AdmittedFlow, ExternalWorkIdentity, FactoryConfiguration, FactoryConfigurationId, FactoryConfigurationVersion,
  FactoryDigest, FactoryKey, FactoryRun, FactoryRunState, FactoryRunVersion, WorkEnvelope,
};
use serde::{Deserialize, Serialize};

use crate::{
  IdempotencyKey, ManagementSecurityScope, MutationDisposition, PublishedFactoryConfiguration, PublishedRepository,
  StoreError, StoreInputError, StoreOperation,
};

/// Provider-neutral namespace that scopes an external Work identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FactoryWorkSourceScope {
  /// Application-owned Work Source namespace, such as the built-in `manual` source.
  pub source: FactoryKey,
  /// Accepted security partition for this source invocation.
  pub security_scope: ManagementSecurityScope,
}

/// Read-only identity lookup in the existing authoritative admission namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryAdmissionIdentity {
  /// Trusted application-resolved source and security partition.
  pub source_scope: FactoryWorkSourceScope,
  /// Stable input-scoped item identity, independent of mutable observations.
  pub external_identity: ExternalWorkIdentity,
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
  /// Transport replay identity normalized by the application ingress.
  pub idempotency_key: IdempotencyKey,
}

/// Exact immutable definitions required to validate and resolve one admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryAdmissionContext {
  /// Selected immutable Factory Configuration version.
  pub configuration: PublishedFactoryConfiguration,
  /// Selected Repository version; absent for Project/input admission.
  pub repository: Option<PublishedRepository>,
}

/// Request for exact immutable admission definitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadFactoryAdmissionContext {
  /// Project that must own both definitions.
  pub project_id: ProjectId,
  /// Selected Factory Configuration identity.
  pub configuration_id: FactoryConfigurationId,
  /// Selected immutable Factory Configuration version.
  pub configuration_version: FactoryConfigurationVersion,
  /// Selected Repository identity; both repository fields must be present or absent.
  pub repository_id: Option<RepositoryId>,
  /// Selected Repository version; absent for Project/input admission.
  pub repository_version: Option<RepositoryVersion>,
}

/// Atomic input that publishes one immutable Work Envelope and admitted Run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitFactoryWork {
  /// Replay identity checked before VCS resolution.
  pub probe: FactoryAdmissionProbe,
  /// Exact Repository version; absent only when Work has no repository subject.
  pub repository_version: Option<RepositoryVersion>,
  /// Immutable normalized Work input.
  pub work: WorkEnvelope,
  /// Initial admitted Factory Run projection.
  pub run: FactoryRun,
  /// Exact Flow Definition closure and initial root execution records.
  pub flow: AdmittedFlow,
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
    let valid = self.repository_version.is_some() == self.work.subject().repository().is_ok()
      && self.work.external_identity() == &self.probe.external_identity
      && initial_admission_matches(&self.work, &self.run, &self.flow);
    valid.then_some(()).ok_or_else(|| {
      StoreError::invalid(
        StoreOperation::AdmitFactoryWork,
        StoreInputError::InvalidFactoryAdmission,
      )
    })
  }

  /// Verifies that the admitted closure is exactly the one selected by the
  /// immutable configuration version, rather than a caller-supplied variant.
  pub fn validate_configuration(&self, configuration: &FactoryConfiguration) -> Result<(), StoreError> {
    self
      .work
      .profile_overrides()
      .validate(&configuration.flow().closure, self.work.subject().project_id())
      .map_err(|_| {
        StoreError::invalid(
          StoreOperation::AdmitFactoryWork,
          StoreInputError::InvalidFactoryAdmission,
        )
      })?;
    let expected = AdmittedFlow::from_configuration(configuration, &self.run)
      .and_then(|flow| flow.with_code_pins(self.flow.code_pins().to_vec(), self.work.subject().project_id()))
      .map_err(|_| {
        StoreError::invalid(
          StoreOperation::AdmitFactoryWork,
          StoreInputError::InvalidFactoryAdmission,
        )
      })?;
    (self.flow == expected).then_some(()).ok_or_else(|| {
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
  /// Exact immutable Flow admission pinned with the Run.
  pub flow: AdmittedFlow,
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

// Both newly admitted input and a retained receipt obey one original Work/Run relationship.
fn initial_admission_matches(work: &WorkEnvelope, run: &FactoryRun, flow: &AdmittedFlow) -> bool {
  run.state() == FactoryRunState::Admitted
    && run.version() == FactoryRunVersion::INITIAL
    && run.configuration() == work.configuration()
    && run.work_id() == work.id()
    && run.subject() == work.subject()
    && flow.root_run().factory_run_id() == run.id()
    && flow.root_run().definition() == flow.closure().root()
    && flow.initial_cycle().flow_run_id() == flow.root_run().id()
}
impl PublishedFactoryAdmission {
  /// Revalidates the original immutable admission receipt, never a later Run projection.
  pub fn validate(&self) -> Result<(), StoreError> {
    initial_admission_matches(&self.work, &self.run, &self.flow)
      .then_some(())
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::AdmitFactoryWork,
          StoreInputError::InvalidFactoryAdmission,
        )
      })
  }
}
