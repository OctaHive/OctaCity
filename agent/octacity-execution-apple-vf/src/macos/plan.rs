//! Converts one backend-neutral request into a fixed Apple container plan.

use std::{
  fs,
  path::{Path, PathBuf},
};

use octacity_execution::{
  CACHE_CA_CERTIFICATE_PATH, CACHE_DIRECTORY_PATH, CACHE_TOKEN_PATH, ExecutionError, ExecutionPaths, ExecutionTarget,
  RunnerProgram, StartExecution, WORKLOAD_IDENTITY_PATH,
};
use sha2::{Digest as _, Sha256};

use super::{backend, invalid};

const GUEST_RELEASE: &str = "/opt/octacity/octa";
const GUEST_TOOLS: &str = "/opt/octacity/tools";
const GUEST_WORKSPACE: &str = "/workspace";
const MAX_MARKER_BYTES: u64 = 128;

pub(super) struct AppleVfPlan {
  pub(super) container_name: String,
  pub(super) image: String,
  pub(super) paths: ExecutionPaths,
  pub(super) marker: PathBuf,
  create_arguments: Vec<String>,
}

impl AppleVfPlan {
  pub(super) fn build(
    owner_identity: &str,
    marker_root: &Path,
    open_files_limit: u64,
    runner: &RunnerProgram,
    request: &StartExecution,
  ) -> Result<Self, ExecutionError> {
    let ExecutionTarget::Oci { reference, .. } = &request.root else {
      return Err(invalid("Apple VF isolation requires an OCI image"));
    };
    let container_name = container_name(owner_identity, &request.execution_id);
    let marker = marker_root.join(format!("{container_name}.owner"));
    let paths = guest_paths(runner, request)?;
    let executable = map_path(&runner.release_root, &runner.executable, Path::new(GUEST_RELEASE))?;
    let cpus = request.cpu_millis / 1000;
    let memory_mib = exact_mib("memory", request.memory_bytes)?;
    let temporary_mib = (memory_mib / 8).clamp(1, 1024);
    let mut arguments = vec![
      "create".to_owned(),
      "--name".to_owned(),
      container_name.clone(),
      "--label".to_owned(),
      format!("octacity.owner={owner_identity}"),
      "--platform".to_owned(),
      "linux/arm64".to_owned(),
      "--cpus".to_owned(),
      cpus.to_string(),
      "--memory".to_owned(),
      format!("{memory_mib}M"),
      "--network".to_owned(),
      "none".to_owned(),
      "--no-dns".to_owned(),
      "--read-only".to_owned(),
      "--cap-drop".to_owned(),
      "ALL".to_owned(),
      "--ulimit".to_owned(),
      format!("nofile={open_files_limit}:{open_files_limit}"),
      "--shm-size".to_owned(),
      "64M".to_owned(),
      "--mount".to_owned(),
      format!("type=tmpfs,target=/tmp,size={temporary_mib}M"),
      "--mount".to_owned(),
      "type=tmpfs,target=/run,size=16M".to_owned(),
      "--workdir".to_owned(),
      GUEST_WORKSPACE.to_owned(),
    ];
    push_directory_bind(&mut arguments, &request.workspace, GUEST_WORKSPACE, false)?;
    push_directory_bind(&mut arguments, &runner.release_root, GUEST_RELEASE, true)?;
    for projection in runner.external_executable_projections() {
      let guest = projection
        .destination(Path::new(GUEST_TOOLS))
        .to_string_lossy()
        .into_owned();
      push_readonly_file_bind(&mut arguments, &projection.source, &guest)?;
      arguments.push("--env".to_owned());
      arguments.push(format!("{}={guest}", projection.selector));
    }
    if let Some(identity) = &request.workload_identity {
      push_readonly_file_bind(&mut arguments, identity, WORKLOAD_IDENTITY_PATH)?;
    }
    if let Some(cache) = &request.cache {
      push_directory_bind(&mut arguments, &cache.local_directory, CACHE_DIRECTORY_PATH, false)?;
      if let Some(token) = &cache.token_file {
        push_readonly_file_bind(&mut arguments, token, CACHE_TOKEN_PATH)?;
      }
      if let Some(certificate) = &cache.ca_certificate_file {
        push_readonly_file_bind(&mut arguments, certificate, CACHE_CA_CERTIFICATE_PATH)?;
      }
    }
    arguments.push(reference.clone());
    arguments.push(executable);
    Ok(Self {
      container_name,
      image: reference.clone(),
      paths,
      marker,
      create_arguments: arguments,
    })
  }

  pub(super) fn pull_arguments(&self) -> Vec<String> {
    vec![
      "image".to_owned(),
      "pull".to_owned(),
      "--progress".to_owned(),
      "none".to_owned(),
      "--platform".to_owned(),
      "linux/arm64".to_owned(),
      self.image.clone(),
    ]
  }

  pub(super) fn create_arguments(&self) -> Vec<String> {
    self.create_arguments.clone()
  }

