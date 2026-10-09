use std::{
  collections::BTreeSet,
  fs,
  path::{Component, Path, PathBuf},
  time::{Duration, SystemTime},
};

use octacity_private_fs::{
  create_private_directory, read_bounded_regular_file, validate_private_access, validate_trusted_owner,
};
use octacity_protocol::{PROTECTED_INPUT_ROOT, ProtectedInputManifestV3, ProtectedInputTransferV3};
use reqwest::{StatusCode, Url, header::HeaderMap};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{io::AsyncWriteExt as _, time::Instant};
use tokio_util::sync::CancellationToken;

/// Agent-owned transport policy for immutable protected inputs.
#[derive(Clone, Debug)]
pub struct ProtectedInputStagerConfig {
  /// Exact HTTPS origins (or loopback HTTP origins) allowed for download.
  pub allowed_origins: Vec<String>,
  /// Per-input upper bound covering connection and body transfer.
  pub download_timeout: Duration,
}

/// Downloads and publishes immutable v3 inputs below one private Job root.
pub struct ProtectedInputStager {
  client: reqwest::Client,
  allowed_origins: BTreeSet<String>,
  download_timeout: Duration,
}

impl ProtectedInputStager {
  /// Validates local transport policy and constructs a redirect-free client.
  pub fn new(config: ProtectedInputStagerConfig) -> Result<Self, ProtectedInputError> {
    if config.allowed_origins.is_empty() || config.download_timeout.is_zero() {
      return Err(ProtectedInputError::Policy);
    }
    let allowed_origins = config
      .allowed_origins
      .iter()
      .map(|value| canonical_origin(value))
      .collect::<Result<BTreeSet<_>, _>>()?;
    if allowed_origins.len() != config.allowed_origins.len() {
      return Err(ProtectedInputError::Policy);
    }
    let client = reqwest::Client::builder()
      .connect_timeout(config.download_timeout)
      .redirect(reqwest::redirect::Policy::none())
      .build()
      .map_err(|_| ProtectedInputError::Transport)?;
    Ok(Self {
      client,
      allowed_origins,
      download_timeout: config.download_timeout,
    })
  }

  /// Downloads every exact transfer, atomically publishes validated files,
  /// and seals the resulting subtree read-only.
  pub async fn stage(
    &self,
    transfers: &[ProtectedInputTransferV3],
    job_root: &Path,
    cancellation: &CancellationToken,
  ) -> Result<StagedProtectedInputs, ProtectedInputError> {
    let manifest = ProtectedInputManifestV3 {
      inputs: transfers.iter().map(|transfer| transfer.input.clone()).collect(),
    };
    manifest.validate().map_err(|_| ProtectedInputError::Manifest)?;
    for transfer in transfers {
      transfer
        .capability
        .validate()
        .map_err(|_| ProtectedInputError::Capability)?;
    }
    validate_private_access(job_root).map_err(|_| ProtectedInputError::Filesystem)?;
    validate_trusted_owner(job_root).map_err(|_| ProtectedInputError::Filesystem)?;

    let root = job_root.join("protected");
    let staging = job_root.join(".protected-input-staging");
    if root.exists() || staging.exists() {
      return Err(ProtectedInputError::Collision);
    }
    create_private_directory(&root).map_err(|_| ProtectedInputError::Filesystem)?;
    if let Err(error) = create_private_directory(&staging).map_err(|_| ProtectedInputError::Filesystem) {
      cleanup_private_tree(&root).map_err(|_| ProtectedInputError::Cleanup)?;
      return Err(error);
    }

    let operation = self.download_all(transfers, &root, &staging, cancellation).await;
    let staging_cleanup = cleanup_private_tree(&staging);
    if operation.is_err() || staging_cleanup.is_err() {
      let root_cleanup = cleanup_private_tree(&root);
      return match (operation, staging_cleanup, root_cleanup) {
        (Err(operation), Ok(()), Ok(())) => Err(operation),
        _ => Err(ProtectedInputError::Cleanup),
      };
    }
    if seal_tree(&root).is_err() {
      return match cleanup_private_tree(&root) {
        Ok(()) => Err(ProtectedInputError::Filesystem),
        Err(_) => Err(ProtectedInputError::Cleanup),
      };
    }
    let staged = StagedProtectedInputs { root, manifest };
    if let Err(error) = staged.revalidate() {
      return cleanup_failed_publication(&staged.root, error);
    }
    Ok(staged)
  }

