//! Write-only GitHub adapter for the installed purpose/resource credential files.
use octacity_server_secrets::{
  LogicalSecretReference, ProviderCredentialError as Error, ProviderCredentialManager, ProviderCredentialReceipt,
  ProviderCredentialValue,
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::sync::Arc;
use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
};

const MAX_INSTALLED_CREDENTIAL_TARGETS: usize = 128;
const MAX_PRIVATE_HANDLE_BYTES: usize = 1024;
const MAX_RESOURCE_ALIAS_BYTES: usize = 1024;

#[derive(Clone, Copy)]
pub(super) enum PublishMode {
  Create,
  Rotate,
}

/// Existing private-file classes. Remote token permissions do not widen a class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GithubCredentialClass {
  /// GitHub Issue Source read authority.
  TaskRead,
  /// GitHub Issue Reporter write authority.
  TaskUpdate,
  /// Existing independent repository-read class.
  RepositoryRead,
  /// Existing independent Delivery class.
  Delivery,
}
impl GithubCredentialClass {
  #[cfg(any(target_os = "linux", target_os = "macos"))]
  pub(crate) fn wire(self) -> &'static str {
    match self {
      Self::TaskRead => "work_source_read",
      Self::TaskUpdate => "work_reporter_write",
      Self::RepositoryRead => "vcs_read",
      Self::Delivery => "delivery_write",
    }
  }
}
/// Trusted installed target, supplied by server composition, never a client request.
#[derive(Clone)]
pub struct GithubCredentialTarget {
  /// Logical reference already pinned by connection metadata.
  pub reference: LogicalSecretReference,
  /// Original private provider handle, preserved during adoption/rotation.
  pub handle: String,
  /// Immutable independently scoped credential class.
  pub class: GithubCredentialClass,
  /// Exact installed resource aliases permitted for this handle.
  pub repositories: Vec<String>,
}
/// Private-file implementation of the common integration credential port.
/// It performs no provider I/O, Job delegation, connection activation or Factory admission.
/// Linux and macOS descriptor-relative private storage are supported; other OSes fail closed.
pub struct GithubCredentialManager {
  #[cfg(any(target_os = "linux", target_os = "macos"))]
  pub(crate) directory: Arc<crate::credential_files::CredentialDirectory>,
  pub(crate) targets: BTreeMap<LogicalSecretReference, GithubCredentialTarget>,
}
impl GithubCredentialManager {
  /// Binds only operator-installed paths, opaque handles and resource/class ceilings.
  pub fn new(root: &Path, relative: &Path, targets: Vec<GithubCredentialTarget>) -> Result<Self, Error> {
    let mut selected = BTreeMap::new();
    let mut handles = BTreeSet::new();
    if targets.is_empty() || targets.len() > MAX_INSTALLED_CREDENTIAL_TARGETS {
      return Err(Error::Denied);
    }
    for target in targets {
      if target.handle.is_empty()
        || target.handle.len() > MAX_PRIVATE_HANDLE_BYTES
        || target.handle.chars().any(char::is_control)
        || !handles.insert(target.handle.clone())
        || target.repositories.is_empty()
        || target.repositories.len() > crate::config::MAX_CREDENTIAL_RESOURCES
        || target
          .repositories
          .iter()
          .any(|r| r.is_empty() || r.len() > MAX_RESOURCE_ALIAS_BYTES || r.chars().any(char::is_control))
        || target.repositories.iter().collect::<BTreeSet<_>>().len() != target.repositories.len()
        || selected.insert(target.reference.clone(), target).is_some()
      {
        return Err(Error::Denied);
      }
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
      Ok(Self {
        directory: Arc::new(crate::credential_files::CredentialDirectory::new(root, relative)?),
        targets: selected,
      })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
      let _ = (root, relative, selected);
      Err(Error::Unavailable)
    }
  }
}
#[async_trait::async_trait]
impl ProviderCredentialManager for GithubCredentialManager {
  async fn adopt(&self, reference: &LogicalSecretReference) -> Result<ProviderCredentialReceipt, Error> {
    let target = self.targets.get(reference).ok_or(Error::NotConfigured)?.clone();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
      let directory = self.directory.clone();
      tokio::task::spawn_blocking(move || {
        let bytes = directory.read(&target.handle)?;
        crate::config::validate_installed_credential(&bytes, target.class.wire(), &target.repositories)
          .map_err(|_| Error::Denied)?;
        Ok(ProviderCredentialReceipt {
          reference: target.reference,
          configured: true,
        })
      })
      .await
      .map_err(|_| Error::Unavailable)?
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
      let _ = target;
      Err(Error::Unavailable)
    }
  }
  async fn provision(
    &self,
    reference: &LogicalSecretReference,
    value: ProviderCredentialValue,
  ) -> Result<ProviderCredentialReceipt, Error> {
    self.publish(reference, value, PublishMode::Create).await
  }
  async fn rotate(
    &self,
    reference: &LogicalSecretReference,
    value: ProviderCredentialValue,
  ) -> Result<ProviderCredentialReceipt, Error> {
    self.publish(reference, value, PublishMode::Rotate).await
  }
}
impl GithubCredentialManager {
  async fn publish(
    &self,
    reference: &LogicalSecretReference,
    value: ProviderCredentialValue,
    mode: PublishMode,
  ) -> Result<ProviderCredentialReceipt, Error> {
    let target = self.targets.get(reference).ok_or(Error::NotConfigured)?.clone();
    let token = std::str::from_utf8(value.expose_for_installation()).map_err(|_| Error::InvalidValue)?;
    if !crate::config::valid_credential_token(token) {
      return Err(Error::InvalidValue);
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
      let directory = self.directory.clone();
      tokio::task::spawn_blocking(move || {
        let token = std::str::from_utf8(value.expose_for_installation()).map_err(|_| Error::InvalidValue)?;
        let bytes = crate::config::encode_installed_credential(target.class.wire(), token, &target.repositories, None)?;
        let _lock = directory.lock()?;
        if matches!(mode, PublishMode::Rotate) {
          let current = directory.read(&target.handle)?;
          let installed: crate::config::Credential = serde_json::from_slice(&current).map_err(|_| Error::Denied)?;
          if installed.publication.is_some() {
            return Err(Error::Denied);
          }
          crate::config::validate_installed_credential(&current, target.class.wire(), &target.repositories)
            .map_err(|_| Error::Denied)?;
        }
        directory.publish(&target.handle, &bytes, mode, None)?;
        Ok(ProviderCredentialReceipt {
          reference: target.reference,
          configured: true,
        })
      })
      .await
      .map_err(|_| Error::Unavailable)?
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
      let _ = (target, value, mode);
      Err(Error::Unavailable)
    }
  }
}
