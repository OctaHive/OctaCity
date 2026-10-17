//! Optional read-only Source/Reporter check composition; no Factory, task or cursor store is held.
use octacity_server_application::{
  ApplicationError, IntegrationCheckReadiness, IntegrationCheckState, IntegrationConnectionChecker,
};
use octacity_server_factory::{FactoryDigest, ImmutableReference};
use octacity_server_store::{
  IntegrationCapability, IntegrationPurpose, IntegrationResource, PublishedIntegrationConnection,
};
use octacity_server_work_source::{HostFailureClass, WorkAdapterRegistry};
use octacity_work_source_protocol::{
  FailureClass, Request, ResourceAccess, Response,
  check::{CheckIntent, CheckState},
  reporter, source,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

/// Trusted installed check material, independent of a Project, pipeline, input or task card.
#[derive(Clone, Debug)]
pub struct IntegrationCheckProfile {
  /// Original immutable installation namespace.
  pub installation: ImmutableReference,
  /// Exact installed native resource definition.
  pub resource_reference: ImmutableReference,
  /// Original purpose/configuration/credential-reference selection.
  pub capability: IntegrationCapability,
  /// Content-verified installed executable.
  pub plugin: ImmutableReference,
  /// Original private provider profile digest.
  pub provider_configuration: FactoryDigest,
  /// Exact installed native resource alias and purpose-specific opaque handle.
  pub resource: ResourceAccess,
  /// Trusted installed network ceiling, never derived from provider data.
  pub network_hosts: Vec<String>,
}
type ProfileKey = (
  ImmutableReference,
  ImmutableReference,
  IntegrationPurpose,
  ImmutableReference,
  ImmutableReference,
);
/// Uses explicit installed check operations; unsupported VCS/Delivery checks never borrow task credentials.
pub struct HostedIntegrationConnectionChecker {
  registry: Arc<WorkAdapterRegistry>,
  profiles: BTreeMap<ProfileKey, IntegrationCheckProfile>,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
}
impl HostedIntegrationConnectionChecker {
  /// Installs trusted profiles and finite server-owned process bounds, without starting any worker.
  pub fn new(
    registry: Arc<WorkAdapterRegistry>,
    profiles: Vec<IntegrationCheckProfile>,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, ApplicationError> {
    if timeout.is_zero() || grace.is_zero() {
      return Err(ApplicationError::invalid());
    }
    let mut installed = BTreeMap::new();
    for profile in profiles {
      let key = (
        profile.installation.clone(),
        profile.resource_reference.clone(),
        profile.capability.purpose,
        profile.capability.capability.clone(),
        profile.capability.configuration.clone(),
      );
      if installed.insert(key, profile).is_some() {
        return Err(ApplicationError::invalid());
      }
    }
    Ok(Self {
      registry,
      profiles: installed,
      timeout,
      grace,
      cancellation,
    })
  }
}
#[async_trait::async_trait]
impl IntegrationConnectionChecker for HostedIntegrationConnectionChecker {
  async fn check(
    &self,
    connection: &PublishedIntegrationConnection,
    resource: &IntegrationResource,
    purpose: IntegrationPurpose,
  ) -> Result<IntegrationCheckReadiness, ApplicationError> {
    let mut readiness = IntegrationCheckReadiness::unverified();
    if !matches!(purpose, IntegrationPurpose::TaskRead | IntegrationPurpose::TaskUpdate) {
      readiness.capability = IntegrationCheckState::Unsupported;
      return Ok(readiness);
    }
    if !connection.definition.resources.contains(resource) {
      readiness.configuration = IntegrationCheckState::Invalid;
      return Ok(readiness);
    }
    let Some(capability) = resource.capabilities.iter().find(|c| c.purpose == purpose) else {
      readiness.capability = IntegrationCheckState::Unsupported;
      return Ok(readiness);
    };
    let Some(profile) = self.profiles.get(&(
      connection.definition.installation.clone(),
      resource.reference.clone(),
      purpose,
      capability.capability.clone(),
      capability.configuration.clone(),
    )) else {
      readiness.configuration = IntegrationCheckState::Missing;
      return Ok(readiness);
    };
    if profile.plugin != connection.definition.adapter
      || profile.capability != *capability
      || profile.resource.resource_id != resource.native_id.as_str()
    {
      readiness.configuration = IntegrationCheckState::Conflict;
      return Ok(readiness);
    }
    let adapter = match self
      .registry
      .resolve(profile.plugin.identity().as_str(), &profile.plugin.digest().to_string())
    {
      Ok(adapter) => adapter,
      Err(_) => {
        readiness.configuration = IntegrationCheckState::Missing;
        return Ok(readiness);
      }
    };
    let id = uuid::Uuid::new_v4();
    let intent = CheckIntent {
      check_id: FactoryDigest::sha256("octacity.integration-check.v1", &[id.as_bytes()]).to_string(),
      configuration_digest: profile.provider_configuration.to_string(),
      endpoint: connection.definition.endpoint.clone(),
      resource: profile.resource.clone(),
      network_hosts: profile.network_hosts.clone(),
    };
    let request = match purpose {
      IntegrationPurpose::TaskRead => Request::Source(source::Request {
        protocol_version: source::PROTOCOL_VERSION,
        request_id: id.to_string(),
        command: source::Command::Check(intent),
      }),
      IntegrationPurpose::TaskUpdate => {
        let Some(range) = adapter.manifest.protocol.reporter else {
          readiness.capability = IntegrationCheckState::Unsupported;
          return Ok(readiness);
        };
        let version = match (octacity_work_source_protocol::ProtocolRange {
          min: reporter::PROTOCOL_VERSION,
          max: reporter::ORDERED_PROTOCOL_VERSION,
        })
        .negotiate(range)
        {
          Ok(v) => v,
          Err(_) => {
            readiness.capability = IntegrationCheckState::Unsupported;
            return Ok(readiness);
          }
        };
        Request::Reporter(reporter::Request {
          protocol_version: version,
          request_id: id.to_string(),
          command: reporter::Command::Check(intent),
        })
      }
      _ => unreachable!(),
    };
    let report = match adapter
      .execute(request, self.timeout, self.grace, self.cancellation.child_token())
      .await
    {
      Ok(Response::Source(source::Response {
        outcome: source::Outcome::Checked(report),
        ..
      }))
      | Ok(Response::Reporter(reporter::Response {
        outcome: reporter::Outcome::Checked(report),
        ..
      })) => report,
      Ok(Response::Source(source::Response {
        outcome: source::Outcome::Failure(failure),
        ..
      }))
      | Ok(Response::Reporter(reporter::Response {
        outcome: reporter::Outcome::Failure(failure),
        ..
      })) => {
        readiness.retry_after_millis = failure.retry_after_millis;
        readiness.capability = match failure.class {
          FailureClass::Transient if failure.retry_after_millis.is_some() => IntegrationCheckState::Throttled,
          FailureClass::Transient | FailureClass::Cancelled => IntegrationCheckState::Unavailable,
          FailureClass::Unsupported => IntegrationCheckState::Unsupported,
          FailureClass::Conflict => IntegrationCheckState::Conflict,
          FailureClass::Permanent => IntegrationCheckState::Denied,
          FailureClass::InvalidRequest | FailureClass::ProtocolFault => IntegrationCheckState::Invalid,
        };
        return Ok(readiness);
      }
      Err(error) => {
        match error.class() {
          HostFailureClass::Unsupported => readiness.capability = IntegrationCheckState::Unsupported,
          HostFailureClass::InvalidRequest | HostFailureClass::Permanent | HostFailureClass::ProtocolFault => {
            readiness.configuration = IntegrationCheckState::Invalid;
          }
          HostFailureClass::Transient | HostFailureClass::Cancelled => {
            readiness.capability = IntegrationCheckState::Unavailable
          }
        }
        return Ok(readiness);
      }
      _ => {
        readiness.configuration = IntegrationCheckState::Invalid;
        return Ok(readiness);
      }
    };
    Ok(IntegrationCheckReadiness {
      configuration: state(report.configuration),
      authentication: state(report.authentication),
      resource_access: state(report.resource_access),
      capability: state(report.capability),
      retry_after_millis: report.retry_after_millis,
    })
  }
}
fn state(value: CheckState) -> IntegrationCheckState {
  match value {
    CheckState::Ready => IntegrationCheckState::Ready,
    CheckState::Missing => IntegrationCheckState::Missing,
    CheckState::Invalid => IntegrationCheckState::Invalid,
    CheckState::Denied => IntegrationCheckState::Denied,
    CheckState::Conflict => IntegrationCheckState::Conflict,
    CheckState::Unsupported => IntegrationCheckState::Unsupported,
    CheckState::Unverified => IntegrationCheckState::Unverified,
    CheckState::Throttled => IntegrationCheckState::Throttled,
    CheckState::Unavailable => IntegrationCheckState::Unavailable,
  }
}