  async fn download_all(
    &self,
    transfers: &[ProtectedInputTransferV3],
    root: &Path,
    staging: &Path,
    cancellation: &CancellationToken,
  ) -> Result<(), ProtectedInputError> {
    for (index, transfer) in transfers.iter().enumerate() {
      let relative = protected_relative_path(&transfer.input.destination)?;
      let target = root.join(&relative);
      let parent = target.parent().ok_or(ProtectedInputError::Manifest)?;
      create_private_ancestors(root, parent)?;
      if fs::symlink_metadata(&target).is_ok() {
        return Err(ProtectedInputError::Collision);
      }
      let temporary = staging.join(format!("input-{index}"));
      self.download_one(transfer, &temporary, cancellation).await?;
      set_read_only(&temporary, true)?;
      fs::rename(&temporary, &target).map_err(|_| ProtectedInputError::Filesystem)?;
    }
    Ok(())
  }

  async fn download_one(
    &self,
    transfer: &ProtectedInputTransferV3,
    temporary: &Path,
    cancellation: &CancellationToken,
  ) -> Result<(), ProtectedInputError> {
    let url = self.validate_capability(transfer)?;
    let headers =
      transfer
        .capability
        .required_headers
        .iter()
        .try_fold(HeaderMap::new(), |mut headers, (name, value)| {
          let name = reqwest::header::HeaderName::try_from(name).map_err(|_| ProtectedInputError::Capability)?;
          let value = reqwest::header::HeaderValue::try_from(value).map_err(|_| ProtectedInputError::Capability)?;
          headers.insert(name, value);
          Ok::<_, ProtectedInputError>(headers)
        })?;
    let deadline = Instant::now()
      .checked_add(self.download_timeout)
      .ok_or(ProtectedInputError::Policy)?;
    let mut response = tokio::select! {
      () = cancellation.cancelled() => return Err(ProtectedInputError::Cancelled),
      response = tokio::time::timeout_at(deadline, self.client.get(url).headers(headers).send()) => {
        response.map_err(|_| ProtectedInputError::TimedOut)?
          .map_err(|_| ProtectedInputError::Transport)?
      }
    };
    if response.status() != StatusCode::OK {
      return Err(ProtectedInputError::Transport);
    }
    let media_type = response
      .headers()
      .get(reqwest::header::CONTENT_TYPE)
      .and_then(|value| value.to_str().ok())
      .ok_or(ProtectedInputError::MediaType)?;
    if media_type != transfer.input.media_type {
      return Err(ProtectedInputError::MediaType);
    }
    if response
      .content_length()
      .is_some_and(|length| length != transfer.input.size_bytes)
    {
      return Err(ProtectedInputError::Size);
    }

    let mut file = tokio::fs::OpenOptions::new()
      .create_new(true)
      .write(true)
      .open(temporary)
      .await
      .map_err(|_| ProtectedInputError::Filesystem)?;
    let mut received = 0_u64;
    let mut digest = Sha256::new();
    loop {
      let chunk = tokio::select! {
        () = cancellation.cancelled() => return Err(ProtectedInputError::Cancelled),
        chunk = tokio::time::timeout_at(deadline, response.chunk()) => {
          chunk.map_err(|_| ProtectedInputError::TimedOut)?
            .map_err(|_| ProtectedInputError::Transport)?
        }
      };
      let Some(chunk) = chunk else { break };
      received = received
        .checked_add(u64::try_from(chunk.len()).map_err(|_| ProtectedInputError::Size)?)
        .ok_or(ProtectedInputError::Size)?;
      if received > transfer.input.size_bytes {
        return Err(ProtectedInputError::Size);
      }
      digest.update(&chunk);
      file
        .write_all(&chunk)
        .await
        .map_err(|_| ProtectedInputError::Filesystem)?;
    }
    file.flush().await.map_err(|_| ProtectedInputError::Filesystem)?;
    file.sync_all().await.map_err(|_| ProtectedInputError::Filesystem)?;
    drop(file);
    if received != transfer.input.size_bytes {
      return Err(ProtectedInputError::Size);
    }
    if hex::encode(digest.finalize()) != transfer.input.sha256 {
      return Err(ProtectedInputError::Digest);
    }
    Ok(())
  }

