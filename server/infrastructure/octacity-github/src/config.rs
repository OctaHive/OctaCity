//! Provider-owned settings and scoped protected credential resolution.
use crate::GithubConfigError;
use octacity_delivery_protocol::DeliveryIntent;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::{
  collections::BTreeMap,
  fs,
  path::{Component, Path, PathBuf},
};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Repository {
  pub(crate) repository_id: String,
  pub(crate) owner: String,
  pub(crate) name: String,
  pub(crate) remote: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
  config_version: u16,
  api_base: String,
  api_version: String,
  #[serde(default)]
  api_root_certificates_pem: Vec<String>,
  git_path: PathBuf,
  git_sha256: String,
  credential_directory: PathBuf,
  input_directory: PathBuf,
  repositories: BTreeMap<String, Repository>,
}
pub(crate) struct Config {
  pub(crate) digest: String,
  pub(crate) api_base: url::Url,
  pub(crate) api_version: String,
  pub(crate) api_root_certificates: Vec<reqwest::Certificate>,
  pub(crate) git_path: PathBuf,
  pub(crate) git_sha256: String,
  pub(crate) credential_directory: PathBuf,
  pub(crate) input_directory: PathBuf,
  pub(crate) repositories: BTreeMap<String, Repository>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credential {
  class: String,
  #[serde(deserialize_with = "secret")]
  pub(crate) token: Zeroizing<String>,
  repositories: Vec<String>,
}
fn secret<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Zeroizing<String>, D::Error> {
  String::deserialize(d).map(Zeroizing::new)
}
impl Config {
  pub(crate) fn verify_git(&self) -> Result<(), GithubConfigError> {
    verify_executable(&self.git_path, &self.git_sha256)
  }
  pub(crate) fn load(directory: &Path) -> Result<Self, GithubConfigError> {
    let directory = directory.canonicalize().map_err(|_| GithubConfigError)?;
    private_directory(&directory)?;
    let bytes = read_private(&directory.join("github-plugin.toml"), 64 * 1024)?;
    let wire: Wire =
      toml::from_str(std::str::from_utf8(&bytes).map_err(|_| GithubConfigError)?).map_err(|_| GithubConfigError)?;
    let api_base = https(&wire.api_base)?;
    if wire.api_root_certificates_pem.len() > 8 {
      return Err(GithubConfigError);
    }
    let mut api_root_certificates = Vec::with_capacity(wire.api_root_certificates_pem.len());
    for pem in wire.api_root_certificates_pem {
      if pem.len() > 16 * 1024 {
        return Err(GithubConfigError);
      }
      let certificates = reqwest::Certificate::from_pem_bundle(pem.as_bytes()).map_err(|_| GithubConfigError)?;
      if certificates.len() != 1 {
        return Err(GithubConfigError);
      }
      api_root_certificates.extend(certificates);
    }
    if wire.config_version != 1
      || wire.api_version.is_empty()
      || wire.api_version.len() > 32
      || wire.api_version.chars().any(char::is_control)
      || !wire.git_path.is_absolute()
      || wire.repositories.is_empty()
      || wire.repositories.len() > 128
      || wire.repositories.iter().any(|(key, repo)| {
        key.is_empty()
          || key.len() > 1024
          || repo.repository_id.is_empty()
          || !segment(&repo.owner)
          || !segment(&repo.name)
          || https(&repo.remote).is_err()
      })
    {
      return Err(GithubConfigError);
    }
    let git_path = wire.git_path.canonicalize().map_err(|_| GithubConfigError)?;
    verify_executable(&git_path, &wire.git_sha256)?;
    let private = |path: &Path| {
      if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
      {
        return Err(GithubConfigError);
      }
      let selected = directory.join(path);
      private_directory(&selected)?;
      let selected = selected.canonicalize().map_err(|_| GithubConfigError)?;
      if !selected.starts_with(&directory) {
        return Err(GithubConfigError);
      }
      Ok(selected)
    };
    Ok(Self {
      digest: hex::encode(Sha256::digest(&*bytes)),
      api_base,
      api_version: wire.api_version,
      api_root_certificates,
      git_path,
      git_sha256: wire.git_sha256,
      credential_directory: private(&wire.credential_directory)?,
      input_directory: private(&wire.input_directory)?,
      repositories: wire.repositories,
    })
  }
  pub(crate) fn authorize_network(&self, intent: &DeliveryIntent) -> Result<(), GithubConfigError> {
    let repository = self
      .repositories
      .get(&intent.repository.repository_locator)
      .ok_or(GithubConfigError)?;
    let remote = url::Url::parse(&repository.remote).map_err(|_| GithubConfigError)?;
    for endpoint in [&self.api_base, &remote] {
      // Installed configuration admits HTTPS only. Local file URLs are used by
      // private transport fixtures; they perform no network activity.
      if endpoint.scheme() == "file" && endpoint.host().is_none() {
        continue;
      }
      let host = match endpoint.host().ok_or(GithubConfigError)? {
        url::Host::Domain(host) => host.to_owned(),
        url::Host::Ipv4(host) => host.to_string(),
        url::Host::Ipv6(host) => host.to_string(),
      };
      if !intent.network_hosts.contains(&host) {
        return Err(GithubConfigError);
      }
    }
    Ok(())
  }
  pub(crate) fn delivery_credential(&self, intent: &DeliveryIntent) -> Result<Credential, GithubConfigError> {
    let repo = self
      .repositories
      .get(&intent.repository.repository_locator)
      .ok_or(GithubConfigError)?;
    if repo.repository_id != intent.repository.repository_id {
      return Err(GithubConfigError);
    }
    let path = self.credential_directory.join(format!(
      "{}.json",
      hex::encode(Sha256::digest(&intent.repository.credential.handle))
    ));
    let bytes = read_private(&path, 64 * 1024)?;
    let credential: Credential = serde_json::from_slice(&bytes).map_err(|_| GithubConfigError)?;
    if credential.class != "delivery_write"
      || credential.token.is_empty()
      || credential.token.len() > 8192
      || credential.token.chars().any(char::is_control)
      || credential.repositories.len() > 128
      || !credential.repositories.contains(&intent.repository.repository_locator)
    {
      return Err(GithubConfigError);
    }
    Ok(credential)
  }
}
fn verify_executable(path: &Path, expected: &str) -> Result<(), GithubConfigError> {
  use std::io::Read as _;
  let mut file = fs::File::open(path).map_err(|_| GithubConfigError)?;
  let metadata = file.metadata().map_err(|_| GithubConfigError)?;
  if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
    return Err(GithubConfigError);
  }
  let mut digest = Sha256::new();
  let mut buffer = [0; 8192];
  let mut remaining = 256 * 1024 * 1024 + 1;
  loop {
    let n = file.read(&mut buffer).map_err(|_| GithubConfigError)?;
    if n == 0 {
      break;
    }
    remaining -= n;
    if remaining == 0 {
      return Err(GithubConfigError);
    }
    digest.update(&buffer[..n]);
  }
  if hex::encode(digest.finalize()) != expected {
    return Err(GithubConfigError);
  }
  Ok(())
}
pub(crate) fn read_private(path: &Path, maximum: u64) -> Result<Zeroizing<Vec<u8>>, GithubConfigError> {
  let metadata = fs::symlink_metadata(path).map_err(|_| GithubConfigError)?;
  if !metadata.file_type().is_file() || metadata.len() > maximum {
    return Err(GithubConfigError);
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o077 != 0 {
      return Err(GithubConfigError);
    }
  }
  use std::io::Read as _;
  let mut bytes = Zeroizing::new(Vec::new());
  fs::File::open(path)
    .map_err(|_| GithubConfigError)?
    .take(maximum + 1)
    .read_to_end(&mut bytes)
    .map_err(|_| GithubConfigError)?;
  if bytes.len() as u64 > maximum {
    return Err(GithubConfigError);
  }
  Ok(bytes)
}
fn private_directory(path: &Path) -> Result<(), GithubConfigError> {
  let metadata = fs::symlink_metadata(path).map_err(|_| GithubConfigError)?;
  if !metadata.file_type().is_dir() {
    return Err(GithubConfigError);
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o077 != 0 {
      return Err(GithubConfigError);
    }
  }
  Ok(())
}
fn segment(value: &str) -> bool {
  !value.is_empty() && value.len() <= 100 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn https(value: &str) -> Result<url::Url, GithubConfigError> {
  let url = url::Url::parse(value).map_err(|_| GithubConfigError)?;
  if url.scheme() != "https"
    || url.host_str().is_none()
    || !url.username().is_empty()
    || url.password().is_some()
    || url.query().is_some()
    || url.fragment().is_some()
  {
    return Err(GithubConfigError);
  }
  Ok(url)
}
