//! Exact retained candidate verification and non-overwriting branch publication.
use crate::config::{Config, Credential, read_private};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_delivery_protocol::{ArtifactInput, DeliveryIntent, FailureClass};
use octacity_protocol::CapturedChangeSetManifestV1;
use processkit::{Command, OutputBufferPolicy, OverflowMode};
use sha2::{Digest as _, Sha256};
use std::{fs, time::Duration};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub(crate) struct Candidate<'a> {
  scratch: tempfile::TempDir,
  config: &'a Config,
  credential: &'a Credential,
  cancellation: &'a CancellationToken,
  remote: String,
}
impl<'a> Candidate<'a> {
  pub(crate) async fn prepare(
    config: &'a Config,
    credential: &'a Credential,
    intent: &DeliveryIntent,
    cancellation: &'a CancellationToken,
  ) -> Result<Self, FailureClass> {
    let manifest_bytes = input(config, &intent.manifest)?;
    let manifest =
      CapturedChangeSetManifestV1::decode_canonical(&manifest_bytes).map_err(|_| FailureClass::InvalidRequest)?;
    let bundle = input(config, &intent.bundle)?;
    if manifest.candidate_revision != intent.candidate_revision
      || manifest.bundle.sha256 != intent.bundle.sha256
      || manifest.bundle.size_bytes != intent.bundle.size_bytes
      || manifest
        .cumulative_changes
        .as_ref()
        .is_none_or(|changes| changes.base_revision != intent.base_revision)
    {
      return Err(FailureClass::Conflict);
    }
    let remote = config
      .repositories
      .get(&intent.repository.repository_locator)
      .ok_or(FailureClass::Permanent)?
      .remote
      .clone();
    let mut scratch = tempfile::Builder::new();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      scratch.permissions(fs::Permissions::from_mode(0o700));
    }
    let candidate = Self {
      scratch: scratch.tempdir().map_err(|_| FailureClass::Permanent)?,
      config,
      credential,
      cancellation,
      remote,
    };
    fs::write(candidate.scratch.path().join("candidate.bundle"), &*bundle).map_err(|_| FailureClass::Permanent)?;
    fs::create_dir(candidate.scratch.path().join("hooks")).map_err(|_| FailureClass::Permanent)?;
    candidate.run(&["init", "--bare", "repository.git"]).await?;
    candidate
      .transfer(&[
        "fetch",
        "--no-tags",
        "--no-write-fetch-head",
        &candidate.remote,
        &format!("{}:refs/octacity/base", intent.base_revision),
      ])
      .await?;
    let bundle_path = candidate.scratch.path().join("candidate.bundle");
    let bundle_path = bundle_path.to_str().ok_or(FailureClass::Permanent)?;
    candidate.run(&["bundle", "verify", bundle_path]).await?;
    let heads = candidate.run(&["bundle", "list-heads", bundle_path]).await?;
    let mut heads = heads.lines();
    if heads
      .next()
      .and_then(|line| line.split_once(' '))
      .is_none_or(|(sha, _)| sha != intent.candidate_revision)
      || heads.next().is_some()
    {
      return Err(FailureClass::Conflict);
    }
    candidate
      .run(&[
        "fetch",
        "--no-tags",
        "--no-write-fetch-head",
        bundle_path,
        &format!("{}:refs/octacity/candidate", intent.candidate_revision),
      ])
      .await?;
    if candidate
      .run(&["rev-parse", "refs/octacity/candidate^{commit}"])
      .await?
      .trim()
      != intent.candidate_revision
      || candidate
        .run(&["show", "--no-patch", "--format=%P", "refs/octacity/candidate"])
        .await?
        .trim()
        != manifest.base_revision
    {
      return Err(FailureClass::Conflict);
    }
    candidate
      .run(&[
        "merge-base",
        "--is-ancestor",
        &intent.base_revision,
        &intent.candidate_revision,
      ])
      .await?;
    Ok(candidate)
  }
  pub(crate) async fn publish_branch(&self, intent: &DeliveryIntent) -> Result<(), FailureClass> {
    // The empty lease is an atomic absent-ref precondition. An unknown response
    // is subsequently observed; no remote head is ever overwritten or rebased.
    self
      .transfer(&[
        "push",
        "--porcelain",
        &format!("--force-with-lease={}:", intent.target.candidate_reference),
        &self.remote,
        &format!("{}:{}", intent.candidate_revision, intent.target.candidate_reference),
      ])
      .await?;
    Ok(())
  }
  async fn run(&self, args: &[&str]) -> Result<String, FailureClass> {
    self.command(args, FailureClass::Conflict).await
  }
  async fn transfer(&self, args: &[&str]) -> Result<String, FailureClass> {
    // A nonzero network operation does not prove a precondition conflict:
    // remote acceptance may precede a lost acknowledgment. Replay observes it.
    self.command(args, FailureClass::Transient).await
  }
  async fn command(&self, args: &[&str], nonzero: FailureClass) -> Result<String, FailureClass> {
    self.config.verify_git().map_err(|_| FailureClass::Permanent)?;
    let basic = Zeroizing::new(format!("x-access-token:{}", self.credential.token.as_str()));
    let header = Zeroizing::new(format!("Authorization: Basic {}", STANDARD.encode(basic.as_bytes())));
    let mut command = Command::new(&self.config.git_path)
      .env_clear()
      .env("GIT_CONFIG_NOSYSTEM", "1")
      .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
      .env("GIT_TERMINAL_PROMPT", "0")
      .env("GIT_LFS_SKIP_SMUDGE", "1")
      .env("LC_ALL", "C")
      .env("GIT_CONFIG_COUNT", "1")
      .env("GIT_CONFIG_KEY_0", format!("http.{}.extraHeader", self.remote))
      .env("GIT_CONFIG_VALUE_0", &*header)
      .arg("--no-optional-locks")
      .arg("-c")
      .arg(format!(
        "core.hooksPath={}",
        self.scratch.path().join("hooks").display()
      ))
      .args([
        "-c",
        "credential.helper=",
        "-c",
        "http.followRedirects=false",
        "-c",
        "protocol.ext.allow=never",
        "-c",
        "maintenance.auto=false",
        "-c",
        "gc.auto=0",
      ])
      .current_dir(self.scratch.path())
      .timeout(Duration::from_secs(30))
      .cancel_on(self.cancellation.clone())
      .output_buffer(
        OutputBufferPolicy::unbounded()
          .with_max_bytes(4096)
          .with_overflow(OverflowMode::Error),
      );
    // Bundle reads are confined local files. The selected remote is HTTPS-only
    // in installed configuration; no user Git config, helper or hook is loaded.
    if self.scratch.path().join("repository.git").is_dir() {
      command = command.arg("--git-dir=repository.git");
    }
    let output = command.args(args.iter().copied()).output_bytes().await.map_err(|_| {
      if self.cancellation.is_cancelled() {
        FailureClass::Cancelled
      } else {
        FailureClass::Transient
      }
    })?;
    self.config.verify_git().map_err(|_| FailureClass::Permanent)?;
    if !output.is_success() {
      return Err(nonzero);
    }
    String::from_utf8(output.stdout().to_vec()).map_err(|_| FailureClass::ProtocolFault)
  }
}
fn input(config: &Config, input: &ArtifactInput) -> Result<Zeroizing<Vec<u8>>, FailureClass> {
  let path = config
    .input_directory
    .join(format!("{}.bin", hex::encode(Sha256::digest(&input.handle))));
  let bytes = read_private(&path, input.size_bytes).map_err(|_| FailureClass::InvalidRequest)?;
  if bytes.len() as u64 != input.size_bytes || hex::encode(Sha256::digest(&*bytes)) != input.sha256 {
    return Err(FailureClass::Conflict);
  }
  Ok(bytes)
}
