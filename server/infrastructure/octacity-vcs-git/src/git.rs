//! Git plumbing operations and provider-neutral result normalization.

use std::{
  collections::{BTreeMap, HashMap},
  ffi::OsString,
  fs,
  path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::Uri;
use octacity_vcs_protocol::{
  Command as VcsCommand, Commit, Failure, FailureClass, FileContent, ListReferences, ListTree, MAX_TEXT_BYTES, Outcome,
  ReadCommit, ReadFile, Reference, ReferenceKind, ReferencePage, Request, ResolveRevision, ResolvedRevision, TreeEntry,
  TreeEntryKind, TreePage,
};
use processkit::{Command, ErrorReason, OutputBufferPolicy, OverflowMode};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::config::{GitAdapterConfig, GitAdapterConfigError, null_device};

const RETRY_AFTER_MS: u64 = 1_000;

/// Read-only Git VCS adapter loaded from one trusted installation directory.
#[derive(Clone, Debug)]
pub struct GitVcsAdapter {
  config: GitAdapterConfig,
}

#[derive(Debug)]
enum GitError {
  Invalid(&'static str, &'static str),
  Permanent(&'static str, &'static str),
  Transient(&'static str, &'static str),
  Cancelled,
}

struct BareRepository {
  _temporary: TempDir,
  path: PathBuf,
  hooks: PathBuf,
}

impl GitVcsAdapter {
  /// Loads and validates the operator-owned adapter configuration.
  pub fn load(adapter_directory: &Path) -> Result<Self, GitAdapterConfigError> {
    Ok(Self {
      config: GitAdapterConfig::load(adapter_directory)?,
    })
  }

  /// Executes one validated provider-neutral request.
  pub async fn execute(&self, request: &Request, cancellation: CancellationToken) -> Outcome {
    let result = match &request.command {
      VcsCommand::ListReferences(value) => self
        .list_references(value, &cancellation)
        .await
        .map(Outcome::References),
      VcsCommand::ReadCommit(value) => self.read_commit(value, &cancellation).await.map(Outcome::Commit),
      VcsCommand::ListTree(value) => self.list_tree(value, &cancellation).await.map(Outcome::Tree),
      VcsCommand::ReadFile(value) => self.read_file(value, &cancellation).await.map(Outcome::File),
      VcsCommand::ResolveRevision(value) => self.resolve_revision(value, &cancellation).await.map(Outcome::Resolved),
      VcsCommand::Cancel(value) => {
        return Outcome::Acknowledged {
          operation_id: value.target_operation_id.clone(),
        };
      }
    };
    result.unwrap_or_else(|error| Outcome::Failure(error.into_failure()))
  }

  async fn list_references(
    &self,
    request: &ListReferences,
    cancellation: &CancellationToken,
  ) -> Result<ReferencePage, GitError> {
    let context = self.context(&request.repository, cancellation)?;
    let output = context
      .run(
        Path::new("."),
        "reference listing",
        [
          OsString::from("ls-remote"),
          OsString::from("--heads"),
          OsString::from("--tags"),
          OsString::from("--"),
          OsString::from(&request.repository.repository_locator),
        ],
        self.config.max_git_output_bytes,
      )
      .await?;
    let references = parse_references(&output)?;
    let filtered = references
      .into_iter()
      .filter(|entry| {
        request
          .prefix
          .as_ref()
          .is_none_or(|prefix| entry.name.starts_with(prefix))
      })
      .filter(|entry| request.cursor.as_ref().is_none_or(|cursor| entry.name > *cursor))
      .collect::<Vec<_>>();
    Ok(page_references(filtered, request.page_size))
  }

  async fn resolve_revision(
    &self,
    request: &ResolveRevision,
    cancellation: &CancellationToken,
  ) -> Result<ResolvedRevision, GitError> {
    if is_object_id(&request.reference) {
      let repository = self.clone_bare(&request.repository, cancellation).await?;
      self
        .verify_commit(&repository, &request.reference, cancellation)
        .await?;
      return Ok(ResolvedRevision {
        reference: request.reference.clone(),
        revision: request.reference.clone(),
      });
    }
    if request.reference != "HEAD"
      && !request.reference.starts_with("refs/heads/")
      && !request.reference.starts_with("refs/tags/")
    {
      return Err(GitError::Invalid(
        "unsupported_reference",
        "Git references must be HEAD or a fully qualified branch or tag",
      ));
    }
    let context = self.context(&request.repository, cancellation)?;
    let peeled = format!("{}^{{}}", request.reference);
    let output = context
      .run(
        Path::new("."),
        "revision resolution",
        [
          OsString::from("ls-remote"),
          OsString::from("--exit-code"),
          OsString::from("--"),
          OsString::from(&request.repository.repository_locator),
          OsString::from(&request.reference),
          OsString::from(peeled),
        ],
        self.config.max_git_output_bytes,
      )
      .await?;
    let revisions = parse_remote_lines(&output)?;
    let peeled_name = format!("{}^{{}}", request.reference);
    let revision = revisions
      .get(&peeled_name)
      .or_else(|| revisions.get(&request.reference))
      .ok_or(GitError::Permanent(
        "reference_not_found",
        "Git reference was not found",
      ))?;
    if !is_object_id(revision) {
      return Err(GitError::Permanent(
        "invalid_object_id",
        "Git returned a non-canonical object identifier",
      ));
    }
    Ok(ResolvedRevision {
      reference: request.reference.clone(),
      revision: revision.clone(),
    })
  }

  async fn read_commit(&self, request: &ReadCommit, cancellation: &CancellationToken) -> Result<Commit, GitError> {
    require_object_id(&request.revision)?;
    let repository = self.clone_bare(&request.repository, cancellation).await?;
    self.verify_commit(&repository, &request.revision, cancellation).await?;
    let context = self.context(&request.repository, cancellation)?;
    let output = context
      .run(
        repository.path.parent().unwrap_or(Path::new(".")),
        "commit read",
        git_dir_arguments(&repository.path, ["cat-file", "commit", request.revision.as_str()]),
        self.config.max_git_output_bytes,
      )
      .await?;
    parse_commit(&request.revision, &output)
  }

  async fn list_tree(&self, request: &ListTree, cancellation: &CancellationToken) -> Result<TreePage, GitError> {
    require_object_id(&request.revision)?;
    let repository = self.clone_bare(&request.repository, cancellation).await?;
    self.verify_commit(&repository, &request.revision, cancellation).await?;
    let treeish = request.path.as_ref().map_or_else(
      || request.revision.clone(),
      |path| format!("{}:{path}", request.revision),
    );
    let context = self.context(&request.repository, cancellation)?;
    let output = context
      .run(
        repository.path.parent().unwrap_or(Path::new(".")),
        "tree read",
        git_dir_arguments(&repository.path, ["ls-tree", "-z", "-l", treeish.as_str()]),
        self.config.max_git_output_bytes,
      )
      .await?;
    let entries = parse_tree(&output, request.path.as_deref())?
      .into_iter()
      .filter(|entry| request.cursor.as_ref().is_none_or(|cursor| entry.path > *cursor))
      .collect::<Vec<_>>();
    Ok(page_tree(entries, request.page_size))
  }

  async fn read_file(&self, request: &ReadFile, cancellation: &CancellationToken) -> Result<FileContent, GitError> {
    require_object_id(&request.revision)?;
    let repository = self.clone_bare(&request.repository, cancellation).await?;
    self.verify_commit(&repository, &request.revision, cancellation).await?;
    let object = format!("{}:{}", request.revision, request.path);
    let context = self.context(&request.repository, cancellation)?;
    let directory = repository.path.parent().unwrap_or(Path::new("."));
    let kind = context
      .run(
        directory,
        "file type read",
        git_dir_arguments(&repository.path, ["cat-file", "-t", object.as_str()]),
        256,
      )
      .await?;
    if trim_ascii(&kind) != b"blob" {
      return Err(GitError::Permanent("not_a_file", "requested Git path is not a file"));
    }
    let size = context
      .run(
        directory,
        "file size read",
        git_dir_arguments(&repository.path, ["cat-file", "-s", object.as_str()]),
        256,
      )
      .await?;
    let size = std::str::from_utf8(trim_ascii(&size))
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .ok_or(GitError::Permanent(
        "invalid_blob_size",
        "Git returned an invalid file size",
      ))?;
    if size > self.config.max_blob_bytes {
      return Err(GitError::Permanent(
        "blob_too_large",
        "Git file exceeds the configured browsing limit",
      ));
    }
    let content = context
      .run(
        directory,
        "file content read",
        git_dir_arguments(&repository.path, ["cat-file", "blob", object.as_str()]),
        self.config.max_blob_bytes,
      )
      .await?;
    let offset = usize::try_from(request.offset).unwrap_or(usize::MAX).min(content.len());
    let end = offset.saturating_add(request.max_bytes as usize).min(content.len());
    Ok(FileContent {
      path: request.path.clone(),
      offset: request.offset,
      content_base64: STANDARD.encode(&content[offset..end]),
      truncated: end < content.len(),
    })
  }

  async fn verify_commit(
    &self,
    repository: &BareRepository,
    revision: &str,
    cancellation: &CancellationToken,
  ) -> Result<(), GitError> {
    let context = GitContext {
      adapter: self,
      credential_config: None,
      hooks: repository.hooks.clone(),
      cancellation,
    };
    let object = format!("{revision}^{{commit}}");
    let output = context
      .run(
        repository.path.parent().unwrap_or(Path::new(".")),
        "commit verification",
        git_dir_arguments(
          &repository.path,
          ["rev-parse", "--verify", "--end-of-options", object.as_str()],
        ),
        256,
      )
      .await?;
    if trim_ascii(&output) != revision.as_bytes() {
      return Err(GitError::Permanent(
        "revision_not_found",
        "immutable Git commit was not found",
      ));
    }
    Ok(())
  }

  async fn clone_bare(
    &self,
    access: &octacity_vcs_protocol::RepositoryAccess,
    cancellation: &CancellationToken,
  ) -> Result<BareRepository, GitError> {
    let context = self.context(access, cancellation)?;
    let temporary = tempfile::Builder::new()
      .prefix("octacity-vcs-git-")
      .tempdir()
      .map_err(|_| GitError::Transient("temporary_storage", "temporary storage is unavailable"))?;
    let hooks = temporary.path().join("disabled-hooks");
    fs::create_dir(&hooks).map_err(|_| GitError::Transient("temporary_storage", "temporary storage is unavailable"))?;
    let path = temporary.path().join("repository.git");
    let context = GitContext {
      hooks: hooks.clone(),
      ..context
    };
    context
      .run(
        temporary.path(),
        "bare clone",
        [
          OsString::from("clone"),
          OsString::from("--quiet"),
          OsString::from("--bare"),
          OsString::from("--no-hardlinks"),
          OsString::from("--filter=blob:none"),
          OsString::from("--template="),
          OsString::from("--"),
          OsString::from(&access.repository_locator),
          path.as_os_str().to_owned(),
        ],
        self.config.max_git_output_bytes,
      )
      .await?;
    enforce_directory_limit(&path, self.config.max_repository_bytes)?;
    Ok(BareRepository {
      _temporary: temporary,
      path,
      hooks,
    })
  }

  fn context<'a>(
    &'a self,
    access: &octacity_vcs_protocol::RepositoryAccess,
    cancellation: &'a CancellationToken,
  ) -> Result<GitContext<'a>, GitError> {
    validate_remote(&access.repository_locator, self.config.allow_file)?;
    let credential_config = self
      .config
      .credential_config(&access.credential_handle)
      .map_err(|_| GitError::Permanent("credential_unavailable", "Git credential handle is unavailable"))?;
    Ok(GitContext {
      adapter: self,
      credential_config,
      hooks: PathBuf::from(null_device()),
      cancellation,
    })
  }
}

