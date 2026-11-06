use std::{collections::BTreeMap, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_server_application::{
  ManualSourceSelection, RevisionResolutionError, RevisionResolutionRequest, RevisionResolver,
};
use octacity_server_domain::{ImmutableRevision, IntegrationId};
use octacity_server_factory::FactoryRepositoryPath;
use octacity_server_store::PublishedRepository;
use octacity_server_vcs::{HostFailureClass, VcsAdapterRegistry};
use octacity_vcs_protocol::{
  Command, FailureClass, Outcome, ReadFiles, RepositoryAccess, Request, ResolveRevision, VCS_PROTOCOL_VERSION,
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
    match self
      .execute(&request.repository, |repository| {
        Command::ResolveRevision(ResolveRevision {
          repository,
          reference: reference.as_str().to_owned(),
        })
      })
      .await?
    {
      Outcome::Resolved(resolved) => {
        ImmutableRevision::new(resolved.revision).map_err(|_| RevisionResolutionError::Invalid)
      }
      _ => Err(RevisionResolutionError::Invalid),
    }
  }

  async fn read_files(
    &self,
    repository: &PublishedRepository,
    revision: &ImmutableRevision,
    paths: &[FactoryRepositoryPath],
    max_bytes: u32,
  ) -> Result<BTreeMap<FactoryRepositoryPath, Vec<u8>>, RevisionResolutionError> {
    let Outcome::Files(files) = self
      .execute(repository, |repository| {
        Command::ReadFiles(ReadFiles {
          repository,
          revision: revision.as_str().to_owned(),
          paths: paths.iter().map(|path| path.as_str().to_owned()).collect(),
          max_bytes,
        })
      })
      .await?
    else {
      return Err(RevisionResolutionError::Invalid);
    };
    paths
      .iter()
      .cloned()
      .zip(files)
      .map(|(path, file)| {
        let bytes = STANDARD
          .decode(&file.content_base64)
          .map_err(|_| RevisionResolutionError::Invalid)?;
        if STANDARD.encode(&bytes) != file.content_base64 {
          return Err(RevisionResolutionError::Invalid);
        }
        Ok((path, bytes))
      })
      .collect()
  }
}

impl HostedRevisionResolver {
  async fn execute(
    &self,
    repository: &PublishedRepository,
    command: impl FnOnce(RepositoryAccess) -> Command,
  ) -> Result<Outcome, RevisionResolutionError> {
    let integration = self
      .integrations
      .get(&repository.definition.vcs_integration_id)
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
          command: command(RepositoryAccess {
            operation_id,
            repository_id: repository.id.to_string(),
            repository_locator: repository.definition.repository_locator.as_str().to_owned(),
            credential_handle: integration.credential_handle.clone(),
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
      Outcome::Failure(failure) => Err(match failure.class {
        FailureClass::InvalidRequest | FailureClass::ProtocolFault => RevisionResolutionError::Invalid,
        FailureClass::Unsupported => RevisionResolutionError::Unavailable,
        FailureClass::Permanent if failure.code == "not_found" => RevisionResolutionError::NotFound,
        FailureClass::Permanent => RevisionResolutionError::Invalid,
        FailureClass::Transient => RevisionResolutionError::Transient,
        FailureClass::Cancelled => RevisionResolutionError::Cancelled,
      }),
      outcome => Ok(outcome),
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
case "$request" in
  *'"operation":"read_files"'*'"revision":"0123456789abcdef"'*'"paths":["suite/fixture.bin"]'*'"max_bytes":2'*)
    outcome='{"status":"files","payload":[{"path":"suite/fixture.bin","offset":0,"content_base64":"AP8=","truncated":false}]}' ;;
  *'"operation":"read_files"'*'"paths":["suite/truncated.bin"]'*)
    outcome='{"status":"files","payload":[{"path":"suite/truncated.bin","offset":0,"content_base64":"AP8=","truncated":true}]}' ;;
  *'"operation":"resolve_revision"'*)
    outcome='{"status":"resolved","payload":{"reference":"refs/heads/main","revision":"0123456789abcdef"}}' ;;
  *) exit 92 ;;
esac
printf '{"protocol_version":1,"request_id":"%s","outcome":%s}\n' "$request_id" "$outcome"
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
        repository: repository.clone(),
        selection: ManualSourceSelection::Reference(SourceReference::new("refs/heads/main").unwrap()),
      })
      .await
      .unwrap();
    assert_eq!(revision.as_str(), "0123456789abcdef");
    let path = FactoryRepositoryPath::new("suite/fixture.bin").unwrap();
    assert_eq!(
      resolver
        .read_files(&repository, &revision, std::slice::from_ref(&path), 2)
        .await
        .unwrap()[&path],
      [0, 255]
    );
    let path = FactoryRepositoryPath::new("suite/truncated.bin").unwrap();
    assert_eq!(
      resolver.read_files(&repository, &revision, &[path], 2).await,
      Err(RevisionResolutionError::Invalid)
    );
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
    let digest = hex::encode(Sha256::digest(script.as_bytes()));
    fs::write(
      directory.join("adapter.toml"),
      format!(
        "manifest_version = 1\nadapter_id = \"fixture\"\nexecutable = \"adapter\"\nexecutable_sha256 = \"{digest}\"\n\n[protocol]\nmin = 1\nmax = 1\n\n[capabilities]\nlist_references = false\nread_commit = false\nlist_tree = false\nread_file = true\nread_files = true\nresolve_revision = true\n"
      ),
    )
    .unwrap();
    (root, digest)
  }
}
