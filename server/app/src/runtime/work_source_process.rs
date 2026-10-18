//! Verified process translation and safe provider failure mapping.
use super::*;
pub(super) struct SourcePageBridge<S> {
  pub(super) managed_connections: Option<ManagedIntegrationConnections>,
  pub(super) store: Arc<S>,
  pub(super) registry: Arc<WorkAdapterRegistry>,
  pub(super) profiles: Vec<FactorySourceProfile>,
  pub(super) clock: Arc<dyn FactoryExecutionClock>,
  pub(super) settings: FactorySourceWorkerSettings,
  pub(super) cancellation: CancellationToken,
}
#[async_trait::async_trait]
impl<S: FactorySourceStore> FactorySourcePageReader for SourcePageBridge<S> {
  async fn read(&self, request: SourcePollRequest) -> Result<SourcePollPage, ApplicationError> {
    use octacity_work_source_protocol::{Request, source};
    let members = self
      .store
      .source_scan_members(request.claim.clone(), self.clock.now()?)
      .await?;
    let profiles = profiles_for(&self.profiles, &members, self.managed_connections.as_ref())?;
    for profile in &profiles {
      profile.authorize_access().await?;
    }
    let profile = &profiles[0];
    if profiles.iter().any(|p| {
      p.plugin != profile.plugin
        || p.provider_configuration != profile.provider_configuration
        || p.resource != profile.resource
        || p.network_hosts != profile.network_hosts
    }) {
      return Err(ApplicationError::invalid());
    }
    let mut selections = vec![profile.selection.clone()];
    for p in &profiles {
      if !selections.contains(&p.selection) {
        selections.push(p.selection.clone());
      }
    }
    let primary = selections.remove(0);
    let window = request
      .window
      .map(|(from, until)| {
        Ok::<_, ApplicationError>(source::ScanWindow {
          from_millis: u64::try_from(from.unix_millis()).map_err(|_| ApplicationError::invalid())?,
          until_millis: u64::try_from(until.unix_millis()).map_err(|_| ApplicationError::invalid())?,
        })
      })
      .transpose()?;
    let wire = Request::Source(source::Request {
      protocol_version: source::PROTOCOL_VERSION,
      request_id: format!("source-{}", uuid::Uuid::new_v4()),
      command: source::Command::Scan(Box::new(source::ScanIntent {
        scan_id: request.scan_id.to_string(),
        configuration_digest: profile.provider_configuration.to_string(),
        resource: profile.resource.clone(),
        selection: primary.clone(),
        additional_selections: selections,
        network_hosts: profile.network_hosts.clone(),
        cursor: request.continuation,
        window,
        max_items: request.max_items,
        max_snapshot_bytes: MAX_SOURCE_INPUT_BYTES,
        max_page_bytes: request.max_page_bytes,
      })),
    });
    let source::Outcome::Page(page) = execute_source(
      &self.registry,
      profile,
      wire,
      self.settings.operation_timeout,
      self.settings.cancellation_grace,
      self.cancellation.clone(),
    )
    .await?
    else {
      return Err(ApplicationError::capability_unavailable());
    };
    let candidates = page
      .candidates
      .into_iter()
      .enumerate()
      .map(|(index, candidate)| {
        Ok(WorkSourceSnapshot {
          selection_matches: page
            .matching_selections
            .get(index)
            .cloned()
            .unwrap_or_else(|| vec![primary.clone()])
            .iter()
            .map(selection_digest)
            .collect::<Result<_, _>>()?,
          item: WorkSourceItem {
            resource: ExternalWorkIdentity::new(candidate.container_id).map_err(|_| ApplicationError::invalid())?,
            item: ExternalWorkIdentity::new(candidate.item_id).map_err(|_| ApplicationError::invalid())?,
          },
          display_id: candidate.display_id,
          update_identity: candidate.update_identity,
          title: candidate.title,
          body: candidate.body,
          fields: candidate.fields,
          labels: candidate.labels,
          state: candidate.state,
        })
      })
      .collect::<Result<Vec<_>, ApplicationError>>()?;
    Ok(SourcePollPage {
      candidates,
      next_cursor: page.next_cursor,
    })
  }
}

pub(super) async fn execute_source(
  registry: &WorkAdapterRegistry,
  profile: &FactorySourceProfile,
  wire: octacity_work_source_protocol::Request,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
) -> Result<octacity_work_source_protocol::source::Outcome, ApplicationError> {
  use octacity_work_source_protocol::{Response, source};
  let adapter = registry
    .resolve(profile.plugin.identity().as_str(), &profile.plugin.digest().to_string())
    .map_err(|_| ApplicationError::capability_unavailable())?;
  let response = adapter
    .execute(wire, timeout, grace, cancellation)
    .await
    .map_err(|error| {
      use octacity_server_work_source::{HostError, HostFailureClass};
      let class = match error {
        HostError::TimedOut { .. } => WorkSourceFailure::Timeout,
        _ => match error.class() {
          HostFailureClass::Cancelled => WorkSourceFailure::Cancelled,
          HostFailureClass::Unsupported => WorkSourceFailure::Unavailable,
          HostFailureClass::ProtocolFault => WorkSourceFailure::ProtocolFault,
          HostFailureClass::Permanent | HostFailureClass::InvalidRequest => WorkSourceFailure::InvalidConfiguration,
          HostFailureClass::Transient => WorkSourceFailure::Transient,
        },
      };
      ApplicationError::SourceDependency {
        class,
        retry_after_millis: None,
      }
    })?;
  if let Response::Source(source::Response {
    outcome: source::Outcome::Failure(failure),
    ..
  }) = &response
  {
    use octacity_work_source_protocol::FailureClass;
    let class = match failure.class {
      FailureClass::Transient if failure.retry_after_millis.is_some() => WorkSourceFailure::RateLimited,
      FailureClass::Transient => WorkSourceFailure::Transient,
      FailureClass::Unsupported => WorkSourceFailure::Unavailable,
      FailureClass::Conflict => WorkSourceFailure::Conflict,
      FailureClass::Cancelled => WorkSourceFailure::Cancelled,
      FailureClass::ProtocolFault => WorkSourceFailure::ProtocolFault,
      FailureClass::Permanent | FailureClass::InvalidRequest => WorkSourceFailure::InvalidConfiguration,
    };
    return Err(ApplicationError::SourceDependency {
      class,
      retry_after_millis: failure.retry_after_millis,
    });
  }
  let Response::Source(response) = response else {
    return Err(ApplicationError::capability_unavailable());
  };
  Ok(response.outcome)
}
