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

pub(crate) const MAX_CREDENTIAL_FILE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_CREDENTIAL_TOKEN_BYTES: usize = 8192;
pub(crate) const MAX_CREDENTIAL_RESOURCES: usize = 128;

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
  pub(crate) class: String,
  #[serde(deserialize_with = "secret")]
  pub(crate) token: Zeroizing<String>,
  pub(crate) repositories: Vec<String>,
  #[serde(default)]
  pub(crate) publication: Option<crate::credential_publication::PrivatePublication>,
}
fn secret<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Zeroizing<String>, D::Error> {
  String::deserialize(d).map(Zeroizing::new)
}
impl std::fmt::Debug for Credential {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Credential")
      .field("class", &self.class)
      .finish_non_exhaustive()
  }
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
    let api = api_settings(wire.api_base, wire.api_version, wire.api_root_certificates_pem)?;
    if wire.config_version != 1
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
    Ok(Self {
      digest: hex::encode(Sha256::digest(&*bytes)),
      api_base: api.base,
      api_version: api.version,
      api_root_certificates: api.root_certificates,
      git_path,
      git_sha256: wire.git_sha256,
      credential_directory: private_subdirectory(&directory, &wire.credential_directory)?,
      input_directory: private_subdirectory(&directory, &wire.input_directory)?,
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
    read_credential(
      &self.credential_directory,
      "delivery_write",
      &intent.repository.credential.handle,
      &intent.repository.repository_locator,
    )
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
pub(crate) fn private_directory(path: &Path) -> Result<(), GithubConfigError> {
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
pub(crate) fn segment(value: &str) -> bool {
  !value.is_empty() && value.len() <= 100 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

pub(crate) struct ApiSettings {
  pub(crate) base: url::Url,
  pub(crate) version: String,
  pub(crate) root_certificates: Vec<reqwest::Certificate>,
}
pub(crate) fn api_settings(base: String, version: String, pems: Vec<String>) -> Result<ApiSettings, GithubConfigError> {
  let base = https(&base)?;
  if version.is_empty() || version.len() > 32 || version.chars().any(char::is_control) || pems.len() > 8 {
    return Err(GithubConfigError);
  }
  let mut roots = Vec::with_capacity(pems.len());
  for pem in pems {
    if pem.len() > 16 * 1024 {
      return Err(GithubConfigError);
    }
    let certificates = reqwest::Certificate::from_pem_bundle(pem.as_bytes()).map_err(|_| GithubConfigError)?;
    if certificates.len() != 1 {
      return Err(GithubConfigError);
    }
    roots.extend(certificates);
  }
  Ok(ApiSettings {
    base,
    version,
    root_certificates: roots,
  })
}
pub(crate) fn private_subdirectory(directory: &Path, relative: &Path) -> Result<PathBuf, GithubConfigError> {
  if relative.as_os_str().is_empty()
    || relative.is_absolute()
    || !relative.components().all(|c| matches!(c, Component::Normal(_)))
  {
    return Err(GithubConfigError);
  }
  let selected = directory.join(relative);
  private_directory(&selected)?;
  let selected = selected.canonicalize().map_err(|_| GithubConfigError)?;
  if !selected.starts_with(directory) {
    return Err(GithubConfigError);
  }
  Ok(selected)
}
pub(crate) fn read_credential(
  directory: &Path,
  class: &str,
  handle: &str,
  repository: &str,
) -> Result<Credential, GithubConfigError> {
  let path = directory.join(credential_file_name(handle));
  let bytes = read_private(&path, MAX_CREDENTIAL_FILE_BYTES as u64)?;
  let credential = decode_credential(&bytes)?;
  if credential.class != class || !credential.repositories.iter().any(|alias| alias == repository) {
    return Err(GithubConfigError);
  }
  Ok(credential)
}
fn decode_credential(bytes: &[u8]) -> Result<Credential, GithubConfigError> {
  let credential: Credential = serde_json::from_slice(bytes).map_err(|_| GithubConfigError)?;
  if !valid_credential_token(&credential.token)
    || credential.repositories.len() > MAX_CREDENTIAL_RESOURCES
    || credential
      .publication
      .as_ref()
      .is_some_and(|p| p.abandoned || p.kind == octacity_server_secrets::CredentialPublicationKind::Revoke)
  {
    return Err(GithubConfigError);
  }
  Ok(credential)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn validate_installed_credential(
  bytes: &[u8],
  class: &str,
  repositories: &[String],
) -> Result<(), GithubConfigError> {
  let credential = decode_credential(bytes)?;
  let expected = repositories.iter().collect::<std::collections::BTreeSet<_>>();
  let actual = credential
    .repositories
    .iter()
    .collect::<std::collections::BTreeSet<_>>();
  if credential.class != class || expected != actual || actual.len() != credential.repositories.len() {
    return Err(GithubConfigError);
  }
  Ok(())
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

pub(crate) fn credential_file_name(handle: &str) -> String {
  format!("{}.json", hex::encode(Sha256::digest(handle)))
}
pub(crate) fn valid_credential_token(token: &str) -> bool {
  !token.is_empty() && token.len() <= MAX_CREDENTIAL_TOKEN_BYTES && !token.chars().any(char::is_control)
}

// Stream borrowed secret bytes directly into a zeroing, bounded private document.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn encode_installed_credential(
  class: &str,
  token: &str,
  repositories: &[String],
  publication: Option<&crate::credential_publication::PrivatePublication>,
) -> Result<Zeroizing<Vec<u8>>, octacity_server_secrets::ProviderCredentialError> {
  use octacity_server_secrets::ProviderCredentialError as Error;
  #[derive(serde::Serialize)]
  struct Wire<'a> {
    class: &'a str,
    token: &'a str,
    repositories: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    publication: Option<&'a crate::credential_publication::PrivatePublication>,
  }
  let mut bytes = Zeroizing::new(Vec::new());
  serde_json::to_writer(
    &mut *bytes,
    &Wire {
      class,
      token,
      repositories,
      publication,
    },
  )
  .map_err(|_| Error::Unavailable)?;
  if bytes.len() > MAX_CREDENTIAL_FILE_BYTES {
    return Err(Error::Denied);
  }
  Ok(bytes)
}
