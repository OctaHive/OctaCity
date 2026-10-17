//! Independent installed Issue scopes; no Delivery material or Git executable is required.
use crate::{
  GithubConfigError,
  config::{self, ApiSettings, Credential},
};
use octacity_work_source_protocol::{FailureClass, Request, reporter, source};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Repository {
  repository_id: String,
  owner: String,
  name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capabilities {
  #[serde(default)]
  source_repositories: Vec<String>,
  #[serde(default)]
  reporter_repositories: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
  config_version: u16,
  api_base: String,
  api_version: String,
  #[serde(default)]
  api_root_certificates_pem: Vec<String>,
  credential_directory: PathBuf,
  selection_directory: Option<PathBuf>,
  mapping_directory: Option<PathBuf>,
  capabilities: Capabilities,
  repositories: BTreeMap<String, Repository>,
}
/// The Issue capabilities of the same GitHub provider binary, independently installed from Delivery.
pub struct GithubWorkProvider {
  directory: PathBuf,
  digest: String,
  api: ApiSettings,
  credential_directory: PathBuf,
  selection_directory: Option<PathBuf>,
  mapping_directory: Option<PathBuf>,
  repositories: BTreeMap<String, Repository>,
  capabilities: Capabilities,
}
/// Validated native Issue scope with a private, zeroing grant retained for provider use.
/// No public API exposes token bytes, caller paths or arbitrary provider destinations.
pub struct AuthorizedIssueAccess {
  resource_id: String,
  credential: Credential,
}
impl AuthorizedIssueAccess {
  /// Returns the exact native ticket-container identity, independent of the code Repository.
  pub fn resource_id(&self) -> &str {
    &self.resource_id
  }
}
impl std::fmt::Debug for AuthorizedIssueAccess {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AuthorizedIssueAccess")
      .field("credential", &self.credential)
      .finish_non_exhaustive()
  }
}
impl GithubWorkProvider {
  /// Loads a private `github-work.toml` without reading a forge credential.
  pub fn load(directory: &Path) -> Result<Self, GithubConfigError> {
    let directory = directory.canonicalize().map_err(|_| GithubConfigError)?;
    config::private_directory(&directory)?;
    let bytes = config::read_private(&directory.join("github-work.toml"), 64 * 1024)?;
    let wire: Wire =
      toml::from_str(std::str::from_utf8(&bytes).map_err(|_| GithubConfigError)?).map_err(|_| GithubConfigError)?;
    if wire.config_version != 1
      || wire.repositories.is_empty()
      || wire.repositories.len() > 128
      || wire.repositories.iter().any(|(alias, repo)| {
        !bounded(alias, 1024)
          || !bounded(&repo.repository_id, 1024)
          || !config::segment(&repo.owner)
          || !config::segment(&repo.name)
      })
    {
      return Err(GithubConfigError);
    }
    for aliases in [
      &wire.capabilities.source_repositories,
      &wire.capabilities.reporter_repositories,
    ] {
      let unique: BTreeSet<_> = aliases.iter().collect();
      if aliases.len() > 128
        || unique.len() != aliases.len()
        || aliases.iter().any(|alias| !wire.repositories.contains_key(alias))
      {
        return Err(GithubConfigError);
      }
    }
    config::private_subdirectory(&directory, &wire.credential_directory)?;
    if let Some(selections) = &wire.selection_directory {
      config::private_subdirectory(&directory, selections)?;
    }
    if let Some(mappings) = &wire.mapping_directory {
      config::private_subdirectory(&directory, mappings)?;
    }
    Ok(Self {
      digest: hex::encode(Sha256::digest(&*bytes)),
      api: config::api_settings(wire.api_base, wire.api_version, wire.api_root_certificates_pem)?,
      credential_directory: wire.credential_directory,
      selection_directory: wire.selection_directory,
      mapping_directory: wire.mapping_directory,
      directory,
      repositories: wire.repositories,
      capabilities: wire.capabilities,
    })
  }
  /// Exact installed profile identity, excluding credential file contents and token rotation.
  pub fn configuration_digest(&self) -> &str {
    &self.digest
  }
  /// Executes one scoped Source read or pinned Reporter effect with independent role authority.
  /// Private provider content is returned only as checked normalized candidates or closed failures.
  pub async fn execute(
    &self,
    request: &Request,
    cancellation: tokio_util::sync::CancellationToken,
  ) -> octacity_work_source_protocol::Response {
    if let Request::Reporter(intent) = request {
      return self.report(request, intent, cancellation).await;
    }
    if let Request::Source(source::Request {
      request_id,
      command: source::Command::Authorize(intent),
      ..
    }) = request
    {
      let result = if cancellation.is_cancelled() {
        Err(FailureClass::Cancelled)
      } else {
        self.prepare(request)
      };
      return octacity_work_source_protocol::Response::Source(source::Response {
        protocol_version: source::PROTOCOL_VERSION,
        request_id: request_id.clone(),
        outcome: match result {
          Ok(access) => source::Outcome::Authorized(source::AuthorizedAccess {
            scan_id: intent.scan_id.clone(),
            resource_id: access.resource_id,
            configuration_digest: intent.configuration_digest.clone(),
          }),
          Err(class) => source::Outcome::Failure(octacity_work_source_protocol::Failure {
            class,
            retry_after_millis: None,
          }),
        },
      });
    }
    let Request::Source(source::Request {
      command: source::Command::Scan(intent),
      ..
    }) = request
    else {
      return crate::work_protocol::response(
        request,
        self.prepare(request).err().unwrap_or(FailureClass::Unsupported),
      );
    };
    let operation = async {
      let access = self.prepare(request)?;
      let selections = self.selection_directory.as_ref().ok_or(FailureClass::Unsupported)?;
      let directory = config::private_subdirectory(&self.directory, selections).map_err(|_| FailureClass::Permanent)?;
      let mut selectors = Vec::new();
      for selection in std::iter::once(&intent.selection).chain(&intent.additional_selections) {
        let path = directory.join(format!("{}.json", hex::encode(Sha256::digest(&selection.identity))));
        let bytes = config::read_private(&path, crate::issue_selector::MAX_SELECTOR_CONFIG_BYTES as u64)
          .map_err(|_| FailureClass::Permanent)?;
        if hex::encode(Sha256::digest(&*bytes)) != selection.sha256 {
          return Err(FailureClass::Conflict.into());
        }
        selectors.push(crate::issue_selector::IssueSelector::from_config(&bytes).map_err(|_| FailureClass::Permanent)?);
      }
      let repository = self
        .repositories
        .get(&intent.resource.resource_locator)
        .ok_or(FailureClass::InvalidRequest)?;
      crate::issue_source::scan(
        &self.api,
        &repository.owner,
        &repository.name,
        &access.credential,
        intent,
        &selectors,
      )
      .await
    };
    let result = tokio::select! {
      biased;
      _ = cancellation.cancelled() => Err(crate::work_api::WorkApiFailure::from(FailureClass::Cancelled)),
      result = operation => result,
    };
    let mut response = source::Response {
      protocol_version: source::PROTOCOL_VERSION,
      request_id: match request {
        Request::Source(request) => request.request_id.clone(),
        _ => unreachable!(),
      },
      outcome: match result {
        Ok(page) => source::Outcome::Page(page),
        Err(error) => source::Outcome::Failure(octacity_work_source_protocol::Failure {
          class: error.class,
          retry_after_millis: error.retry_after_millis,
        }),
      },
    };
    if let Request::Source(request) = request
      && response.validate_for(request).is_err()
    {
      response.outcome = source::Outcome::Failure(octacity_work_source_protocol::Failure {
        class: FailureClass::ProtocolFault,
        retry_after_millis: None,
      });
    }
    octacity_work_source_protocol::Response::Source(response)
  }
  async fn report(
    &self,
    envelope: &Request,
    request: &reporter::Request,
    cancellation: tokio_util::sync::CancellationToken,
  ) -> octacity_work_source_protocol::Response {
    let operation = async {
      let access = self.prepare(envelope)?;
      let intent = request.command.intent().ok_or(FailureClass::Unsupported)?;
      let mappings = self.mapping_directory.as_ref().ok_or(FailureClass::Unsupported)?;
      let directory = config::private_subdirectory(&self.directory, mappings).map_err(|_| FailureClass::Permanent)?;
      let path = directory.join(format!(
        "{}.json",
        hex::encode(Sha256::digest(&intent.mapping.identity))
      ));
      let bytes = config::read_private(&path, 64 * 1024).map_err(|_| FailureClass::Permanent)?;
      if hex::encode(Sha256::digest(&*bytes)) != intent.mapping.sha256 {
        return Err(FailureClass::Conflict.into());
      }
      let mapping = crate::status_mapping::StatusMapping::from_config(&bytes).map_err(|_| FailureClass::Permanent)?;
      mapping.effects(&intent.status).map_err(|_| FailureClass::Unsupported)?;
      let repository = self
        .repositories
        .get(&intent.resource.resource_locator)
        .ok_or(FailureClass::InvalidRequest)?;
      crate::issue_reporter::report(
        &self.api,
        &repository.owner,
        &repository.name,
        &access.credential,
        request,
        &mapping,
      )
      .await
    };
    let result = tokio::select! {
      biased;
      _ = cancellation.cancelled() => Err(crate::work_api::WorkApiFailure::from(FailureClass::Cancelled)),
      result = operation => result,
    };
    octacity_work_source_protocol::Response::Reporter(reporter::Response {
      protocol_version: request.protocol_version,
      request_id: request.request_id.clone(),
      outcome: match result {
        Ok(outcome) => outcome,
        Err(error) => reporter::Outcome::Failure(octacity_work_source_protocol::Failure {
          class: error.class,
          retry_after_millis: error.retry_after_millis,
        }),
      },
    })
  }
  /// Verifies role, pinned profile and native/network scope before resolving any private grant.
  /// This performs no provider I/O; concrete listing/projection consumes the retained access.
  pub fn prepare(&self, request: &Request) -> Result<AuthorizedIssueAccess, FailureClass> {
    request.validate().map_err(|_| FailureClass::InvalidRequest)?;
    let (profile, access, network, aliases, class) = match request {
      Request::Source(source::Request {
        command: source::Command::Scan(intent),
        ..
      }) => (
        &intent.configuration_digest,
        &intent.resource,
        &intent.network_hosts,
        &self.capabilities.source_repositories,
        "work_source_read",
      ),
      Request::Source(source::Request {
        command: source::Command::Authorize(intent),
        ..
      }) => (
        &intent.configuration_digest,
        &intent.resource,
        &intent.network_hosts,
        &self.capabilities.source_repositories,
        "work_source_read",
      ),
      Request::Reporter(request) if request.command.intent().is_some() => {
        let intent = request.command.intent().ok_or(FailureClass::Unsupported)?;
        (
          &intent.configuration_digest,
          &intent.resource,
          &intent.network_hosts,
          &self.capabilities.reporter_repositories,
          "work_reporter_write",
        )
      }
      _ => return Err(FailureClass::Unsupported),
    };
    if *profile != self.digest {
      return Err(FailureClass::Conflict);
    }
    config::private_directory(&self.directory).map_err(|_| FailureClass::Permanent)?;
    let current =
      config::read_private(&self.directory.join("github-work.toml"), 64 * 1024).map_err(|_| FailureClass::Permanent)?;
    if hex::encode(Sha256::digest(&*current)) != self.digest {
      return Err(FailureClass::Conflict);
    }
    if !aliases.contains(&access.resource_locator) {
      return Err(FailureClass::Unsupported);
    }
    let repository = self
      .repositories
      .get(&access.resource_locator)
      .ok_or(FailureClass::InvalidRequest)?;
    if repository.repository_id != access.resource_id {
      return Err(FailureClass::InvalidRequest);
    }
    let host = match self.api.base.host().ok_or(FailureClass::InvalidRequest)? {
      url::Host::Domain(host) => host.to_owned(),
      url::Host::Ipv4(ip) => ip.to_string(),
      url::Host::Ipv6(ip) => ip.to_string(),
    };
    if !network.contains(&host) {
      return Err(FailureClass::InvalidRequest);
    }
    let credential_directory =
      config::private_subdirectory(&self.directory, &self.credential_directory).map_err(|_| FailureClass::Permanent)?;
    let credential = config::read_credential(
      &credential_directory,
      class,
      &access.credential.handle,
      &access.resource_locator,
    )
    .map_err(|_| FailureClass::Permanent)?;
    Ok(AuthorizedIssueAccess {
      resource_id: repository.repository_id.clone(),
      credential,
    })
  }
}
fn bounded(value: &str, maximum: usize) -> bool {
  !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