  pub(super) fn create_marker(&self) -> Result<(), ExecutionError> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut file = fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .mode(0o600)
      .open(&self.marker)
      .map_err(ExecutionError::Io)?;
    file
      .write_all(self.container_name.as_bytes())
      .map_err(ExecutionError::Io)?;
    file.sync_all().map_err(ExecutionError::Io)
  }

  pub(super) fn remove_marker(&self) -> Result<(), ExecutionError> {
    match fs::remove_file(&self.marker) {
      Ok(()) => Ok(()),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(error) => Err(ExecutionError::Io(error)),
    }
  }
}

pub(super) fn owner_identity(agent_id: &str) -> String {
  let digest = Sha256::digest(agent_id.as_bytes());
  hex_prefix(&digest, 12)
}

pub(super) fn container_from_marker(marker: &Path, owner_identity: &str) -> Result<Option<String>, ExecutionError> {
  let Some(name) = marker.file_name().and_then(|name| name.to_str()) else {
    return Ok(None);
  };
  let Some(expected_container) = name.strip_suffix(".owner") else {
    return Ok(None);
  };
  if !valid_container_name(expected_container)
    || !expected_container.starts_with(&format!("octacity-{owner_identity}-"))
  {
    return Err(backend(format!(
      "invalid Apple VF cleanup marker '{}'; refusing deletion",
      marker.display()
    )));
  }
  let metadata = fs::symlink_metadata(marker).map_err(ExecutionError::Io)?;
  if !metadata.file_type().is_file() || metadata.len() > MAX_MARKER_BYTES {
    return Err(backend(format!(
      "invalid Apple VF cleanup marker '{}'; refusing deletion",
      marker.display()
    )));
  }
  let recorded = fs::read_to_string(marker).map_err(ExecutionError::Io)?;
  if recorded != expected_container {
    return Err(backend(format!(
      "Apple VF cleanup marker '{}' does not match its resource name",
      marker.display()
    )));
  }
  Ok(Some(recorded))
}

fn container_name(owner_identity: &str, execution_id: &str) -> String {
  let digest = Sha256::digest(execution_id.as_bytes());
  format!("octacity-{owner_identity}-{}", hex_prefix(&digest, 32))
}

