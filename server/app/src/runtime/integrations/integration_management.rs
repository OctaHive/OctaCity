//! Deployment-owned integration services. Browser metadata cannot install adapters or grant authority.
use crate::runtime::ServerRuntimeError;
use octacity_server_api_rest::integrations::IntegrationManagementApplication;
use octacity_server_application::*;
use octacity_server_secrets::{CredentialPublication, ProviderCredentialError, ProviderCredentialPublisher};
use octacity_server_store::InstalledIntegration;
use octacity_server_store_postgres::PostgresStore;
use octacity_server_work_source::WorkAdapterRegistry;
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

const MAX_CATALOG_BYTES: u64 = 4 * 1024 * 1024;

/// Protected ingress and supervised consumers share the validated installation owner's partition.
/// Keep this platform-independent and preserve the scope of previously accepted history.
pub(in crate::runtime) fn request_context(principal: &str) -> ManagementRequestContext {
  ManagementRequestContext::new(
    ManagementActor::authenticated(principal).expect("validated principal"),
    ManagementSecurityScope::new(format!(
      "local-proxy:{}",
      octacity_server_factory::FactoryDigest::content_sha256(principal.as_bytes())
    ))
    .expect("bounded principal"),
    ManagementRequestId::new(uuid::Uuid::new_v4()).expect("generated identity"),
    ManagementRequestAttributes::new(ManagementIngress::VerifiedIdentity, None),
  )
  .expect("verified installation owner")
}

// Handler composition is static; provider availability belongs to consumer diagnostics.
pub(in crate::runtime) struct SourceManagementReadiness(pub(in crate::runtime) bool);
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
  checks: Vec<crate::runtime::IntegrationCheckProfile>,
  #[serde(default)]
  source_profiles_file: Option<PathBuf>,
  #[serde(default)]
  review_readers: Vec<ReviewReader>,
}

/// One explicitly installed reader with an exact connection and Project boundary.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewReader {
  project_id: octacity_server_domain::ProjectId,
  connection_id: octacity_server_domain::IntegrationId,
  connection_version: octacity_server_domain::IntegrationVersion,
  read: crate::runtime::IntegrationCheckProfile,
}

impl ReviewReader {
  fn bind(self, store: Arc<PostgresStore>, principal: &str) -> crate::runtime::IssueCodeReviewReadProfile {
    let read = self.read;
    crate::runtime::IssueCodeReviewReadProfile {
      managed_access: ManagedIntegrationAccess::new(store, self.connection_id, self.connection_version),
      project_id: self.project_id,
      binding: octacity_server_factory::WorkSourceBinding {
        installation: read.installation,
        capability: read.capability.capability,
        configuration: read.capability.configuration,
        resource: read.resource_reference,
      },
      repository: None,
      publication_targets: vec![],
      plugin: read.plugin,
      provider_configuration: read.provider_configuration,
      resource: read.resource,
      network_hosts: read.network_hosts,
      context: request_context(principal),
      page_items: octacity_work_source_protocol::source::MAX_ITEMS,
      max_page_bytes: octacity_work_source_protocol::MAX_RESPONSE_BYTES as u32,
    }
  }
}
impl Catalog {
  async fn load(path: &std::path::Path) -> Result<Self, ServerRuntimeError> {
    let invalid = || ServerRuntimeError::InvalidManagementPolicy;
    let bytes = crate::runtime::assembly::read_private_bytes("integration catalog", path, MAX_CATALOG_BYTES)
      .await
      .map_err(ServerRuntimeError::Assembly)?;
    let catalog: Self = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if catalog.version != 1
      || !catalog.registry.is_absolute()
      || catalog.installations.len() > 128
      || catalog.review_readers.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || catalog
        .source_profiles_file
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
      return Err(invalid());
    }
    Ok(catalog)
  }
}

