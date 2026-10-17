//! Bounded GET-only diagnostics; no selectors, task snapshots or mutation mappings are consumed.
use super::*;
use octacity_work_source_protocol::check::{CheckIntent, CheckReport, CheckState};

impl GithubWorkProvider {
  pub(super) async fn check_connection(
    &self,
    request: &Request,
    intent: &CheckIntent,
    cancellation: tokio_util::sync::CancellationToken,
  ) -> CheckReport {
    tokio::select! {
      biased;
      _ = cancellation.cancelled() => {
        let mut report = CheckReport::unverified(intent); report.capability = CheckState::Unavailable; report
      }
      report = self.read_connection(request, intent) => report,
    }
  }
  async fn read_connection(&self, request: &Request, intent: &CheckIntent) -> CheckReport {
    let mut report = CheckReport::unverified(intent);
    if intent.endpoint != self.api.base.as_str() {
      report.configuration = CheckState::Invalid;
      return report;
    }
    let (resource, class) = match self.validate_scope(request) {
      Ok(scope) => scope,
      Err(FailureClass::Unsupported) => {
        report.configuration = CheckState::Ready;
        report.capability = CheckState::Unsupported;
        return report;
      }
      Err(_) => {
        report.configuration = CheckState::Invalid;
        return report;
      }
    };
    report.configuration = CheckState::Ready;
    let credential = match self.credential_for(resource, class) {
      Ok(credential) => credential,
      Err(_) => {
        report.authentication = CheckState::Missing;
        return report;
      }
    };
    let result = async {
      let repository = self
        .repositories
        .get(&intent.resource.resource_locator)
        .ok_or(FailureClass::InvalidRequest)?;
      let client = crate::api::client(
        &self.api.base,
        &self.api.version,
        &self.api.root_certificates,
        &credential,
      )
      .map_err(|_| FailureClass::Permanent)?;
      let mut url = self.api.base.clone();
      url
        .path_segments_mut()
        .map_err(|_| FailureClass::Permanent)?
        .pop_if_empty()
        .push("repos")
        .push(&repository.owner)
        .push(&repository.name);
      let (metadata, _) = crate::work_api::read(&client, &url, &self.api.version).await?;
      report.authentication = CheckState::Ready;
      let native = metadata
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or(FailureClass::ProtocolFault)?;
      if native.to_string() != intent.resource.resource_id {
        report.resource_access = CheckState::Conflict;
        return Ok(());
      }
      report.resource_access = CheckState::Ready;
      if matches!(request, Request::Reporter(_)) {
        return Ok(());
      }
      url
        .path_segments_mut()
        .map_err(|_| FailureClass::Permanent)?
        .push("issues");
      url
        .query_pairs_mut()
        .append_pair("state", "all")
        .append_pair("per_page", "1");
      let (issues, _) = crate::work_api::read(&client, &url, &self.api.version).await?;
      if !issues.as_array().is_some_and(|items| items.len() <= 1) {
        return Err(FailureClass::ProtocolFault.into());
      }
      report.capability = CheckState::Ready;
      Ok::<(), crate::work_api::WorkApiFailure>(())
    }
    .await;
    if let Err(error) = result {
      report.retry_after_millis = error.retry_after_millis;
      if error.status == Some(401) {
        report.authentication = CheckState::Denied;
        report.resource_access = CheckState::Unverified;
        report.capability = CheckState::Unverified;
      } else {
        let state = if error.retry_after_millis.is_some() {
          CheckState::Throttled
        } else {
          match error.class {
            FailureClass::Transient | FailureClass::Cancelled => CheckState::Unavailable,
            FailureClass::Conflict => CheckState::Conflict,
            FailureClass::ProtocolFault => CheckState::Invalid,
            _ => CheckState::Denied,
          }
        };
        if report.resource_access == CheckState::Ready {
          report.capability = state;
        } else {
          report.resource_access = state;
        }
      }
    }
    report
  }
}
