//! Private preparation and monotonic activation behind the original runtime handle.
use octacity_server_secrets::{CredentialPublication, CredentialPublicationKind as Kind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrivatePublication {
  pub stamp: CredentialPublication,
  pub kind: Kind,
  pub abandoned: bool,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
  use super::*;
  use crate::{GithubCredentialManager, credential_management::PublishMode};
  use octacity_server_secrets::{
    ProviderCredentialError as Error, ProviderCredentialPublisher, ProviderCredentialValue,
  };

  fn staged_handle(handle: &str, stamp: &CredentialPublication) -> String {
    // Installed handles forbid control bytes. This hash input cannot alias any
    // permitted active handle, even one spelling our preparation prefix.
    format!("octacity-prepared\0{handle}\0{}", hex::encode(stamp.operation))
  }
  fn read(
    directory: &crate::credential_files::CredentialDirectory,
    handle: &str,
  ) -> Result<Option<crate::config::Credential>, Error> {
    match directory.read(handle) {
      Err(Error::NotConfigured) => Ok(None),
      Err(error) => Err(error),
      Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| Error::Denied),
    }
  }
  fn validate_scope(
    credential: &crate::config::Credential,
    target: &crate::GithubCredentialTarget,
  ) -> Result<(), Error> {
    if credential.class != target.class.wire()
      || credential
        .repositories
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        != target.repositories.iter().collect::<std::collections::BTreeSet<_>>()
      || credential.repositories.len() != target.repositories.len()
    {
      return Err(Error::Denied);
    }
    Ok(())
  }
  fn encode(
    target: &crate::GithubCredentialTarget,
    token: &str,
    publication: &PrivatePublication,
  ) -> Result<zeroize::Zeroizing<Vec<u8>>, Error> {
    if !publication.abandoned && publication.kind != Kind::Revoke && !crate::config::valid_credential_token(token) {
      return Err(Error::Denied);
    }
    crate::config::encode_installed_credential(target.class.wire(), token, &target.repositories, Some(publication))
  }

  fn check_current(
    current: Option<&crate::config::Credential>,
    target: &crate::GithubCredentialTarget,
    stamp: &CredentialPublication,
  ) -> Result<bool, Error> {
    if let Some(current) = current {
      validate_scope(current, target)?;
      if let Some(published) = &current.publication {
        if published.abandoned
          || (published.kind != Kind::Revoke && !crate::config::valid_credential_token(&current.token))
          || (published.kind == Kind::Revoke && !current.token.is_empty())
        {
          return Err(Error::Denied);
        }
        if published.stamp.reference != stamp.reference {
          return Err(Error::Denied);
        }
        if published.stamp == *stamp {
          return Ok(true);
        }
        if published.stamp.generation >= stamp.generation {
          return Err(Error::Conflict);
        }
      }
    }
    Ok(false)
  }

  #[async_trait::async_trait]
  impl ProviderCredentialPublisher for GithubCredentialManager {
    async fn prepare(
      &self,
      stamp: &CredentialPublication,
      kind: Kind,
      value: Option<ProviderCredentialValue>,
    ) -> Result<(), Error> {
      let target = self.targets.get(&stamp.reference).ok_or(Error::NotConfigured)?.clone();
      let directory = self.directory.clone();
      let stamp = stamp.clone();
      tokio::task::spawn_blocking(move || {
        let _lock = directory.lock()?;
        let current = read(&directory, &target.handle)?;
        if check_current(current.as_ref(), &target, &stamp)? {
          if current
            .as_ref()
            .and_then(|c| c.publication.as_ref())
            .is_some_and(|p| p.kind == kind)
          {
            return Ok(());
          }
          return Err(Error::Conflict);
        }
        let handle = staged_handle(&target.handle, &stamp);
        if let Some(prepared) = read(&directory, &handle)? {
          validate_scope(&prepared, &target)?;
          if prepared
            .publication
            .as_ref()
            .is_some_and(|p| p.stamp == stamp && p.kind == kind && !p.abandoned)
          {
            return Ok(());
          }
          return Err(Error::Conflict);
        }
        let adopted;
        let token = match kind {
          Kind::Provision | Kind::Rotate => {
            if kind == Kind::Provision
              && current
                .as_ref()
                .is_some_and(|c| c.publication.as_ref().is_none_or(|p| p.kind != Kind::Revoke))
            {
              return Err(Error::Conflict);
            }
            if kind == Kind::Rotate
              && !current
                .as_ref()
                .is_some_and(|c| crate::config::valid_credential_token(&c.token))
            {
              return Err(Error::NotConfigured);
            }
            let bytes = value.as_ref().ok_or(Error::NotConfigured)?.expose_for_installation();
            let token = std::str::from_utf8(bytes).map_err(|_| Error::InvalidValue)?;
            if !crate::config::valid_credential_token(token) {
              return Err(Error::InvalidValue);
            }
            token
          }
          Kind::Adopt => {
            adopted = current.ok_or(Error::NotConfigured)?;
            if !crate::config::valid_credential_token(&adopted.token) {
              return Err(Error::NotConfigured);
            }
            &adopted.token
          }
          Kind::Revoke => "",
        };
        let publication = PrivatePublication {
          stamp,
          kind,
          abandoned: false,
        };
        let bytes = encode(&target, token, &publication)?;
        directory.publish(&handle, &bytes, PublishMode::Create, Some(&publication.stamp))
      })
      .await
      .map_err(|_| Error::Unavailable)?
    }
    async fn activate(&self, stamp: &CredentialPublication) -> Result<(), Error> {
      let target = self.targets.get(&stamp.reference).ok_or(Error::NotConfigured)?.clone();
      let directory = self.directory.clone();
      let stamp = stamp.clone();
      tokio::task::spawn_blocking(move || {
        let _lock = directory.lock()?;
        let current = read(&directory, &target.handle)?;
        if check_current(current.as_ref(), &target, &stamp)? {
          return Ok(());
        }
        let handle = staged_handle(&target.handle, &stamp);
        let prepared = read(&directory, &handle)?.ok_or(Error::NotConfigured)?;
        validate_scope(&prepared, &target)?;
        let publication = prepared.publication.as_ref().ok_or(Error::Denied)?;
        if publication.stamp != stamp || publication.abandoned {
          return Err(Error::Conflict);
        }
        let bytes = encode(&target, &prepared.token, publication)?;
        directory.publish(
          &target.handle,
          &bytes,
          if current.is_some() {
            PublishMode::Rotate
          } else {
            PublishMode::Create
          },
          Some(&stamp),
        )
      })
      .await
      .map_err(|_| Error::Unavailable)?
    }
    async fn discard(&self, stamp: &CredentialPublication) -> Result<(), Error> {
      let target = self.targets.get(&stamp.reference).ok_or(Error::NotConfigured)?.clone();
      let directory = self.directory.clone();
      let stamp = stamp.clone();
      tokio::task::spawn_blocking(move || {
        let _lock = directory.lock()?;
        let handle = staged_handle(&target.handle, &stamp);
        let existing = read(&directory, &handle)?;
        if let Some(existing) = &existing {
          validate_scope(existing, &target)?;
          if existing.publication.as_ref().is_none_or(|p| p.stamp != stamp) {
            return Err(Error::Denied);
          }
        }
        let kind = existing
          .as_ref()
          .and_then(|c| c.publication.as_ref())
          .map_or(Kind::Revoke, |p| p.kind);
        let publication = PrivatePublication {
          stamp,
          kind,
          abandoned: true,
        };
        let bytes = encode(&target, "", &publication)?;
        directory.publish(
          &handle,
          &bytes,
          if existing.is_some() {
            PublishMode::Rotate
          } else {
            PublishMode::Create
          },
          Some(&publication.stamp),
        )?;
        directory.remove_temporary(&target.handle, Some(&publication.stamp))?;
        // Adoption also owns cleanup of an interrupted legacy write to this exact handle.
        directory.remove_temporary(&target.handle, None)
      })
      .await
      .map_err(|_| Error::Unavailable)?
    }
  }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[async_trait::async_trait]
impl octacity_server_secrets::ProviderCredentialPublisher for crate::GithubCredentialManager {
  async fn prepare(
    &self,
    _: &CredentialPublication,
    _: Kind,
    _: Option<octacity_server_secrets::ProviderCredentialValue>,
  ) -> Result<(), octacity_server_secrets::ProviderCredentialError> {
    Err(octacity_server_secrets::ProviderCredentialError::Unavailable)
  }
  async fn activate(&self, _: &CredentialPublication) -> Result<(), octacity_server_secrets::ProviderCredentialError> {
    Err(octacity_server_secrets::ProviderCredentialError::Unavailable)
  }
  async fn discard(&self, _: &CredentialPublication) -> Result<(), octacity_server_secrets::ProviderCredentialError> {
    Err(octacity_server_secrets::ProviderCredentialError::Unavailable)
  }
}