  fn validate_capability(&self, transfer: &ProtectedInputTransferV3) -> Result<Url, ProtectedInputError> {
    let expires = SystemTime::UNIX_EPOCH
      .checked_add(Duration::from_millis(transfer.capability.expires_at_unix_ms))
      .ok_or(ProtectedInputError::Capability)?;
    if expires <= SystemTime::now() {
      return Err(ProtectedInputError::Capability);
    }
    let url = Url::parse(&transfer.capability.url).map_err(|_| ProtectedInputError::Capability)?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
      return Err(ProtectedInputError::Capability);
    }
    if !self.allowed_origins.contains(&url.origin().ascii_serialization()) {
      return Err(ProtectedInputError::Capability);
    }
    Ok(url)
  }
}

/// Sealed protected subtree whose exact contents can be checked before spawn.
#[derive(Debug)]
pub struct StagedProtectedInputs {
  root: PathBuf,
  manifest: ProtectedInputManifestV3,
}

impl StagedProtectedInputs {
  /// Revalidates the sealed tree and returns its host root only on success.
  /// Callers must invoke this immediately before mapping the tree read-only at
  /// [`PROTECTED_INPUT_ROOT`] for runner start.
  pub fn revalidate_for_runner(&self) -> Result<&Path, ProtectedInputError> {
    self.revalidate()?;
    Ok(&self.root)
  }

  /// Resolves one signed logical Artifact to its sealed host path.
  ///
  /// The complete tree is revalidated first, so trusted preprocessing cannot
  /// consume bytes that differ from those later projected to the runner.
  pub fn path_for_input(&self, artifact_id: &str) -> Result<PathBuf, ProtectedInputError> {
    self.revalidate()?;
    let input = self
      .manifest
      .inputs
      .iter()
      .find(|input| input.artifact_id == artifact_id)
      .ok_or(ProtectedInputError::Manifest)?;
    Ok(self.root.join(protected_relative_path(&input.destination)?))
  }

  /// Removes the protected tree, including portable read-only attributes.
  pub fn cleanup(self) -> Result<(), ProtectedInputError> {
    cleanup_private_tree(&self.root).map_err(|_| ProtectedInputError::Cleanup)
  }

  fn revalidate(&self) -> Result<(), ProtectedInputError> {
    validate_private_access(&self.root).map_err(|_| ProtectedInputError::Filesystem)?;
    validate_trusted_owner(&self.root).map_err(|_| ProtectedInputError::Filesystem)?;
    require_directory(&self.root)?;
    let expected = self
      .manifest
      .inputs
      .iter()
      .map(|input| protected_relative_path(&input.destination))
      .collect::<Result<BTreeSet<_>, _>>()?;
    if collect_files(&self.root)? != expected {
      return Err(ProtectedInputError::Collision);
    }
    for input in &self.manifest.inputs {
      let path = self.root.join(protected_relative_path(&input.destination)?);
      let metadata = fs::symlink_metadata(&path).map_err(|_| ProtectedInputError::Filesystem)?;
      if !metadata.file_type().is_file() || !metadata.permissions().readonly() || metadata.len() != input.size_bytes {
        return Err(ProtectedInputError::Mutation);
      }
      let contents = read_bounded_regular_file(&path, input.size_bytes.saturating_add(1))
        .map_err(|_| ProtectedInputError::Mutation)?;
      if contents.len() as u64 != input.size_bytes || hex::encode(Sha256::digest(&contents)) != input.sha256 {
        return Err(ProtectedInputError::Mutation);
      }
    }
    Ok(())
  }
}

