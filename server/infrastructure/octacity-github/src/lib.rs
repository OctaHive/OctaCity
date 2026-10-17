//! One GitHub provider plugin, with capability-specific authority and shared transport.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod api;
mod candidate;
mod config;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod credential_files;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod credential_files_tests;
mod credential_management;
mod credential_publication;
pub use credential_management::{GithubCredentialClass, GithubCredentialManager, GithubCredentialTarget};
mod issue_pagination;
mod issue_reporter;
pub mod issue_selector;
mod issue_source;
mod protocol_io;
pub mod status_mapping;
mod work_api;
mod work_config;
mod work_protocol;
use config::Config;
use octacity_delivery_protocol::{Command, Failure, FailureClass, Outcome, Request};
pub use protocol_io::serve;
use std::path::Path;
use tokio_util::sync::CancellationToken;
pub use work_config::{AuthorizedIssueAccess, GithubWorkProvider};

/// One installed GitHub provider plugin. Capability roles share transport/configuration,
/// while the requested protocol and installed credential class remain authoritative.
pub struct GithubPlugin {
  config: Config,
}
/// Secret-free failure loading operator-owned provider configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("GitHub plugin configuration is invalid")]
pub struct GithubConfigError;
impl GithubPlugin {
  /// Loads strict operator configuration without reading a forge token.
  pub fn load(directory: &Path) -> Result<Self, GithubConfigError> {
    Ok(Self {
      config: Config::load(directory)?,
    })
  }
  /// Immutable provider profile identity; credential rotation is excluded.
  pub fn configuration_digest(&self) -> &str {
    &self.config.digest
  }
  /// Executes one scoped Delivery operation. Provider details never become diagnostics.
  pub async fn execute(&self, request: &Request, cancellation: CancellationToken) -> Outcome {
    if request.validate().is_err() {
      return failure(FailureClass::InvalidRequest);
    }
    if cancellation.is_cancelled() {
      return failure(FailureClass::Cancelled);
    }
    let intent = match &request.command {
      Command::Observe(intent) | Command::Publish(intent) => intent,
      Command::Cancel { target_delivery_id } => {
        return Outcome::Acknowledged {
          delivery_id: target_delivery_id.clone(),
        };
      }
    };
    if intent.configuration_digest != self.config.digest {
      return failure(FailureClass::Conflict);
    }
    if self.config.authorize_network(intent).is_err() {
      return failure(FailureClass::InvalidRequest);
    }
    let credential = match self.config.delivery_credential(intent) {
      Ok(credential) => credential,
      Err(_) => return failure(FailureClass::Permanent),
    };
    let operation = async {
      if !api::object_id(&intent.base_revision) || !api::object_id(&intent.candidate_revision) {
        return Err(FailureClass::InvalidRequest);
      }
      api::branch(&intent.target.base_reference)?;
      api::branch(&intent.target.candidate_reference)?;
      let repository = self
        .config
        .repositories
        .get(&intent.repository.repository_locator)
        .ok_or(FailureClass::Permanent)?;
      // The shared SDK client consumes a provider repository and scoped grant,
      // rather than a Delivery DTO; other capabilities reuse this transport.
      let api = api::GithubApi::new(&self.config, repository, &credential)?;
      match &request.command {
        Command::Observe(_) => api.inspect(intent).await.map(|observation| match observation {
          None => Outcome::Absent,
          Some(observation)
            if !observation.metadata_matches
              && observation
                .receipt
                .review
                .as_ref()
                .is_some_and(|review| review.state == octacity_delivery_protocol::ReviewState::Open) =>
          {
            Outcome::RepairRequired(Box::new(observation.receipt))
          }
          Some(observation) => Outcome::Observed(Box::new(observation.receipt)),
        }),
        Command::Publish(_) => {
          let candidate = candidate::Candidate::prepare(&self.config, &credential, intent, &cancellation).await?;
          let observed = api.inspect(intent).await?;
          if let Some(observation) = observed
            .as_ref()
            .filter(|observation| observation.receipt.review.is_some())
          {
            let review = observation.receipt.review.as_ref().ok_or(FailureClass::ProtocolFault)?;
            if review.state == octacity_delivery_protocol::ReviewState::Closed {
              return Err(FailureClass::Conflict);
            }
            if observation.metadata_matches || review.state == octacity_delivery_protocol::ReviewState::MergedExternally
            {
              return Ok(Outcome::Observed(Box::new(observation.receipt.clone())));
            }
            api.repair_review(intent, &review.identity).await?;
            let repaired = api.inspect(intent).await?.ok_or(FailureClass::Conflict)?;
            if !repaired.metadata_matches
              || repaired
                .receipt
                .review
                .as_ref()
                .is_none_or(|r| r.identity != review.identity)
            {
              return Err(FailureClass::Conflict);
            }
            return Ok(Outcome::Observed(Box::new(repaired.receipt)));
          }
          if observed.is_none() {
            candidate.publish_branch(intent).await?;
          }
          let observed = api.observe(intent).await?.ok_or(FailureClass::Conflict)?;
          if observed.review.is_none() {
            api.create_review(intent).await?;
          }
          let receipt = api.observe(intent).await?.ok_or(FailureClass::Conflict)?;
          if receipt.review.is_none() {
            return Err(FailureClass::ProtocolFault);
          }
          Ok(Outcome::Observed(Box::new(receipt)))
        }
        _ => Err(FailureClass::Unsupported),
      }
    };
    tokio::select! {
      biased;
      _ = cancellation.cancelled() => failure(FailureClass::Cancelled),
      result = operation => result.unwrap_or_else(failure),
    }
  }
}
fn failure(class: FailureClass) -> Outcome {
  Outcome::Failure(Failure {
    class,
    retry_after_millis: None,
  })
}

#[cfg(all(test, unix))]
mod tests;