struct GitContext<'a> {
  adapter: &'a GitVcsAdapter,
  credential_config: Option<PathBuf>,
  hooks: PathBuf,
  cancellation: &'a CancellationToken,
}

impl GitContext<'_> {
  async fn run<I>(
    &self,
    directory: &Path,
    _step: &'static str,
    arguments: I,
    max_output_bytes: usize,
  ) -> Result<Vec<u8>, GitError>
  where
    I: IntoIterator<Item = OsString>,
  {
    if self.cancellation.is_cancelled() {
      return Err(GitError::Cancelled);
    }
    let protocol_file = if self.adapter.config.allow_file {
      "always"
    } else {
      "never"
    };
    let allowed_protocols = if self.adapter.config.allow_file {
      "file:https"
    } else {
      "https"
    };
    let result = Command::new(&self.adapter.config.git_path)
      .env_clear()
      .env("GIT_CONFIG_NOSYSTEM", "1")
      .env("GIT_TERMINAL_PROMPT", "0")
      .env("GIT_LFS_SKIP_SMUDGE", "1")
      .env("GIT_PROTOCOL_FROM_USER", "0")
      .env("GIT_ALLOW_PROTOCOL", allowed_protocols)
      .env("LC_ALL", "C")
      .env(
        "GIT_CONFIG_GLOBAL",
        self
          .credential_config
          .as_deref()
          .unwrap_or_else(|| Path::new(null_device())),
      )
      .arg("--no-optional-locks")
      .arg("-c")
      .arg(format!("core.hooksPath={}", self.hooks.display()))
      .arg("-c")
      .arg(format!("protocol.file.allow={protocol_file}"))
      .arg("-c")
      .arg("filter.lfs.required=false")
      .arg("-c")
      .arg("filter.lfs.smudge=")
      .arg("-c")
      .arg("maintenance.auto=false")
      .arg("-c")
      .arg("gc.auto=0")
      .arg("-c")
      .arg("fetch.writeCommitGraph=false")
      .args(arguments)
      .current_dir(directory)
      .cancel_on(self.cancellation.clone())
      .output_buffer(
        OutputBufferPolicy::unbounded()
          .with_max_bytes(max_output_bytes)
          .with_overflow(OverflowMode::Error),
      )
      .output_bytes()
      .await
      .map_err(|source| match source.reason() {
        ErrorReason::Cancelled { .. } => GitError::Cancelled,
        ErrorReason::OutputTooLarge { .. } => {
          GitError::Permanent("git_output_limit", "Git output exceeds the configured limit")
        }
        _ => GitError::Permanent("git_process_failed", "configured Git executable could not be run"),
      })?;
    if !result.is_success() {
      let diagnostic = String::from_utf8_lossy(result.stderr().as_bytes()).to_ascii_lowercase();
      if diagnostic.contains("authentication failed")
        || diagnostic.contains("not found")
        || diagnostic.contains("does not appear to be a git repository")
        || diagnostic.contains("repository not found")
      {
        return Err(GitError::Permanent(
          "repository_unavailable",
          "Git repository or credentials are invalid",
        ));
      }
      return Err(GitError::Transient(
        "git_command_failed",
        "Git provider operation failed",
      ));
    }
    Ok(result.into_stdout())
  }
}