fn canonical_origin(value: &str) -> Result<String, ProtectedInputError> {
  let url = Url::parse(value).map_err(|_| ProtectedInputError::Policy)?;
  let local_http = url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "::1" | "localhost"));
  if (url.scheme() != "https" && !local_http)
    || !url.username().is_empty()
    || url.password().is_some()
    || url.path() != "/"
    || url.query().is_some()
    || url.fragment().is_some()
  {
    return Err(ProtectedInputError::Policy);
  }
  Ok(url.origin().ascii_serialization())
}

fn protected_relative_path(destination: &str) -> Result<PathBuf, ProtectedInputError> {
  let relative = destination
    .strip_prefix(PROTECTED_INPUT_ROOT)
    .and_then(|value| value.strip_prefix('/'))
    .ok_or(ProtectedInputError::Manifest)?;
  let path = Path::new(relative);
  if relative.is_empty()
    || path
      .components()
      .any(|component| !matches!(component, Component::Normal(_)))
  {
    return Err(ProtectedInputError::Manifest);
  }
  Ok(path.to_owned())
}

fn create_private_ancestors(root: &Path, parent: &Path) -> Result<(), ProtectedInputError> {
  let relative = parent.strip_prefix(root).map_err(|_| ProtectedInputError::Manifest)?;
  let mut current = root.to_owned();
  for component in relative.components() {
    let Component::Normal(component) = component else {
      return Err(ProtectedInputError::Manifest);
    };
    current.push(component);
    match fs::symlink_metadata(&current) {
      Ok(metadata) if metadata.file_type().is_dir() && !is_redirect(&current, &metadata)? => {}
      Ok(_) => return Err(ProtectedInputError::Collision),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        create_private_directory(&current).map_err(|_| ProtectedInputError::Filesystem)?;
      }
      Err(_) => return Err(ProtectedInputError::Filesystem),
    }
  }
  Ok(())
}

fn collect_files(root: &Path) -> Result<BTreeSet<PathBuf>, ProtectedInputError> {
  let mut files = BTreeSet::new();
  let mut pending = vec![root.to_owned()];
  while let Some(directory) = pending.pop() {
    for entry in fs::read_dir(&directory).map_err(|_| ProtectedInputError::Filesystem)? {
      let entry = entry.map_err(|_| ProtectedInputError::Filesystem)?;
      let path = entry.path();
      let metadata = fs::symlink_metadata(&path).map_err(|_| ProtectedInputError::Filesystem)?;
      if is_redirect(&path, &metadata)? {
        return Err(ProtectedInputError::Mutation);
      }
      if metadata.file_type().is_dir() {
        if !metadata.permissions().readonly() {
          return Err(ProtectedInputError::Mutation);
        }
        pending.push(path);
      } else if metadata.file_type().is_file() {
        files.insert(
          path
            .strip_prefix(root)
            .map_err(|_| ProtectedInputError::Filesystem)?
            .to_owned(),
        );
      } else {
        return Err(ProtectedInputError::Mutation);
      }
    }
  }
  Ok(files)
}

fn require_directory(path: &Path) -> Result<(), ProtectedInputError> {
  let metadata = fs::symlink_metadata(path).map_err(|_| ProtectedInputError::Filesystem)?;
  if metadata.file_type().is_dir() && metadata.permissions().readonly() && !is_redirect(path, &metadata)? {
    Ok(())
  } else {
    Err(ProtectedInputError::Mutation)
  }
}

