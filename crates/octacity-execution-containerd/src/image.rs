//! Resolves immutable OCI image metadata and derives the rootfs chain.

use super::*;

/// Pulls and unpacks exactly the requested platform into the configured snapshotter.
pub(super) async fn pull_and_unpack(
  client: &Client,
  config: &ContainerdEngineConfig,
  platform: ExecutionPlatform,
  reference: &str,
  deadline: Instant,
  cancellation: &CancellationToken,
) -> Result<(), ExecutionError> {
  let platform = containerd_platform(platform);
  let resolver = containerd_client::types::transfer::RegistryResolver {
    host_dir: config
      .registry_config_dir
      .as_ref()
      .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
    ..Default::default()
  };
  let source = OciRegistry {
    reference: reference.to_owned(),
    resolver: Some(resolver),
  };
  let destination = ImageStore {
    name: reference.to_owned(),
    platforms: vec![platform.clone()],
    unpacks: vec![UnpackConfiguration {
      platform: Some(platform),
      snapshotter: config.snapshotter.clone(),
    }],
    ..Default::default()
  };
  grpc_before(
    deadline,
    Some(cancellation),
    "pull and unpack OCI image",
    client.transfer().transfer(namespaced(
      TransferRequest {
        source: Some(to_any(&source)),
        destination: Some(to_any(&destination)),
        options: Some(TransferOptions::default()),
      },
      &config.namespace,
    )?),
  )
  .await?;
  Ok(())
}

/// Resolves the pulled digest to a platform-specific rootfs chain and process environment.
///
/// Containerd's stored target is checked against the signed digest before any
/// index, manifest, or config content is trusted.
pub(super) async fn image_configuration(
  client: &Client,
  namespace: &str,
  reference: &str,
  platform: ExecutionPlatform,
  deadline: Instant,
  cancellation: &CancellationToken,
) -> Result<ResolvedImage, ExecutionError> {
  let image = grpc_before(
    deadline,
    Some(cancellation),
    "resolve pulled OCI image",
    client.images().get(namespaced(
      GetImageRequest {
        name: reference.to_owned(),
      },
      namespace,
    )?),
  )
  .await?
  .into_inner()
  .image
  .ok_or_else(|| backend("containerd returned an empty image record"))?;
  let mut descriptor = image
    .target
    .ok_or_else(|| backend("containerd image has no target descriptor"))?;
  let expected_digest = reference
    .rsplit_once('@')
    .map(|(_, digest)| digest)
    .ok_or_else(|| invalid("OCI image reference does not contain a digest"))?;
  if descriptor.digest != expected_digest {
    return Err(unavailable(format!(
      "containerd resolved image target '{}' instead of signed digest '{expected_digest}'",
      descriptor.digest
    )));
  }
  if is_index_media_type(&descriptor.media_type) {
    let index: ImageIndex =
      serde_json::from_slice(&read_content(client, namespace, &descriptor.digest, deadline, cancellation).await?)
        .map_err(|error| backend(format!("decode OCI image index: {error}")))?;
    descriptor = index
      .manifests
      .into_iter()
      .find(|candidate| {
        candidate
          .platform
          .as_ref()
          .is_some_and(|candidate| candidate.matches(platform))
      })
      .ok_or_else(|| unavailable(format!("OCI image does not contain platform {platform:?}")))?
      .into_containerd_descriptor();
  }
  if !is_manifest_media_type(&descriptor.media_type) {
    return Err(unavailable(format!(
      "unsupported OCI target media type '{}'",
      descriptor.media_type
    )));
  }
  let manifest: ImageManifest =
    serde_json::from_slice(&read_content(client, namespace, &descriptor.digest, deadline, cancellation).await?)
      .map_err(|error| backend(format!("decode OCI image manifest: {error}")))?;
  let config: ImageConfiguration =
    serde_json::from_slice(&read_content(client, namespace, &manifest.config.digest, deadline, cancellation).await?)
      .map_err(|error| backend(format!("decode OCI image configuration: {error}")))?;
  if !config.matches(platform) {
    return Err(unavailable(format!(
      "OCI image configuration does not match requested platform {platform:?}"
    )));
  }
  if config.rootfs.kind != "layers" {
    return Err(unavailable("OCI image rootfs type must be 'layers'"));
  }
  Ok(ResolvedImage {
    chain_id: chain_id(&config.rootfs.diff_ids)?,
    environment: process_environment(config.config.environment)?,
  })
}

async fn read_content(
  client: &Client,
  namespace: &str,
  digest: &str,
  deadline: Instant,
  cancellation: &CancellationToken,
) -> Result<Vec<u8>, ExecutionError> {
  // OCI metadata is attacker-controlled even after digest verification. Bound
  // accumulation before deserializing it into richer structures.
  validate_digest(digest)?;
  let mut stream = grpc_before(
    deadline,
    Some(cancellation),
    "read OCI metadata from containerd",
    client.content().read(namespaced(
      containerd_client::services::v1::ReadContentRequest {
        digest: digest.to_owned(),
        offset: 0,
        size: 0,
      },
      namespace,
    )?),
  )
  .await?
  .into_inner();
  let mut bytes = Vec::new();
  while let Some(chunk) = grpc_before(
    deadline,
    Some(cancellation),
    "stream OCI metadata from containerd",
    stream.message(),
  )
  .await?
  {
    if bytes.len().saturating_add(chunk.data.len()) > MAX_IMAGE_METADATA_BYTES {
      return Err(unavailable("OCI image metadata exceeds the 8 MiB limit"));
    }
    bytes.extend_from_slice(&chunk.data);
  }
  Ok(bytes)
}