fn validate_remote(remote: &str, allow_file: bool) -> Result<(), GitError> {
  if Path::new(remote).is_absolute() {
    return if allow_file {
      Ok(())
    } else {
      Err(GitError::Invalid(
        "local_transport_disabled",
        "local Git transports are disabled",
      ))
    };
  }
  let uri: Uri = remote.parse().map_err(|_| {
    GitError::Invalid(
      "invalid_repository_locator",
      "repository locator must be an absolute HTTPS URL",
    )
  })?;
  if uri.scheme_str() != Some("https")
    || uri.host().is_none()
    || uri
      .authority()
      .is_some_and(|authority| authority.as_str().contains('@'))
    || uri.query().is_some()
  {
    return Err(GitError::Invalid(
      "invalid_repository_locator",
      "repository locator must be credential-free HTTPS",
    ));
  }
  Ok(())
}

fn parse_references(bytes: &[u8]) -> Result<Vec<Reference>, GitError> {
  let values = parse_remote_lines(bytes)?;
  let mut references = BTreeMap::<String, Reference>::new();
  for (name, revision) in &values {
    if let Some(base) = name.strip_suffix("^{}") {
      if let Some(entry) = references.get_mut(base) {
        entry.revision.clone_from(revision);
      }
      continue;
    }
    let kind = if name.starts_with("refs/heads/") {
      ReferenceKind::Branch
    } else if name.starts_with("refs/tags/") {
      ReferenceKind::Tag
    } else {
      continue;
    };
    if !is_object_id(revision) || invalid_protocol_text(name) {
      return Err(GitError::Permanent(
        "invalid_reference",
        "Git returned an unsupported reference name or object identifier",
      ));
    }
    references.insert(
      name.clone(),
      Reference {
        name: name.clone(),
        kind,
        revision: revision.clone(),
      },
    );
  }
  for (name, revision) in values {
    if let Some(base) = name.strip_suffix("^{}")
      && let Some(entry) = references.get_mut(base)
    {
      entry.revision = revision;
    }
  }
  Ok(references.into_values().collect())
}