fn seal_tree(root: &Path) -> std::io::Result<()> {
  let mut directories = vec![root.to_owned()];
  let mut pending = vec![root.to_owned()];
  while let Some(directory) = pending.pop() {
    for entry in fs::read_dir(&directory)? {
      let path = entry?.path();
      let metadata = fs::symlink_metadata(&path)?;
      if is_redirect_io(&path, &metadata)? {
        return Err(std::io::Error::other("redirect in protected input tree"));
      }
      if metadata.is_dir() {
        directories.push(path.clone());
        pending.push(path);
      } else if metadata.is_file() {
        set_read_only(&path, true).map_err(|_| std::io::Error::other("cannot seal protected input"))?;
      } else {
        return Err(std::io::Error::other("special file in protected input tree"));
      }
    }
  }
  directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
  for directory in directories {
    set_read_only(&directory, true).map_err(|_| std::io::Error::other("cannot seal protected input tree"))?;
  }
  Ok(())
}

fn cleanup_private_tree(root: &Path) -> std::io::Result<()> {
  let metadata = match fs::symlink_metadata(root) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => return Err(error),
  };
  if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
    return Err(std::io::Error::other("protected input root is not a directory"));
  }
  let mut pending = vec![root.to_owned()];
  while let Some(directory) = pending.pop() {
    set_read_only(&directory, false).map_err(|_| std::io::Error::other("cannot unseal protected input tree"))?;
    for entry in fs::read_dir(&directory)? {
      let path = entry?.path();
      let metadata = fs::symlink_metadata(&path)?;
      if is_redirect_io(&path, &metadata)? {
        return Err(std::io::Error::other("redirect in protected input tree"));
      }
      if metadata.is_dir() {
        pending.push(path);
      } else if metadata.is_file() {
        set_read_only(&path, false).map_err(|_| std::io::Error::other("cannot unseal protected input"))?;
      } else {
        return Err(std::io::Error::other("special file in protected input tree"));
      }
    }
  }
  fs::remove_dir_all(root)
}

fn cleanup_failed_publication<T>(root: &Path, error: ProtectedInputError) -> Result<T, ProtectedInputError> {
  match cleanup_private_tree(root) {
    Ok(()) => Err(error),
    Err(_) => Err(ProtectedInputError::Cleanup),
  }
}

fn set_read_only(path: &Path, value: bool) -> Result<(), ProtectedInputError> {
  let mut permissions = fs::symlink_metadata(path)
    .map_err(|_| ProtectedInputError::Filesystem)?
    .permissions();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = permissions.mode();
    permissions.set_mode(if value { mode & !0o222 } else { mode | 0o200 });
  }
  #[cfg(windows)]
  permissions.set_readonly(value);
  fs::set_permissions(path, permissions).map_err(|_| ProtectedInputError::Filesystem)
}

fn is_redirect(path: &Path, metadata: &fs::Metadata) -> Result<bool, ProtectedInputError> {
  if metadata.file_type().is_symlink() {
    return Ok(true);
  }
  #[cfg(windows)]
  {
    octacity_private_fs::is_reparse_point(path).map_err(|_| ProtectedInputError::Filesystem)
  }
  #[cfg(not(windows))]
  {
    let _ = path;
    Ok(false)
  }
}

fn is_redirect_io(path: &Path, metadata: &fs::Metadata) -> std::io::Result<bool> {
  if metadata.file_type().is_symlink() {
    return Ok(true);
  }
  #[cfg(windows)]
  {
    octacity_private_fs::is_reparse_point(path)
  }
  #[cfg(not(windows))]
  {
    let _ = path;
    Ok(false)
  }
}