#[derive(Deserialize)]
struct ImageIndex {
  manifests: Vec<ImageDescriptor>,
}

#[derive(Deserialize)]
struct ImageManifest {
  config: ImageDescriptor,
}

#[derive(Deserialize)]
struct ImageDescriptor {
  #[serde(rename = "mediaType")]
  media_type: String,
  digest: String,
  #[serde(default)]
  platform: Option<ImagePlatform>,
}

impl ImageDescriptor {
  fn into_containerd_descriptor(self) -> containerd_client::types::Descriptor {
    containerd_client::types::Descriptor {
      media_type: self.media_type,
      digest: self.digest,
      ..Default::default()
    }
  }
}

#[derive(Deserialize)]
/// Platform selector embedded in an OCI image index.
pub(super) struct ImagePlatform {
  pub(super) os: String,
  pub(super) architecture: String,
}

impl ImagePlatform {
  pub(super) fn matches(&self, platform: ExecutionPlatform) -> bool {
    self.os == execution_os(platform.os) && self.architecture == execution_architecture(platform.architecture)
  }
}

#[derive(Deserialize)]
struct ImageConfiguration {
  os: String,
  architecture: String,
  rootfs: ImageRootFs,
  #[serde(default)]
  config: ImageProcessConfiguration,
}

impl ImageConfiguration {
  fn matches(&self, platform: ExecutionPlatform) -> bool {
    self.os == execution_os(platform.os) && self.architecture == execution_architecture(platform.architecture)
  }
}

#[derive(Deserialize)]
struct ImageRootFs {
  #[serde(rename = "type")]
  kind: String,
  diff_ids: Vec<String>,
}

/// Image data needed after registry and content-store resolution completes.
pub(super) struct ResolvedImage {
  pub(super) chain_id: String,
  pub(super) environment: Vec<String>,
}

#[derive(Default, Deserialize)]
struct ImageProcessConfiguration {
  #[serde(default, rename = "Env")]
  environment: Vec<String>,
}

/// Validates image-provided environment entries and overrides sandbox-owned values.
///
/// A deterministic map removes duplicate names. `HOME` belongs to OctaCity,
/// while a missing `PATH` receives a portable default.
pub(super) fn process_environment(values: Vec<String>) -> Result<Vec<String>, ExecutionError> {
  let mut environment = std::collections::BTreeMap::new();
  for value in values {
    let Some((name, _)) = value.split_once('=') else {
      return Err(unavailable("OCI image environment contains an entry without '='"));
    };
    if name.is_empty() || name.contains('\0') || value.contains('\0') {
      return Err(unavailable(
        "OCI image environment contains an invalid name or NUL byte",
      ));
    }
    environment.insert(name.to_owned(), value);
  }
  environment
    .entry("PATH".to_owned())
    .or_insert_with(|| "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_owned());
  environment.insert("HOME".to_owned(), "HOME=/workspace".to_owned());
  Ok(environment.into_values().collect())
}

/// Computes the overlay snapshot parent key defined by the OCI chain-ID algorithm.
pub(super) fn chain_id(diff_ids: &[String]) -> Result<String, ExecutionError> {
  let mut ids = diff_ids.iter();
  let first = ids
    .next()
    .ok_or_else(|| unavailable("OCI image rootfs contains no diff IDs"))?;
  validate_digest(first)?;
  let mut chain = first.clone();
  for diff_id in ids {
    validate_digest(diff_id)?;
    chain = format!("sha256:{:x}", Sha256::digest(format!("{chain} {diff_id}").as_bytes()));
  }
  Ok(chain)
}

/// Accepts only canonical lowercase SHA-256 digests used by signed job specs.
pub(super) fn validate_digest(value: &str) -> Result<(), ExecutionError> {
  let Some(digest) = value.strip_prefix("sha256:") else {
    return Err(unavailable("OCI metadata requires sha256 digests"));
  };
  if digest.len() != 64
    || !digest
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
  {
    return Err(unavailable("OCI metadata contains an invalid sha256 digest"));
  }
  Ok(())
}

/// Recognizes the OCI and Docker index media types supported by the resolver.
pub(super) fn is_index_media_type(value: &str) -> bool {
  matches!(
    value,
    "application/vnd.oci.image.index.v1+json" | "application/vnd.docker.distribution.manifest.list.v2+json"
  )
}

/// Recognizes the OCI and Docker image-manifest media types supported by the resolver.
pub(super) fn is_manifest_media_type(value: &str) -> bool {
  matches!(
    value,
    "application/vnd.oci.image.manifest.v1+json" | "application/vnd.docker.distribution.manifest.v2+json"
  )
}

/// Maps the backend-neutral platform into containerd's API representation.
pub(super) fn containerd_platform(platform: ExecutionPlatform) -> Platform {
  Platform {
    os: execution_os(platform.os).to_owned(),
    architecture: execution_architecture(platform.architecture).to_owned(),
    variant: String::new(),
    os_version: String::new(),
  }
}

pub(super) fn execution_os(os: ExecutionOs) -> &'static str {
  match os {
    ExecutionOs::Linux => "linux",
    ExecutionOs::Windows => "windows",
    ExecutionOs::Macos => "darwin",
  }
}

pub(super) fn execution_architecture(architecture: ExecutionArchitecture) -> &'static str {
  match architecture {
    ExecutionArchitecture::Amd64 => "amd64",
    ExecutionArchitecture::Arm64 => "arm64",
  }
}
