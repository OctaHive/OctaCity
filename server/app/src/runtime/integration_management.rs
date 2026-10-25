//! Deployment-owned integration services. Browser metadata cannot install adapters or grant authority.
use super::ServerRuntimeError;
use octacity_server_api_rest::integrations::IntegrationManagementApplication;
use octacity_server_application::*;
use octacity_server_secrets::{CredentialPublication, ProviderCredentialError, ProviderCredentialPublisher};
use octacity_server_store::InstalledIntegration;
use octacity_server_store_postgres::PostgresStore;
use octacity_server_work_source::WorkAdapterRegistry;
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;
// Handler composition is static; provider availability belongs to consumer diagnostics.
struct SourceManagementReadiness(bool);
#[async_trait::async_trait]
impl crate::readiness::ReadinessCheck for SourceManagementReadiness {
  fn name(&self) -> &'static str {
    "factory-source-management"
  }
  async fn check(&self) -> bool {
    self.0
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
  version: u16,
  registry: PathBuf,
  installations: Vec<InstalledIntegration>,
  #[serde(default)]
  credential_publishers: Vec<CredentialPublisher>,
  #[serde(default)]
  checks: Vec<super::IntegrationCheckProfile>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CredentialPublisher {
  GithubPrivateFiles {
    root: PathBuf,
    directory: PathBuf,
    targets: Vec<CredentialTarget>,
  },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialTarget {
  reference: LogicalSecretReference,
  handle: String,
  purpose: octacity_server_store::IntegrationPurpose,
  resources: Vec<String>,
}
struct LocalIntegrationPolicy {
  principal: String,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for LocalIntegrationPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if context.actor().kind() == ManagementActorKind::AuthenticatedManagement
      && context.actor().identity() == Some(self.principal.as_str())
      && context.attributes().ingress() == ManagementIngress::VerifiedIdentity
    {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    } else {
      Err(ManagementAuthorizationDenial::forbidden())
    }
  }
}
// No installed publisher means no private material can be provisioned or adopted.
#[derive(Default)]
struct InstalledPublishers(std::collections::BTreeMap<LogicalSecretReference, Arc<dyn ProviderCredentialPublisher>>);
impl InstalledPublishers {
  fn load(catalog: &Catalog) -> Result<Self, ServerRuntimeError> {
    use octacity_github::{GithubCredentialClass as Class, GithubCredentialManager, GithubCredentialTarget};
    use octacity_server_store::IntegrationPurpose as Purpose;
    let invalid = || ServerRuntimeError::InvalidManagementPolicy;
    let mut selected = Self::default();
    if catalog.credential_publishers.len() > 128 {
      return Err(invalid());
    }
    for publisher in &catalog.credential_publishers {
      let CredentialPublisher::GithubPrivateFiles {
        root,
        directory,
        targets,
      } = publisher;
      if !root.is_absolute() {
        return Err(invalid());
      }
      let mut installed = Vec::new();
      for target in targets {
        let allowed: std::collections::BTreeSet<_> = catalog
          .installations
          .iter()
          .flat_map(|i| &i.resources)
          .filter(|r| {
            r.capabilities
              .iter()
              .any(|c| c.credential == target.reference && c.purpose == target.purpose)
          })
          .map(|r| r.locator.as_str())
          .collect();
        if allowed.is_empty() || target.resources.iter().any(|r| !allowed.contains(r.as_str())) {
          return Err(invalid());
        }
        installed.push(GithubCredentialTarget {
          reference: target.reference.clone(),
          handle: target.handle.clone(),
          class: match target.purpose {
            Purpose::TaskRead => Class::TaskRead,
            Purpose::TaskUpdate => Class::TaskUpdate,
            Purpose::RepositoryRead => Class::RepositoryRead,
            Purpose::Delivery => Class::Delivery,
          },
          repositories: target.resources.clone(),
        });
      }
      let manager: Arc<dyn ProviderCredentialPublisher> =
        Arc::new(GithubCredentialManager::new(root, directory, installed).map_err(|_| invalid())?);
      for target in targets {
        if selected.0.insert(target.reference.clone(), manager.clone()).is_some() {
          return Err(invalid());
        }
      }
    }
    Ok(selected)
  }
  fn get(
    &self,
    publication: &CredentialPublication,
  ) -> Result<&Arc<dyn ProviderCredentialPublisher>, ProviderCredentialError> {
    self
      .0
      .get(&publication.reference)
      .ok_or(ProviderCredentialError::NotConfigured)
  }
}
#[async_trait::async_trait]
impl ProviderCredentialPublisher for InstalledPublishers {
  async fn prepare(
    &self,
    publication: &CredentialPublication,
    kind: CredentialPublicationKind,
    value: Option<ProviderCredentialValue>,
  ) -> Result<(), ProviderCredentialError> {
    self.get(publication)?.prepare(publication, kind, value).await
  }
  async fn activate(&self, publication: &CredentialPublication) -> Result<(), ProviderCredentialError> {
    self.get(publication)?.activate(publication).await
  }
  async fn discard(&self, publication: &CredentialPublication) -> Result<(), ProviderCredentialError> {
    self.get(publication)?.discard(publication).await
  }
}
pub(super) async fn install(
  config: &crate::ServerConfig,
  store: Arc<PostgresStore>,
  cancellation: tokio_util::sync::CancellationToken,
  readiness: &mut crate::readiness::ReadinessChecks,
) -> Result<Option<IntegrationManagementApplication>, ServerRuntimeError> {
  let Some(local) = config.integration_management() else {
    return Ok(None);
  };
  let Some(path) = &local.catalog_file else {
    return Ok(None);
  };
  let invalid = || ServerRuntimeError::InvalidManagementPolicy;
  let bytes = super::assembly::read_private_bytes("integration catalog", path, MAX_CATALOG_BYTES)
    .await
    .map_err(ServerRuntimeError::Assembly)?;
  let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
  if catalog.version != 1 || !catalog.registry.is_absolute() || catalog.installations.len() > 128 {
    return Err(invalid());
  }
  let registry = Arc::new(WorkAdapterRegistry::discover(&catalog.registry).map_err(|_| invalid())?);
  for installed in &catalog.installations {
    installed.validate().map_err(|_| invalid())?;
    let adapter = registry
      .resolve(
        installed.adapter.identity().as_str(),
        &installed.adapter.digest().to_string(),
      )
      .map_err(|_| invalid())?;
    for resource in &installed.resources {
      for capability in &resource.capabilities {
        let supported = match capability.purpose {
          octacity_server_store::IntegrationPurpose::TaskRead => adapter.manifest.capabilities.source.is_some(),
          octacity_server_store::IntegrationPurpose::TaskUpdate => adapter.manifest.capabilities.reporter.is_some(),
          _ => false,
        };
        if !supported {
          return Err(invalid());
        }
      }
    }
  }
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(LocalIntegrationPolicy {
    principal: local.principal.clone(),
  });
  let publishers = Arc::new(InstalledPublishers::load(&catalog)?);
  let checker = Arc::new(
    super::HostedIntegrationConnectionChecker::new(
      registry,
      catalog.checks,
      std::time::Duration::from_millis(local.operation_timeout_milliseconds),
      std::time::Duration::from_millis(local.cancellation_grace_milliseconds),
      cancellation,
    )
    .map_err(|_| invalid())?,
  );
  let checks = Arc::new(IntegrationCheckHandlers::new(store.clone(), checker, policy.clone()));
  let connections =
    Arc::new(IntegrationConnectionHandlers::new(store.clone(), catalog.installations).map_err(|_| invalid())?);
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), connections.clone()));
  let commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), connections));
  let administration = Arc::new(IntegrationAdministrationHandlers::new(store.clone(), publishers));
  let admin_commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), administration.clone()));
  let admin_queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), administration));
  // Retained input history needs the store and authorization, not installed execution profiles.
  let dependencies =
    Arc::new(FactorySourceConfigurationHandlers::new(store.clone(), policy.clone(), vec![]).map_err(|_| invalid())?);
  let access = Arc::new(IntegrationProjectAccessHandlers::new(store, policy.clone()));
  let application = IntegrationManagementApplication::new(admin_commands.clone())
    .with_checks(Arc::new(AuthorizedQueryHandler::new(policy.clone(), checks)))
    .with_metadata(commands, queries.clone())
    .with_discovery(queries)
    .with_administration(admin_commands, admin_queries)
    .with_dependencies(Arc::new(AuthorizedQueryHandler::new(policy.clone(), dependencies)))
    .with_project_access(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), access.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, access)),
    );
  if let Some(ready) = application.source_management_readiness() {
    readiness.push_factory(Arc::new(SourceManagementReadiness(ready)));
  }
  Ok(Some(application))
}