fn valid_container_name(value: &str) -> bool {
  !value.is_empty()
    && value
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn hex_prefix(bytes: &[u8], characters: usize) -> String {
  let mut encoded = String::with_capacity(characters);
  for byte in bytes {
    use std::fmt::Write as _;
    write!(&mut encoded, "{byte:02x}").expect("writing into String cannot fail");
    if encoded.len() >= characters {
      encoded.truncate(characters);
      break;
    }
  }
  encoded
}

fn guest_paths(runner: &RunnerProgram, request: &StartExecution) -> Result<ExecutionPaths, ExecutionError> {
  Ok(ExecutionPaths {
    workspace: PathBuf::from(GUEST_WORKSPACE),
    data_dir: PathBuf::from(map_path(
      &request.workspace,
      &request.data_dir,
      Path::new(GUEST_WORKSPACE),
    )?),
    plugins_dir: PathBuf::from(map_path(
      &runner.release_root,
      &runner.plugins_dir,
      Path::new(GUEST_RELEASE),
    )?),
    plugin_lock: PathBuf::from(map_path(
      &runner.release_root,
      &runner.plugin_lock,
      Path::new(GUEST_RELEASE),
    )?),
    cache: request.cache.as_ref().map(|cache| cache.projected_paths()),
  })
}

fn map_path(host_root: &Path, host_path: &Path, guest_root: &Path) -> Result<String, ExecutionError> {
  let relative = host_path
    .strip_prefix(host_root)
    .map_err(|_| invalid("mapped path is outside its host root"))?;
  let relative = relative
    .to_str()
    .ok_or_else(|| invalid("mapped host path is not UTF-8"))?;
  let guest = if relative.is_empty() {
    guest_root.to_owned()
  } else {
    guest_root.join(relative)
  };
  guest
    .to_str()
    .map(str::to_owned)
    .ok_or_else(|| invalid("mapped guest path is not UTF-8"))
}

fn push_directory_bind(
  arguments: &mut Vec<String>,
  source: &Path,
  target: &str,
  readonly: bool,
) -> Result<(), ExecutionError> {
  let source = source
    .to_str()
    .ok_or_else(|| invalid("Apple VF bind source is not UTF-8"))?;
  if source.contains(',') {
    return Err(invalid("Apple VF bind sources must not contain commas"));
  }
  let readonly = if readonly { ",readonly" } else { "" };
  arguments.push("--mount".to_owned());
  arguments.push(format!("type=bind,source={source},target={target}{readonly}"));
  Ok(())
}

fn push_readonly_file_bind(arguments: &mut Vec<String>, source: &Path, target: &str) -> Result<(), ExecutionError> {
  let source = source
    .to_str()
    .ok_or_else(|| invalid("Apple VF bind source is not UTF-8"))?;
  if source.contains(':') {
    return Err(invalid("Apple VF file bind sources must not contain colons"));
  }
  arguments.push("--volume".to_owned());
  arguments.push(format!("{source}:{target}:ro"));
  Ok(())
}

fn exact_mib(name: &str, bytes: u64) -> Result<u64, ExecutionError> {
  const MEBIBYTE: u64 = 1024 * 1024;
  if !bytes.is_multiple_of(MEBIBYTE) {
    return Err(ExecutionError::Unavailable(format!(
      "Apple VF {name} limit must use whole MiB"
    )));
  }
  Ok(bytes / MEBIBYTE)
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use octacity_execution::{
    ExecutionArchitecture, ExecutionCacheMounts, ExecutionOs, ExecutionPlatform, LocalCacheCapacity, NetworkAccess,
    OciIsolation,
  };

  use super::*;

  #[test]
  fn plan_keeps_provider_details_outside_the_execution_target() {
    let temporary = tempfile::tempdir().unwrap();
    let release = temporary.path().join("release");
    let workspace = temporary.path().join("workspace");
    let identity = temporary.path().join("identity");
    let cache = temporary.path().join("cache");
    let cache_token = temporary.path().join("cache-token");
    let cache_ca = temporary.path().join("cache-ca.pem");
    fs::create_dir_all(release.join("plugins")).unwrap();
    fs::create_dir_all(workspace.join("data")).unwrap();
    fs::create_dir(&cache).unwrap();
    for file in [release.join("octa-runner"), release.join("Octa.lock")] {
      fs::write(file, "fixture").unwrap();
    }
    for file in [&identity, &cache_token, &cache_ca] {
      fs::write(file, "fixture").unwrap();
    }
    let runner = RunnerProgram {
      release_root: release.clone(),
      executable: release.join("octa-runner"),
      plugins_dir: release.join("plugins"),
      plugin_lock: release.join("Octa.lock"),
      external_executables: std::collections::BTreeMap::new(),
    };
    let request = StartExecution {
      execution_id: "job-1".to_owned(),
      workspace_root: temporary.path().to_owned(),
      workspace: workspace.clone(),
      data_dir: workspace.join("data"),
      workload_identity: Some(identity.clone()),
      cache: Some(ExecutionCacheMounts {
        capacity_root: cache.clone(),
        local_directory: cache,
        local_capacity: LocalCacheCapacity::new(4 * 1024 * 1024, 3 * 1024 * 1024, 2 * 1024 * 1024).unwrap(),
        aggregate_max_bytes: 8 * 1024 * 1024,
        token_file: Some(cache_token.clone()),
        ca_certificate_file: Some(cache_ca.clone()),
      }),
      cpu_millis: 2000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(30),
      root: ExecutionTarget::Oci {
        reference: format!("example.invalid/octa@sha256:{}", "a".repeat(64)),
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Arm64,
        },
        isolation: OciIsolation::Process,
      },
      network: NetworkAccess::Disabled,
    };

    let plan = AppleVfPlan::build("owner", temporary.path(), 1024, &runner, &request).unwrap();
    assert_eq!(plan.paths.workspace, Path::new(GUEST_WORKSPACE));
    assert!(plan.create_arguments.iter().any(|argument| argument == "none"));
    assert!(plan.create_arguments.iter().any(|argument| argument == "--read-only"));
    assert!(plan.create_arguments.iter().any(|argument| argument == "--cap-drop"));
    for expected in [
      format!("{}:{WORKLOAD_IDENTITY_PATH}:ro", identity.display()),
      format!("{}:{CACHE_TOKEN_PATH}:ro", cache_token.display()),
      format!("{}:{CACHE_CA_CERTIFICATE_PATH}:ro", cache_ca.display()),
    ] {
      assert!(
        plan
          .create_arguments
          .windows(2)
          .any(|arguments| arguments == ["--volume", expected.as_str()])
      );
    }
    assert!(
      !plan
        .create_arguments
        .iter()
        .any(|argument| argument == "--virtualization")
    );
  }

  #[test]
  fn regular_files_use_apple_volume_syntax() {
    let temporary = tempfile::tempdir().unwrap();
    let identity = temporary.path().join("identity");
    fs::write(&identity, "fixture").unwrap();
    let mut arguments = Vec::new();

    push_readonly_file_bind(&mut arguments, &identity, WORKLOAD_IDENTITY_PATH).unwrap();

    assert_eq!(
      arguments,
      [
        "--volume".to_owned(),
        format!("{}:{WORKLOAD_IDENTITY_PATH}:ro", identity.display())
      ]
    );
  }

  #[test]
  fn cleanup_marker_cannot_name_an_unowned_container() {
    let temporary = tempfile::tempdir().unwrap();
    let marker = temporary.path().join("foreign.owner");
    fs::write(&marker, "foreign").unwrap();
    assert!(container_from_marker(&marker, "owner").is_err());
  }

  #[test]
  fn cleanup_marker_must_be_a_small_regular_file() {
    let temporary = tempfile::tempdir().unwrap();
    let container = "octacity-owner-0123456789abcdef";
    let marker = temporary.path().join(format!("{container}.owner"));
    fs::write(&marker, "x".repeat(MAX_MARKER_BYTES as usize + 1)).unwrap();
    assert!(container_from_marker(&marker, "owner").is_err());
  }
}