fn parse_remote_lines(bytes: &[u8]) -> Result<HashMap<String, String>, GitError> {
  let text = std::str::from_utf8(bytes)
    .map_err(|_| GitError::Permanent("invalid_git_output", "Git returned non-UTF-8 reference data"))?;
  let mut values = HashMap::new();
  for line in text.lines() {
    let (revision, name) = line.split_once('\t').ok_or(GitError::Permanent(
      "invalid_git_output",
      "Git returned malformed reference data",
    ))?;
    values.insert(name.to_owned(), revision.to_owned());
  }
  Ok(values)
}

fn parse_commit(revision: &str, bytes: &[u8]) -> Result<Commit, GitError> {
  let text = std::str::from_utf8(bytes)
    .map_err(|_| GitError::Permanent("invalid_commit_encoding", "Git commit metadata is not UTF-8"))?;
  let (headers, message) = text.split_once("\n\n").unwrap_or((text, ""));
  let mut parents = Vec::new();
  let mut author = None;
  let mut committed_at_unix_ms = None;
  for line in headers.lines() {
    if let Some(parent) = line.strip_prefix("parent ") {
      if !is_object_id(parent) {
        return Err(GitError::Permanent(
          "invalid_commit",
          "Git commit contains an invalid parent",
        ));
      }
      parents.push(parent.to_owned());
    } else if let Some(value) = line.strip_prefix("author ") {
      author = Some(normalize_commit_text(strip_git_timestamp(value))?);
    } else if let Some(value) = line.strip_prefix("committer ") {
      committed_at_unix_ms = git_timestamp(value).and_then(|seconds| seconds.checked_mul(1_000));
    }
  }
  if parents.len() > usize::from(octacity_vcs_protocol::MAX_PAGE_ENTRIES) {
    return Err(GitError::Permanent(
      "too_many_parents",
      "Git commit has too many parents",
    ));
  }
  Ok(Commit {
    revision: revision.to_owned(),
    parents,
    message: normalize_commit_text(message.trim_end_matches('\n'))?,
    author_display: author,
    committed_at_unix_ms,
    metadata: BTreeMap::new(),
  })
}

