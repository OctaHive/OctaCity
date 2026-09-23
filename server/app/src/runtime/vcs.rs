use std::{collections::BTreeMap, sync::Arc, time::Duration};

use octacity_server_application::{
  ManualSourceSelection, RevisionResolutionError, RevisionResolutionRequest, RevisionResolver,
};
use octacity_server_domain::{ImmutableRevision, IntegrationId};
use octacity_server_vcs::{HostFailureClass, VcsAdapterRegistry};
use octacity_vcs_protocol::{
  Command, FailureClass, Outcome, RepositoryAccess, Request, ResolveRevision, VCS_PROTOCOL_VERSION,
};
use tokio_util::sync::CancellationToken;

use crate::config::VcsIntegrationConfig;

#[derive(Clone)]
struct VcsIntegration {
  adapter_id: String,
  adapter_sha256: String,
  credential_handle: String,
}

/// Revision resolver backed by the operator-installed, digest-pinned VCS registry.
pub(super) struct HostedRevisionResolver {
  registry: Arc<VcsAdapterRegistry>,
  integrations: BTreeMap<IntegrationId, VcsIntegration>,
  operation_timeout: Duration,
  cancellation_grace: Duration,
  cancellation: CancellationToken,
}

impl HostedRevisionResolver {
  pub(super) fn new(
    registry: Arc<VcsAdapterRegistry>,
    integrations: &[VcsIntegrationConfig],
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Self {
    Self {
      registry,
      integrations: integrations
        .iter()
        .map(|config| {
          (
            config.integration_id(),
            VcsIntegration {
              adapter_id: config.adapter_id().to_owned(),
              adapter_sha256: config.adapter_sha256().to_owned(),
              credential_handle: config.credential_handle().to_owned(),
            },
          )
        })
        .collect(),
      operation_timeout,
      cancellation_grace,
      cancellation,
    }
  }
}

#[async_trait::async_trait]
impl RevisionResolver for HostedRevisionResolver {
  async fn resolve(&self, request: RevisionResolutionRequest) -> Result<ImmutableRevision, RevisionResolutionError> {
    let reference = match request.selection {
      ManualSourceSelection::ExactRevision(revision) => return Ok(revision),
      ManualSourceSelection::Reference(reference) => reference,
      ManualSourceSelection::DefaultReference => return Err(RevisionResolutionError::Invalid),
    };
    let integration = self
      .integrations
      .get(&request.repository.definition.vcs_integration_id)
      .ok_or(RevisionResolutionError::Unavailable)?;
    let adapter = self
      .registry
      .resolve(&integration.adapter_id, &integration.adapter_sha256)
      .map_err(|_| RevisionResolutionError::Invalid)?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let outcome = adapter
      .execute(
        Request {
          protocol_version: VCS_PROTOCOL_VERSION,
          request_id: uuid::Uuid::new_v4().to_string(),
          command: Command::ResolveRevision(ResolveRevision {
            repository: RepositoryAccess {
              operation_id,
              repository_id: request.repository.id.to_string(),
              repository_locator: request.repository.definition.repository_locator.as_str().to_owned(),
              credential_handle: integration.credential_handle.clone(),
            },
            reference: reference.as_str().to_owned(),
          }),
        },
        self.operation_timeout,
        self.cancellation_grace,
        self.cancellation.child_token(),
      )
      .await
      .map_err(|error| match error.class() {
        HostFailureClass::InvalidRequest | HostFailureClass::Permanent | HostFailureClass::ProtocolFault => {
          RevisionResolutionError::Invalid
        }
        HostFailureClass::Unsupported => RevisionResolutionError::Unavailable,
        HostFailureClass::Transient => RevisionResolutionError::Transient,
        HostFailureClass::Cancelled => RevisionResolutionError::Cancelled,
      })?;
    match outcome {
      Outcome::Resolved(resolved) => {
        ImmutableRevision::new(resolved.revision).map_err(|_| RevisionResolutionError::Invalid)
      }
      Outcome::Failure(failure) => Err(match failure.class {
        FailureClass::InvalidRequest | FailureClass::ProtocolFault => RevisionResolutionError::Invalid,
        FailureClass::Unsupported => RevisionResolutionError::Unavailable,
        FailureClass::Permanent if failure.code == "not_found" => RevisionResolutionError::NotFound,
        FailureClass::Permanent => RevisionResolutionError::Invalid,
        FailureClass::Transient => RevisionResolutionError::Transient,
        FailureClass::Cancelled => RevisionResolutionError::Cancelled,
      }),
      Outcome::References(_)
      | Outcome::Commit(_)
      | Outcome::Tree(_)
      | Outcome::File(_)
      | Outcome::Acknowledged { .. } => Err(RevisionResolutionError::Invalid),
    }
  }
}

#[cfg(all(test, unix))]
mod tests {
  use std::{collections::BTreeSet, fs, time::Duration};

