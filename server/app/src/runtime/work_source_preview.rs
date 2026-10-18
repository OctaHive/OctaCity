//! Read-only installed Source composition: no scan, admission, Artifact or task-card store.
use super::*;
use octacity_work_source_protocol::{Request, preview, source};
/// Explicit preview capability over verified installed Source profiles.
pub struct HostedFactoryWorkSourcePreview {
  registry: Arc<WorkAdapterRegistry>,
  profiles: Vec<FactorySourceProfile>,
  timeout: Duration,
  grace: Duration,
  cancellation: CancellationToken,
  connections: Option<ManagedIntegrationConnections>,
}
impl HostedFactoryWorkSourcePreview {
  /// Installs finite read bounds without starting a worker or contacting a provider.
  pub fn new(
    registry: Arc<WorkAdapterRegistry>,
    profiles: Vec<FactorySourceProfile>,
    timeout: Duration,
    grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<Self, ApplicationError> {
    if timeout.is_zero()
      || grace.is_zero()
      || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || profiles.iter().enumerate().any(|(i, p)| {
        profiles[..i]
          .iter()
          .any(|a| a.project_id == p.project_id && a.binding == p.binding)
      })
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      registry,
      profiles,
      timeout,
      grace,
      cancellation,
      connections: None,
    })
  }
  /// Resolves each managed input's own connection under its current read grant.
  pub fn with_managed_connections(mut self, connections: ManagedIntegrationConnections) -> Self {
    self.connections = Some(connections);
    self
  }
}
fn dependency(class: WorkSourceFailure) -> ApplicationError {
  ApplicationError::SourceDependency {
    class,
    retry_after_millis: None,
  }
}
#[async_trait::async_trait]
impl FactoryWorkSourcePreview for HostedFactoryWorkSourcePreview {
  async fn preview(&self, request: ReadWorkSourcePreview) -> Result<WorkSourcePreviewPage, ApplicationError> {
    if request.inputs.is_empty()
      || request.inputs.len() > 64
      || !(1..=MAX_WORK_SOURCE_PREVIEW_ITEMS).contains(&request.limit)
    {
      return Err(ApplicationError::invalid());
    }
    let deadline = tokio::time::Instant::now() + self.timeout;
    let prepare = async {
      let mut profiles = vec![];
      for input in &request.inputs {
        let profile = self
          .profiles
          .iter()
          .find(|p| p.project_id == request.project_id && p.binding == input.definition.source)
          .cloned()
          .ok_or_else(|| dependency(WorkSourceFailure::Unavailable))?
          .for_definition(&input.definition, self.connections.as_ref())?;
        profile.authorize_access().await?;
        profiles.push(profile);
      }
      Ok::<_, ApplicationError>(profiles)
    };
    let profiles = tokio::select! {
      _=self.cancellation.cancelled()=>return Err(dependency(WorkSourceFailure::Cancelled)),
      result=tokio::time::timeout_at(deadline,prepare)=>result.map_err(|_|dependency(WorkSourceFailure::Timeout))??,
    };
    let profile = &profiles[0];
    if profiles.iter().any(|p| {
      p.plugin != profile.plugin
        || p.provider_configuration != profile.provider_configuration
        || p.resource != profile.resource
        || p.network_hosts != profile.network_hosts
        || p.context.security_scope() != profile.context.security_scope()
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
    let bounds = common_polling(&profiles)?;
    let id = uuid::Uuid::new_v4();
    let wire = Request::Source(source::Request {
      protocol_version: source::PROTOCOL_VERSION,
      request_id: id.to_string(),
      command: source::Command::Preview(Box::new(source::ScanIntent {
        scan_id: FactoryDigest::sha256("octacity.factory.preview.v1", &[id.as_bytes()]).to_string(),
        configuration_digest: profile.provider_configuration.to_string(),
        resource: profile.resource.clone(),
        selection: primary,
        additional_selections: selections,
        network_hosts: profile.network_hosts.clone(),
        cursor: None,
        window: None,
        max_items: request.limit.min(bounds.page_items).min(bounds.max_items),
        max_snapshot_bytes: MAX_SOURCE_INPUT_BYTES,
        max_page_bytes: MAX_SOURCE_INPUT_BYTES.min(bounds.max_page_bytes).min(bounds.max_bytes),
      })),
    });
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
      return Err(dependency(WorkSourceFailure::Timeout));
    }
    let source::Outcome::Previewed(page) = process::execute_source(
      &self.registry,
      profile,
      wire,
      remaining,
      self.grace,
      self.cancellation.clone(),
    )
    .await?
    else {
      return Err(dependency(WorkSourceFailure::ProtocolFault));
    };
    let candidates = page
      .candidates
      .into_iter()
      .map(|candidate| {
        let selections = request
          .inputs
          .iter()
          .zip(&profiles)
          .map(|(input, profile)| {
            let selected = candidate
              .selections
              .iter()
              .find(|s| s.selection == profile.selection)
              .ok_or_else(|| dependency(WorkSourceFailure::ProtocolFault))?;
            let mut reasons: Vec<_> = selected
              .reasons
              .iter()
              .map(|reason| match reason {
                preview::SelectionMismatch::StateMismatch => WorkSourceSelectionMismatch::StateMismatch,
                preview::SelectionMismatch::RequiredLabelMissing => WorkSourceSelectionMismatch::RequiredLabelMissing,
                preview::SelectionMismatch::AlternativeLabelMissing => {
                  WorkSourceSelectionMismatch::AlternativeLabelMissing
                }
                preview::SelectionMismatch::ExcludedLabelPresent => WorkSourceSelectionMismatch::ExcludedLabelPresent,
                preview::SelectionMismatch::PredicateMismatch => WorkSourceSelectionMismatch::PredicateMismatch,
              })
              .collect();
            if let Some(predicate) = &input.definition.predicate {
              for reason in predicate.mismatches(&candidate.state, &candidate.labels) {
                if !reasons.contains(&reason) {
                  reasons.push(reason);
                }
              }
            }
            Ok(WorkSourcePreviewSelection {
              input_id: input.id,
              reasons,
            })
          })
          .collect::<Result<Vec<_>, ApplicationError>>()?;
        Ok(WorkSourcePreviewItem {
          item: WorkSourceItem {
            resource: ExternalWorkIdentity::new(page.resource_id.clone())
              .map_err(|_| dependency(WorkSourceFailure::ProtocolFault))?,
            item: ExternalWorkIdentity::new(candidate.item_id)
              .map_err(|_| dependency(WorkSourceFailure::ProtocolFault))?,
          },
          display_id: candidate.display_id,
          title: candidate.title,
          state: candidate.state,
          labels: candidate.labels,
          selections,
        })
      })
      .collect::<Result<Vec<_>, ApplicationError>>()?;
    Ok(WorkSourcePreviewPage {
      security_scope: profile.context.security_scope().clone(),
      candidates,
      has_more: page.next_cursor.is_some(),
    })
  }
}