fn parse_tree(bytes: &[u8], parent: Option<&str>) -> Result<Vec<TreeEntry>, GitError> {
  let mut entries = Vec::new();
  for record in bytes.split(|byte| *byte == 0).filter(|record| !record.is_empty()) {
    let tab = record
      .iter()
      .position(|byte| *byte == b'\t')
      .ok_or(GitError::Permanent("invalid_tree", "Git returned malformed tree data"))?;
    let header = std::str::from_utf8(&record[..tab])
      .map_err(|_| GitError::Permanent("invalid_tree", "Git returned malformed tree metadata"))?;
    let name = std::str::from_utf8(&record[tab + 1..])
      .map_err(|_| GitError::Permanent("unsupported_path_encoding", "Git path is not UTF-8"))?;
    let fields = header.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 4 {
      return Err(GitError::Permanent(
        "invalid_tree",
        "Git returned malformed tree metadata",
      ));
    }
    let path = parent.map_or_else(|| name.to_owned(), |parent| format!("{parent}/{name}"));
    if invalid_repository_path(&path) {
      return Err(GitError::Permanent(
        "unsupported_path",
        "Git tree contains a path unsupported by the provider-neutral protocol",
      ));
    }
    let kind = match (fields[0], fields[1]) {
      ("120000", "blob") => TreeEntryKind::Symlink,
      (_, "blob") => TreeEntryKind::File,
      (_, "tree") => TreeEntryKind::Directory,
      _ => TreeEntryKind::Other,
    };
    let size_bytes = fields[3].parse::<u64>().ok();
    entries.push(TreeEntry { path, kind, size_bytes });
  }
  entries.sort_by(|left, right| left.path.cmp(&right.path));
  Ok(entries)
}

fn page_references(mut entries: Vec<Reference>, page_size: u16) -> ReferencePage {
  let page_size = usize::from(page_size);
  let next_cursor = (entries.len() > page_size).then(|| entries[page_size - 1].name.clone());
  entries.truncate(page_size);
  ReferencePage { entries, next_cursor }
}

fn page_tree(mut entries: Vec<TreeEntry>, page_size: u16) -> TreePage {
  let page_size = usize::from(page_size);
  let next_cursor = (entries.len() > page_size).then(|| entries[page_size - 1].path.clone());
  entries.truncate(page_size);
  TreePage { entries, next_cursor }
}