  use octacity_server_domain::{
    ProjectId, RepositoryId, RepositoryLocator, RepositoryName, RepositoryVersion, SourceReference, Timestamp,
  };
  use octacity_server_store::{PublishedRepository, RepositoryDefinition, RepositorySelectionPolicy};
  use sha2::{Digest as _, Sha256};
  use tempfile::TempDir;

  use super::*;

  #[tokio::test]
  async fn configured_integration_resolves_mutable_references_through_the_verified_host() {
    let script = r#"#!/bin/sh
IFS= read -r request || exit 90
case "$request" in
  *repository.git*secret:repository*) ;;
  *) exit 91 ;;
esac
request_id=$(printf '%s' "$request" | /usr/bin/sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '{"protocol_version":1,"request_id":"%s","outcome":{"status":"resolved","payload":{"reference":"refs/heads/main","revision":"0123456789abcdef"}}}\n' "$request_id"
"#;
    let (root, digest) = install_adapter(script);
    let integration_id = IntegrationId::from_uuid(uuid::Uuid::from_u128(201)).unwrap();
    let config: VcsIntegrationConfig = toml::from_str(&format!(
      "integration_id = \"{integration_id}\"\nadapter_id = \"fixture\"\nadapter_sha256 = \"{digest}\"\ncredential_handle = \"secret:repository\"\n"
    ))
    .unwrap();
    let resolver = HostedRevisionResolver::new(
      Arc::new(VcsAdapterRegistry::discover(root.path()).unwrap()),
      &[config],
      Duration::from_secs(2),
      Duration::from_millis(100),
      CancellationToken::new(),
    );
    let repository = repository(integration_id);

    let revision = resolver
      .resolve(RevisionResolutionRequest {
        repository,
        selection: ManualSourceSelection::Reference(SourceReference::new("refs/heads/main").unwrap()),
      })
      .await
      .unwrap();
    assert_eq!(revision.as_str(), "0123456789abcdef");
  }

  #[tokio::test]
  async fn process_cancellation_reaches_an_active_vcs_adapter() {
    let script = "#!/bin/sh\nIFS= read -r request || exit 90\nsleep 30\n";
    let (root, digest) = install_adapter(script);
    let integration_id = IntegrationId::from_uuid(uuid::Uuid::from_u128(211)).unwrap();
    let config: VcsIntegrationConfig = toml::from_str(&format!(
      "integration_id = \"{integration_id}\"\nadapter_id = \"fixture\"\nadapter_sha256 = \"{digest}\"\ncredential_handle = \"secret:repository\"\n"
    ))
    .unwrap();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let resolver = HostedRevisionResolver::new(
      Arc::new(VcsAdapterRegistry::discover(root.path()).unwrap()),
      &[config],
      Duration::from_secs(30),
      Duration::from_millis(50),
      cancellation,
    );

    let result = tokio::time::timeout(
      Duration::from_secs(2),
      resolver.resolve(RevisionResolutionRequest {
        repository: repository(integration_id),
        selection: ManualSourceSelection::Reference(SourceReference::new("refs/heads/main").unwrap()),
      }),
    )
    .await
    .expect("process cancellation must bound the adapter call");
    assert_eq!(result, Err(RevisionResolutionError::Cancelled));
  }

  fn repository(integration_id: IntegrationId) -> PublishedRepository {
    PublishedRepository {
      id: RepositoryId::from_uuid(uuid::Uuid::from_u128(202)).unwrap(),
      project_id: ProjectId::from_uuid(uuid::Uuid::from_u128(203)).unwrap(),
      name: RepositoryName::new("main").unwrap(),
      version: RepositoryVersion::INITIAL,
      definition: RepositoryDefinition {
        vcs_integration_id: integration_id,
        repository_locator: RepositoryLocator::new("https://example.invalid/repository.git").unwrap(),
        selection: RepositorySelectionPolicy {
          allowed_references: BTreeSet::from([SourceReference::new("refs/heads/main").unwrap()]),
          default_reference: None,
          allow_exact_revision: true,
        },
      },
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    }
  }

  fn install_adapter(script: &str) -> (TempDir, String) {
    use std::os::unix::fs::PermissionsExt as _;

    let root = TempDir::new().unwrap();
    let directory = root.path().join("fixture");
    fs::create_dir(&directory).unwrap();
    let executable = directory.join("adapter");
    fs::write(&executable, script).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let digest = format!("{:x}", Sha256::digest(script.as_bytes()));
    fs::write(
      directory.join("adapter.toml"),
      format!(
        "manifest_version = 1\nadapter_id = \"fixture\"\nexecutable = \"adapter\"\nexecutable_sha256 = \"{digest}\"\n\n[protocol]\nmin = 1\nmax = 1\n\n[capabilities]\nlist_references = false\nread_commit = false\nlist_tree = false\nread_file = false\nresolve_revision = true\n"
      ),
    )
    .unwrap();
    (root, digest)
  }
}