pub(in crate::runtime) async fn validate_source_profiles(
  config: &crate::ServerConfig,
) -> Result<(), crate::runtime::RuntimeAssemblyError> {
  let Some(local) = config.integration_management() else {
    return Ok(());
  };
  if let Some(path) = &local.catalog_file {
    let catalog = Catalog::load(path)
      .await
      .map_err(|_| crate::runtime::RuntimeAssemblyError::InvalidSourceProfiles)?;
    if let Some(path) = catalog.source_profiles_file {
      let registry = WorkAdapterRegistry::discover(&catalog.registry)
        .map_err(|_| crate::runtime::RuntimeAssemblyError::InvalidSourceProfiles)?;
      crate::runtime::sources::source_profiles_file::SourceProfiles::load(
        &path,
        &catalog.installations,
        &registry,
        std::time::Duration::from_millis(local.operation_timeout_milliseconds),
        std::time::Duration::from_millis(local.cancellation_grace_milliseconds),
      )
      .await?;
    }
  }
  Ok(())
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
/// Ordinary management retains the acknowledged network mode and the configured verified proxy.
/// Protected integration routes continue to use LocalIntegrationPolicy alone.
pub(in crate::runtime) fn management_policy(principal: Option<&str>) -> Arc<dyn ManagementAuthorizationPolicy> {
  Arc::new(RuntimeManagementPolicy {
    local: principal.map(|principal| LocalIntegrationPolicy {
      principal: principal.into(),
    }),
  })
}
struct RuntimeManagementPolicy {
  local: Option<LocalIntegrationPolicy>,
}
#[async_trait::async_trait]
impl ManagementAuthorizationPolicy for RuntimeManagementPolicy {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if let Ok(grant) = TrustedNetworkManagementPolicy
      .authorize(context, action, resource)
      .await
    {
      return Ok(grant);
    }
    match &self.local {
      Some(local) => local.authorize(context, action, resource).await,
      None => Err(ManagementAuthorizationDenial::forbidden()),
    }
  }
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
pub(in crate::runtime) struct ManagedIssueSources {
  pub(in crate::runtime) handlers: Arc<IssueSourceHandlers<PostgresStore>>,
  pub(in crate::runtime) policy: Arc<dyn ManagementAuthorizationPolicy>,
}
pub(in crate::runtime) struct InstalledIntegrationManagement {
  pub(in crate::runtime) issues: Option<ManagedIssueSources>,
  pub(in crate::runtime) application: IntegrationManagementApplication,
  pub(in crate::runtime) source_reads: Option<crate::runtime::sources::issue_source_worker::IssueSourceWorker>,
  pub(in crate::runtime) source_management: Option<Arc<crate::runtime::sources::source_management::SourceManagement>>,
  pub(in crate::runtime) code_reviews: Option<Arc<dyn IssueCodeReviewObservations>>,
}
#[cfg(all(test, unix))]
pub(in crate::runtime) async fn install(
  config: &crate::ServerConfig,
  store: Arc<PostgresStore>,
  cancellation: tokio_util::sync::CancellationToken,
  readiness: &mut crate::readiness::ReadinessChecks,
) -> Result<Option<InstalledIntegrationManagement>, ServerRuntimeError> {
  install_with_factory(config, store, cancellation, readiness, None).await
}

/// Completes optional native producers before the single Factory executor registry is frozen.
pub(in crate::runtime) async fn install_with_factory(
  config: &crate::ServerConfig,
  store: Arc<PostgresStore>,
  cancellation: tokio_util::sync::CancellationToken,
  readiness: &mut crate::readiness::ReadinessChecks,
  mut factory: Option<&mut crate::runtime::FactoryRuntimeInstallation>,
) -> Result<Option<InstalledIntegrationManagement>, ServerRuntimeError> {
  let Some(local) = config.integration_management() else {
    return Ok(None);
  };
  let Some(path) = &local.catalog_file else {
    return Ok(None);
  };
  let invalid = || ServerRuntimeError::InvalidManagementPolicy;
  let mut catalog = Catalog::load(path).await?;
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
          octacity_server_store::IntegrationPurpose::RepositoryRead => adapter.manifest.capabilities.review.is_some(),
          _ => false,
        };
        if !supported {
          return Err(invalid());
        }
      }
    }
  }
  let code_reviews: Option<Arc<dyn IssueCodeReviewObservations>> = if catalog.review_readers.is_empty() {
    None
  } else {
    for reader in &catalog.review_readers {
      crate::runtime::sources::source_profiles_file::validate_access(
        &reader.read,
        octacity_server_store::IntegrationPurpose::RepositoryRead,
        &catalog.installations,
        &registry,
      )
      .map_err(|_| invalid())?;
    }
    let profiles = std::mem::take(&mut catalog.review_readers)
      .into_iter()
      .map(|reader| reader.bind(store.clone(), &local.principal))
      .collect();
    Some(Arc::new(
      crate::runtime::HostedIssueCodeReviewObservations::new(
        registry.clone(),
        profiles,
        Arc::new(SystemFactoryExecutionClock),
        std::time::Duration::from_millis(local.operation_timeout_milliseconds),
        std::time::Duration::from_millis(local.cancellation_grace_milliseconds),
        cancellation.clone(),
      )
      .map_err(|_| invalid())?,
    ))
  };
  let policy: Arc<dyn ManagementAuthorizationPolicy> = Arc::new(LocalIntegrationPolicy {
    principal: local.principal.clone(),
  });
  let publishers = Arc::new(InstalledPublishers::load(&catalog)?);
  let checker = Arc::new(
    crate::runtime::HostedIntegrationConnectionChecker::new(
      registry.clone(),
      catalog.checks.clone(),
      std::time::Duration::from_millis(local.operation_timeout_milliseconds),
      std::time::Duration::from_millis(local.cancellation_grace_milliseconds),
      cancellation.clone(),
    )
    .map_err(|_| invalid())?,
  );
  let checks = Arc::new(IntegrationCheckHandlers::new(store.clone(), checker, policy.clone()));
  let connections =
    Arc::new(IntegrationConnectionHandlers::new(store.clone(), catalog.installations.clone()).map_err(|_| invalid())?);
  let queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), connections.clone()));
  let commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), connections));
  let administration = Arc::new(IntegrationAdministrationHandlers::new(store.clone(), publishers));
  let admin_commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), administration.clone()));
  let admin_queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), administration));
  // Retained input history needs the store and authorization, not installed execution profiles.
  let dependencies =
    Arc::new(FactorySourceConfigurationHandlers::new(store.clone(), policy.clone(), vec![]).map_err(|_| invalid())?);
  let access = Arc::new(IntegrationProjectAccessHandlers::new(store.clone(), policy.clone()));
  let mut application = IntegrationManagementApplication::new(admin_commands.clone())
    .with_checks(Arc::new(AuthorizedQueryHandler::new(policy.clone(), checks)))
    .with_metadata(commands, queries.clone())
    .with_discovery(queries)
    .with_administration(admin_commands, admin_queries)
    .with_dependencies(Arc::new(AuthorizedQueryHandler::new(policy.clone(), dependencies)))
    .with_project_access(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), access.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy.clone(), access)),
    );
  let mut source_reads = None;
  let mut source_management = None;
  let mut issues = None;
  if let Some(path) = &catalog.source_profiles_file {
    application = application.require_source_management();
    let sources = match crate::runtime::sources::source_profiles_file::SourceProfiles::load(
      path,
      &catalog.installations,
      &registry,
      std::time::Duration::from_millis(local.operation_timeout_milliseconds),
      std::time::Duration::from_millis(local.cancellation_grace_milliseconds),
    )
    .await
    {
      Ok(profiles) => {
        let reporter_actions = factory
          .as_ref()
          .map(|factory| crate::runtime::reporting::source_reporter_actions::contracts(&profiles, factory))
          .transpose()
          .map_err(|_| invalid())?
          .unwrap_or_default();
        let sources = Arc::new(crate::runtime::sources::source_management::SourceManagement {
          reporter_actions,
          store: store.clone(),
          policy: policy.clone(),
          profiles,
          registry,
          timeout: std::time::Duration::from_millis(local.operation_timeout_milliseconds),
          grace: std::time::Duration::from_millis(local.cancellation_grace_milliseconds),
          cancellation,
        });
        if let Some(factory) = factory.as_mut() {
          crate::runtime::reporting::source_reporter_actions::install(&sources, factory).map_err(|_| invalid())?;
        }
        Some(sources)
      }
      Err(_) => {
        tracing::warn!("installed Source profiles unavailable; Source management readiness failed");
        None
      }
    };
    // Trusted protected intent and retained metadata outlive an unavailable native reader.
    let reader: Arc<dyn WorkSourceObservations> = match &sources {
      Some(sources) => sources.clone(),
      None => Arc::new(crate::runtime::application::UnavailableIssueSource),
    };
    let mut project_sources =
      IssueSourceHandlers::new(store, reader, policy.clone(), Arc::new(SystemFactoryExecutionClock));
    let source_profile_provider: Arc<dyn IssueSourceProfileProvider> = match &sources {
      Some(sources) => Arc::new(crate::runtime::sources::source_management::IssueSourceChoiceProvider(
        sources.clone(),
      )),
      None => Arc::new(crate::runtime::sources::source_management::UnavailableIssueSourceProfiles),
    };
    project_sources = project_sources.with_source_profile_provider(source_profile_provider);
    if let Some(sources) = &sources {
      project_sources = project_sources.with_project_source_profiles(Arc::new(
        crate::runtime::sources::source_management::IssueSourceChoiceProvider(sources.clone()),
      ));
    }
    let project_sources = Arc::new(project_sources);
    issues = Some(ManagedIssueSources {
      handlers: project_sources.clone(),
      policy: policy.clone(),
    });
    let project_queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), project_sources.clone()));
    application = application.with_project_source_imports(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), project_sources)),
      project_queries.clone(),
    );
    if let Some(sources) = sources {
      source_management = Some(sources.clone());
      application = application
        .with_project_source_choices(project_queries.clone())
        .with_project_source_preview(project_queries);
      let worker =
        crate::runtime::sources::issue_source_worker::IssueSourceWorker::new(sources.clone(), &local.principal);
      readiness.push_factory(worker.health.clone());
      source_reads = Some(worker);
      let sources = Arc::new(InstalledFactorySourceHandlers::new(sources));
      application = application
        .with_source_configuration(
          Arc::new(AuthorizedCommandHandler::new(policy.clone(), sources.clone())),
          Arc::new(AuthorizedQueryHandler::new(policy.clone(), sources.clone())),
        )
        .with_source_reads(Arc::new(AuthorizedQueryHandler::new(policy, sources)));
    }
  }
  Ok(Some(InstalledIntegrationManagement {
    application,
    source_reads,
    source_management,
    issues,
    code_reviews,
  }))
}