fn git_dir_arguments<const N: usize>(path: &Path, arguments: [&str; N]) -> Vec<OsString> {
  let mut values = Vec::with_capacity(N + 1);
  values.push(OsString::from(format!("--git-dir={}", path.display())));
  values.extend(arguments.into_iter().map(OsString::from));
  values
}

fn enforce_directory_limit(root: &Path, limit: u64) -> Result<(), GitError> {
  let mut total = 0_u64;
  let mut pending = vec![root.to_owned()];
  while let Some(directory) = pending.pop() {
    let entries = fs::read_dir(&directory)
      .map_err(|_| GitError::Transient("temporary_storage", "temporary repository cannot be inspected"))?;
    for entry in entries {
      let entry =
        entry.map_err(|_| GitError::Transient("temporary_storage", "temporary repository cannot be inspected"))?;
      let metadata = fs::symlink_metadata(entry.path())
        .map_err(|_| GitError::Transient("temporary_storage", "temporary repository cannot be inspected"))?;
      if metadata.file_type().is_dir() {
        pending.push(entry.path());
      } else {
        total = total.saturating_add(metadata.len());
        if total > limit {
          return Err(GitError::Permanent(
            "repository_too_large",
            "bare Git object database exceeds the configured limit",
          ));
        }
      }
    }
  }
  Ok(())
}

fn require_object_id(value: &str) -> Result<(), GitError> {
  if is_object_id(value) {
    Ok(())
  } else {
    Err(GitError::Invalid(
      "invalid_revision",
      "revision must be a full lowercase SHA-1 or SHA-256 commit identifier",
    ))
  }
}

fn is_object_id(value: &str) -> bool {
  matches!(value.len(), 40 | 64)
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid_protocol_text(value: &str) -> bool {
  value.is_empty() || value.len() > octacity_vcs_protocol::MAX_FIELD_BYTES || value.chars().any(char::is_control)
}

fn invalid_repository_path(path: &str) -> bool {
  invalid_protocol_text(path)
    || path.starts_with('/')
    || path.contains('\\')
    || path
      .split('/')
      .any(|component| component.is_empty() || component == "." || component == "..")
}

fn normalize_commit_text(value: &str) -> Result<String, GitError> {
  let normalized = value
    .chars()
    .map(|character| if character.is_control() { ' ' } else { character })
    .collect::<String>();
  let normalized = normalized.trim().to_owned();
  let normalized = if normalized.is_empty() {
    "<empty>".to_owned()
  } else {
    normalized
  };
  if normalized.len() > MAX_TEXT_BYTES {
    return Err(GitError::Permanent(
      "commit_metadata_too_large",
      "Git commit metadata exceeds the protocol limit",
    ));
  }
  Ok(normalized)
}

fn strip_git_timestamp(value: &str) -> &str {
  value.rsplitn(3, ' ').nth(2).unwrap_or(value)
}

fn git_timestamp(value: &str) -> Option<u64> {
  let (without_timezone, _) = value.rsplit_once(' ')?;
  let (_, timestamp) = without_timezone.rsplit_once(' ')?;
  timestamp.parse().ok()
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
  let start = bytes
    .iter()
    .position(|byte| !byte.is_ascii_whitespace())
    .unwrap_or(bytes.len());
  let end = bytes
    .iter()
    .rposition(|byte| !byte.is_ascii_whitespace())
    .map_or(start, |index| index + 1);
  &bytes[start..end]
}

impl GitError {
  fn into_failure(self) -> Failure {
    let (class, code, diagnostic, retry_after_ms) = match self {
      Self::Invalid(code, diagnostic) => (FailureClass::InvalidRequest, code, diagnostic, None),
      Self::Permanent(code, diagnostic) => (FailureClass::Permanent, code, diagnostic, None),
      Self::Transient(code, diagnostic) => (FailureClass::Transient, code, diagnostic, Some(RETRY_AFTER_MS)),
      Self::Cancelled => (
        FailureClass::Cancelled,
        "cancelled",
        "Git operation was cancelled",
        None,
      ),
    };
    Failure {
      class,
      code: code.to_owned(),
      diagnostic: diagnostic.to_owned(),
      retry_after_ms,
    }
  }
}