/// Stable protected-input failures that never contain URLs, credentials,
/// source contents, host paths, or HTTP response bodies.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ProtectedInputError {
  /// Local transport policy is absent or unsafe.
  #[error("protected input policy is invalid")]
  Policy,
  /// Signed destinations or immutable metadata are invalid.
  #[error("protected input manifest is invalid")]
  Manifest,
  /// A short-lived download grant is malformed, expired, or outside policy.
  #[error("protected input capability is invalid")]
  Capability,
  /// An existing path conflicts with the reserved subtree.
  #[error("protected input destination collides with existing state")]
  Collision,
  /// The download failed without exposing its capability or response.
  #[error("protected input transport failed")]
  Transport,
  /// The bounded download did not finish in time.
  #[error("protected input download timed out")]
  TimedOut,
  /// The operation was cancelled.
  #[error("protected input download was cancelled")]
  Cancelled,
  /// The response media type did not match signed metadata.
  #[error("protected input media type did not match")]
  MediaType,
  /// The response was shorter or longer than signed metadata.
  #[error("protected input size did not match")]
  Size,
  /// The downloaded bytes did not match signed metadata.
  #[error("protected input digest did not match")]
  Digest,
  /// Private staging or atomic publication could not complete.
  #[error("protected input filesystem operation failed")]
  Filesystem,
  /// Published protected state changed before runner start.
  #[error("protected input changed before runner start")]
  Mutation,
  /// Failure cleanup could not safely remove partial protected state.
  #[error("protected input cleanup failed")]
  Cleanup,
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeMap;
  use std::io::{Read as _, Write as _};

  use octacity_private_fs::create_private_directory;
  use octacity_protocol::{ArtifactTransferCapability, ProtectedInputV3};
  use tempfile::TempDir;

  use super::*;

  fn private_job_root() -> (TempDir, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("job");
    create_private_directory(&root).unwrap();
    (temporary, root)
  }

  fn input(body: &[u8], destination: &str) -> ProtectedInputV3 {
    ProtectedInputV3 {
      artifact_id: "artifact-1".to_owned(),
      size_bytes: body.len() as u64,
      sha256: hex::encode(Sha256::digest(body)),
      media_type: "application/json".to_owned(),
      destination: destination.to_owned(),
    }
  }

  fn transfer(input: ProtectedInputV3, url: String) -> ProtectedInputTransferV3 {
    let expires_at_unix_ms = SystemTime::now()
      .duration_since(SystemTime::UNIX_EPOCH)
      .unwrap()
      .as_millis()
      .saturating_add(60_000) as u64;
    ProtectedInputTransferV3 {
      input,
      capability: ArtifactTransferCapability {
        url,
        required_headers: BTreeMap::new(),
        expires_at_unix_ms,
      },
    }
  }

  async fn server(body: &'static [u8], media_type: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::task::spawn_blocking(move || {
      let listener = listener.into_std().unwrap();
      listener.set_nonblocking(false).unwrap();
      let (mut socket, _) = listener.accept().unwrap();
      let mut request = [0_u8; 4096];
      let _ = socket.read(&mut request).unwrap();
      write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: {media_type}\r\nConnection: close\r\n\r\n"
      )
      .unwrap();
      socket.write_all(body).unwrap();
    });
    (format!("http://{address}"), task)
  }

  fn stager(origin: &str) -> ProtectedInputStager {
    ProtectedInputStager::new(ProtectedInputStagerConfig {
      allowed_origins: vec![origin.to_owned()],
      download_timeout: Duration::from_secs(2),
    })
    .unwrap()
  }

  #[tokio::test]
  async fn stages_exact_read_only_bytes_and_detects_replacement_before_runner_start() {
    let body = br#"{"task":"implement"}"#;
    let (origin, server) = server(body, "application/json").await;
    let (_temporary, job_root) = private_job_root();
    let staged = stager(&origin)
      .stage(
        &[transfer(
          input(body, "/octacity/protected/context/task.json"),
          format!("{origin}/artifact?signature=secret"),
        )],
        &job_root,
        &CancellationToken::new(),
      )
      .await
      .unwrap();
    server.await.unwrap();

    let root = staged.revalidate_for_runner().unwrap();
    assert_eq!(fs::read(root.join("context/task.json")).unwrap(), body);
    set_read_only(root, false).unwrap();
    set_read_only(&root.join("context"), false).unwrap();
    set_read_only(&root.join("context/task.json"), false).unwrap();
    fs::write(root.join("context/task.json"), br#"{"task":"tampered"}"#).unwrap();
    seal_tree(root).unwrap();
    assert_eq!(
      staged.revalidate_for_runner().unwrap_err(),
      ProtectedInputError::Mutation
    );
    staged.cleanup().unwrap();
  }

  #[tokio::test]
  async fn rejects_short_reads_and_removes_partial_state_without_exposing_capabilities() {
    let body = b"short";
    let declared = b"a longer protected input";
    let (origin, server) = server(body, "application/json").await;
    let (_temporary, job_root) = private_job_root();
    let secret_url = format!("{origin}/artifact?signature=do-not-log");
    let error = stager(&origin)
      .stage(
        &[transfer(
          input(declared, "/octacity/protected/task.json"),
          secret_url.clone(),
        )],
        &job_root,
        &CancellationToken::new(),
      )
      .await
      .unwrap_err();
    server.await.unwrap();

    assert_eq!(error, ProtectedInputError::Size);
    assert!(!format!("{error:?} {error}").contains("do-not-log"));
    assert!(!job_root.join("protected").exists());
    assert!(!job_root.join(".protected-input-staging").exists());
  }

  #[tokio::test]
  async fn rejects_digest_and_media_mismatches() {
    for (body, media_type, expected) in [
      (b"wrong".as_slice(), "application/json", ProtectedInputError::Digest),
      (b"right".as_slice(), "text/plain", ProtectedInputError::MediaType),
    ] {
      let (origin, server) = server(body, media_type).await;
      let (_temporary, job_root) = private_job_root();
      let mut expected_input = input(body, "/octacity/protected/task.json");
      expected_input.media_type = "application/json".to_owned();
      if expected == ProtectedInputError::Digest {
        expected_input.sha256 = "0".repeat(64);
      }
      let error = stager(&origin)
        .stage(
          &[transfer(expected_input, format!("{origin}/artifact"))],
          &job_root,
          &CancellationToken::new(),
        )
        .await
        .unwrap_err();
      server.await.unwrap();
      assert_eq!(error, expected);
      assert!(!job_root.join("protected").exists());
    }
  }

  #[tokio::test]
  async fn rejects_traversal_collisions_and_redirects_before_download() {
    let (_temporary, job_root) = private_job_root();
    let stager = stager("http://127.0.0.1:9");
    let traversal = transfer(
      input(b"x", "/octacity/protected/../source"),
      "http://127.0.0.1:9/artifact".to_owned(),
    );
    assert_eq!(
      stager
        .stage(&[traversal], &job_root, &CancellationToken::new())
        .await
        .unwrap_err(),
      ProtectedInputError::Manifest
    );

    create_private_directory(&job_root.join("protected")).unwrap();
    let valid = transfer(
      input(b"x", "/octacity/protected/task"),
      "http://127.0.0.1:9/artifact".to_owned(),
    );
    assert_eq!(
      stager
        .stage(&[valid], &job_root, &CancellationToken::new())
        .await
        .unwrap_err(),
      ProtectedInputError::Collision
    );

    #[cfg(unix)]
    {
      use std::os::unix::fs::symlink;
      let tree = job_root.join("redirect-test");
      create_private_directory(&tree).unwrap();
      symlink(job_root.parent().unwrap(), tree.join("escape")).unwrap();
      assert_eq!(
        create_private_ancestors(&tree, &tree.join("escape/child")).unwrap_err(),
        ProtectedInputError::Collision
      );
    }
  }

  #[cfg(unix)]
  #[test]
  fn cleanup_failures_are_secret_safe_and_never_follow_redirects() {
    use std::os::unix::fs::symlink;

    let (_temporary, job_root) = private_job_root();
    let protected = job_root.join("protected");
    create_private_directory(&protected).unwrap();
    symlink("/private/source/credential-value", protected.join("redirect")).unwrap();

    assert!(cleanup_private_tree(&protected).is_err());
    let error = ProtectedInputError::Cleanup;
    let diagnostic = format!("{error:?} {error}");
    assert!(!diagnostic.contains("private/source"));
    assert!(!diagnostic.contains("credential-value"));
    assert_eq!(
      cleanup_failed_publication::<()>(&protected, ProtectedInputError::Mutation),
      Err(ProtectedInputError::Cleanup)
    );
  }
}
