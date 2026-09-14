//! Creates bounded immutable file and deterministic directory snapshots.

use std::{
  fs::{self, File},
  io::{Read, Write},
  path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

use crate::OutputError;

pub(super) fn snapshot_file(
  source: &Path,
  destination: &Path,
  maximum_bytes: u64,
  cancellation: &CancellationToken,
) -> Result<(u64, String), OutputError> {
  let mut input = octacity_private_fs::open_regular_file_no_follow(source)
    .map_err(|error| io("open output without following links", source, error))?;
  let before = FileStamp::read(&input, source)?;
  if before.len > maximum_bytes {
    return Err(byte_limit_error(maximum_bytes));
  }
  let mut output = SnapshotWriter::new(create_snapshot(destination)?, maximum_bytes);
  copy_cancelled(
    &mut input,
    &mut output,
    maximum_bytes,
    cancellation,
    source,
    destination,
  )?;
  output
    .inner
    .sync_all()
    .map_err(|error| io("sync snapshot", destination, error))?;
  ensure_unchanged(before, &input, source, "staged")?;
  Ok(output.finish())
}

pub(super) fn snapshot_directory(
  source: &Path,
  canonical_source: &Path,
  destination: &Path,
  max_entries: usize,
  maximum_bytes: u64,
  cancellation: &CancellationToken,
) -> Result<(u64, String), OutputError> {
  let output = create_snapshot(destination)?;
  let mut archive = tar::Builder::new(SnapshotWriter::new(output, maximum_bytes));
  archive.mode(tar::HeaderMode::Deterministic);
  let mut discovered = 0_usize;
  let mut pending = discover_children(source, Path::new(""), max_entries, &mut discovered, cancellation)?;
  // Children are pushed in reverse so the stack emits a depth-first lexical
  // order. The order is part of the archive format and therefore its digest.
  pending.reverse();
  while let Some(child) = pending.pop() {
    check_cancel(cancellation)?;
    let child_relative = child.archive_path;
    let child_path = child.source_path;
    #[cfg(windows)]
    reject_windows_reparse_point(&child_path)?;
    let metadata =
      fs::symlink_metadata(&child_path).map_err(|error| io("inspect archive entry", &child_path, error))?;
    if metadata.file_type().is_symlink() {
      append_symlink(
        &mut archive,
        canonical_source,
        &child_path,
        &child_relative,
        cancellation,
      )?;
    } else if metadata.is_dir() {
      append_directory(&mut archive, &child_relative)?;
      let mut children = discover_children(&child_path, &child_relative, max_entries, &mut discovered, cancellation)?;
      children.reverse();
      pending.extend(children);
    } else if metadata.is_file() {
      append_file(&mut archive, &child_path, &child_relative, cancellation)?;
    } else {
      return Err(OutputError::Invalid(format!(
        "output contains unsupported entry '{}'",
        child_path.display()
      )));
    }
  }
  if let Err(error) = archive.finish() {
    return Err(archive_error(&archive, "finish directory archive", destination, error));
  }
  let output = archive
    .into_inner()
    .map_err(|error| io("close directory archive", destination, error))?;
  output
    .inner
    .sync_all()
    .map_err(|error| io("sync directory archive", destination, error))?;
  Ok(output.finish())
}

#[cfg(windows)]
fn reject_windows_reparse_point(path: &Path) -> Result<(), OutputError> {
  if octacity_private_fs::is_reparse_point(path).map_err(|error| io("inspect archive reparse point", path, error))? {
    Err(OutputError::Invalid(format!(
      "output contains Windows reparse point '{}'",
      path.display()
    )))
  } else {
    Ok(())
  }
}

/// Discovers one directory without allowing an untrusted listing to exceed
/// the job-wide entry budget before it is sorted in memory.
fn discover_children(
  directory: &Path,
  relative: &Path,
  max_entries: usize,
  discovered: &mut usize,
  cancellation: &CancellationToken,
) -> Result<Vec<DiscoveredEntry>, OutputError> {
  let iterator = fs::read_dir(directory).map_err(|error| io("read directory", directory, error))?;
  let mut children = Vec::new();
  for child in iterator {
    check_cancel(cancellation)?;
    *discovered = discovered
      .checked_add(1)
      .ok_or_else(|| OutputError::Invalid("archive entry count overflowed".to_owned()))?;
    if *discovered > max_entries {
      return Err(OutputError::Invalid(format!(
        "directory output exceeds {max_entries} entries"
      )));
    }
    let child = child.map_err(|error| io("read directory entry", directory, error))?;
    let name = child.file_name();
    let name = name
      .to_str()
      .ok_or_else(|| OutputError::Invalid("output contains a non-UTF-8 path".to_owned()))?;
    validate_portable_component(name)?;
    children.push(DiscoveredEntry {
      archive_path: relative.join(name),
      source_path: child.path(),
      // Sorting the UTF-8 bytes, rather than `OsStr`, makes one archive digest
      // portable between Windows and Unix hosts.
      name: name.to_owned(),
    });
  }
  children.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
  Ok(children)
}

struct DiscoveredEntry {
  archive_path: PathBuf,
  source_path: PathBuf,
  name: String,
}

fn append_directory(archive: &mut tar::Builder<SnapshotWriter>, path: &Path) -> Result<(), OutputError> {
  let mut header = tar::Header::new_ustar();
  header.set_entry_type(tar::EntryType::Directory);
  set_header(&mut header, path, 0, 0o755)?;
  archive
    .append(&header, std::io::empty())
    .map_err(|error| archive_error(archive, "append archive directory", path, error))
}

fn append_file(
  archive: &mut tar::Builder<SnapshotWriter>,
  source: &Path,
  path: &Path,
  cancellation: &CancellationToken,
) -> Result<(), OutputError> {
  let file = octacity_private_fs::open_regular_file_no_follow(source)
    .map_err(|error| io("open archive file without following links", source, error))?;
  let before = FileStamp::read(&file, source)?;
  let mut reader = CancellationReader {
    inner: file,
    cancellation,
  };
  let executable = before.mode & 0o111 != 0;
  let mut header = tar::Header::new_ustar();
  header.set_entry_type(tar::EntryType::Regular);
  set_header(&mut header, path, before.len, if executable { 0o755 } else { 0o644 })?;
  if let Err(error) = archive.append(&header, &mut reader) {
    if cancellation.is_cancelled() {
      return Err(OutputError::Cancelled);
    }
    return Err(archive_error(archive, "append archive file", source, error));
  }
  ensure_unchanged(before, &reader.inner, source, "archived")?;
  Ok(())
}

fn append_symlink(
  archive: &mut tar::Builder<SnapshotWriter>,
  root: &Path,
  source: &Path,
  path: &Path,
  cancellation: &CancellationToken,
) -> Result<(), OutputError> {
  check_cancel(cancellation)?;
  let target = fs::read_link(source).map_err(|error| io("read symbolic link", source, error))?;
  if target.is_absolute() {
    return Err(OutputError::Invalid(format!(
      "output symlink '{}' has an absolute target",
      source.display()
    )));
  }
  let resolved = source
    .parent()
    .unwrap_or(root)
    .join(&target)
    .canonicalize()
    .map_err(|error| io("resolve symbolic link", source, error))?;
  if !resolved.starts_with(root) {
    return Err(OutputError::Invalid(format!(
      "output symlink '{}' escapes its artifact root",
      source.display()
    )));
  }
  let portable_target = portable_link_target(&target)?;
  let mut header = tar::Header::new_ustar();
  header.set_entry_type(tar::EntryType::Symlink);
  header
    .set_link_name(&portable_target)
    .map_err(|error| io("encode symbolic link", source, error))?;
  set_header(&mut header, path, 0, 0o777)?;
  archive
    .append(&header, std::io::empty())
    .map_err(|error| archive_error(archive, "append symbolic link", source, error))
}

fn portable_link_target(target: &Path) -> Result<String, OutputError> {
  let mut components = Vec::new();
  for component in target.components() {
    match component {
      std::path::Component::ParentDir => components.push(".."),
      std::path::Component::Normal(value) => {
        let value = value
          .to_str()
          .ok_or_else(|| OutputError::Invalid("output symlink target is not UTF-8".to_owned()))?;
        validate_portable_component(value)?;
        components.push(value);
      }
      std::path::Component::CurDir => {}
      std::path::Component::RootDir | std::path::Component::Prefix(_) => {
        return Err(OutputError::Invalid("output symlink target is not relative".to_owned()));
      }
    }
  }
  if components.is_empty() {
    return Err(OutputError::Invalid("output symlink target is empty".to_owned()));
  }
  Ok(components.join("/"))
}

fn validate_portable_component(value: &str) -> Result<(), OutputError> {
  if value.is_empty() || value.contains(['\\', ':', '\0']) || value.chars().any(char::is_control) {
    return Err(OutputError::Invalid(format!(
      "output path component '{value}' is not portable"
    )));
  }
  Ok(())
}

fn set_header(header: &mut tar::Header, path: &Path, size: u64, mode: u32) -> Result<(), OutputError> {
  header
    .set_path(path)
    .map_err(|error| io("encode archive path", path, error))?;
  header.set_size(size);
  header.set_mode(mode);
  header.set_uid(0);
  header.set_gid(0);
  header.set_mtime(0);
  header.set_cksum();
  Ok(())
}

fn create_snapshot(path: &Path) -> Result<File, OutputError> {
  let mut options = fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
  }
  options.open(path).map_err(|error| io("create snapshot", path, error))
}

struct CancellationReader<'a> {
  inner: File,
  cancellation: &'a CancellationToken,
}

impl Read for CancellationReader<'_> {
  fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
    if self.cancellation.is_cancelled() {
      return Err(std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        "output preparation cancelled",
      ));
    }
    self.inner.read(buffer)
  }
}

fn copy_cancelled(
  input: &mut impl Read,
  output: &mut impl Write,
  maximum_bytes: u64,
  cancellation: &CancellationToken,
  input_path: &Path,
  output_path: &Path,
) -> Result<u64, OutputError> {
  let mut buffer = [0_u8; 64 * 1024];
  let mut copied = 0_u64;
  loop {
    check_cancel(cancellation)?;
    let read = input
      .read(&mut buffer)
      .map_err(|error| io("read output", input_path, error))?;
    if read == 0 {
      return Ok(copied);
    }
    if read as u64 > maximum_bytes.saturating_sub(copied) {
      return Err(byte_limit_error(maximum_bytes));
    }
    output
      .write_all(&buffer[..read])
      .map_err(|error| io("write output", output_path, error))?;
    copied = copied
      .checked_add(read as u64)
      .ok_or_else(|| OutputError::Invalid("output size overflowed".to_owned()))?;
  }
}

/// Write-through guard that enforces the signed staging quota and computes the
/// identity of the exact bytes accepted by the staging file in one pass.
struct SnapshotWriter {
  inner: File,
  maximum: u64,
  remaining: u64,
  exceeded: bool,
  written: u64,
  hasher: Sha256,
}

impl SnapshotWriter {
  fn new(inner: File, maximum: u64) -> Self {
    Self {
      inner,
      maximum,
      remaining: maximum,
      exceeded: false,
      written: 0,
      hasher: Sha256::new(),
    }
  }

  fn finish(self) -> (u64, String) {
    (self.written, format!("{:x}", self.hasher.finalize()))
  }
}

impl Write for SnapshotWriter {
  fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
    if buffer.len() as u64 > self.remaining {
      self.exceeded = true;
      return Err(std::io::Error::other("signed output byte limit exceeded"));
    }
    let written = self.inner.write(buffer)?;
    self.remaining -= written as u64;
    self.written += written as u64;
    self.hasher.update(&buffer[..written]);
    Ok(written)
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.inner.flush()
  }
}

fn archive_error(
  archive: &tar::Builder<SnapshotWriter>,
  operation: &'static str,
  path: &Path,
  error: std::io::Error,
) -> OutputError {
  if archive.get_ref().exceeded {
    byte_limit_error(archive.get_ref().maximum)
  } else {
    io(operation, path, error)
  }
}

fn byte_limit_error(maximum_bytes: u64) -> OutputError {
  OutputError::Invalid(format!(
    "staged output exceeds its signed byte limit of {maximum_bytes} bytes"
  ))
}

pub(super) fn check_cancel(cancellation: &CancellationToken) -> Result<(), OutputError> {
  if cancellation.is_cancelled() {
    Err(OutputError::Cancelled)
  } else {
    Ok(())
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileStamp {
  len: u64,
  modified: Option<std::time::SystemTime>,
  mode: u32,
  #[cfg(unix)]
  device: u64,
  #[cfg(unix)]
  inode: u64,
  #[cfg(windows)]
  identity: octacity_private_fs::FileIdentity,
}

impl FileStamp {
  fn read(file: &File, path: &Path) -> Result<Self, OutputError> {
    let metadata = file
      .metadata()
      .map_err(|error| io("inspect file stability", path, error))?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt as _;
    Ok(Self {
      len: metadata.len(),
      modified: metadata.modified().ok(),
      #[cfg(unix)]
      mode: metadata.mode(),
      #[cfg(not(unix))]
      mode: 0,
      #[cfg(unix)]
      device: metadata.dev(),
      #[cfg(unix)]
      inode: metadata.ino(),
      #[cfg(windows)]
      identity: octacity_private_fs::file_identity(file)
        .map_err(|error| io("read Windows file identity", path, error))?,
    })
  }
}

/// Re-checks the identity and mutable metadata captured before a copy.
///
/// The helper is deliberately shared by standalone files and archive entries:
/// either path must fail closed if the producer changes an output while the
/// agent is freezing it for upload.
fn ensure_unchanged(before: FileStamp, opened: &File, path: &Path, operation: &str) -> Result<(), OutputError> {
  let current = octacity_private_fs::open_regular_file_no_follow(path)
    .map_err(|error| io("reopen output without following links", path, error))?;
  if before == FileStamp::read(opened, path)? && before == FileStamp::read(&current, path)? {
    Ok(())
  } else {
    Err(OutputError::Invalid(format!(
      "output '{}' changed while being {operation}",
      path.display()
    )))
  }
}

fn io(operation: &'static str, path: &Path, source: std::io::Error) -> OutputError {
  OutputError::Io {
    operation,
    path: path.to_owned(),
    source,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ustar_rejects_paths_that_require_an_extension_record() {
    let path = PathBuf::from(format!("{}/{}/file", "a".repeat(150), "b".repeat(150)));
    let mut header = tar::Header::new_ustar();

    assert!(matches!(
      set_header(&mut header, &path, 0, 0o644),
      Err(OutputError::Io { .. })
    ));
  }

  #[test]
  fn stability_check_rejects_a_changed_open_file() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("output");
    fs::write(&output, b"first").unwrap();
    let opened = octacity_private_fs::open_regular_file_no_follow(&output).unwrap();
    let before = FileStamp::read(&opened, &output).unwrap();
    fs::write(&output, b"other-longer").unwrap();

    assert!(matches!(
      ensure_unchanged(before, &opened, &output, "staged"),
      Err(OutputError::Invalid(message)) if message.contains("changed while being staged")
    ));
  }

  #[cfg(windows)]
  #[test]
  fn windows_file_identity_distinguishes_same_metadata_replacement() {
    use std::fs::FileTimes;

    let temporary = tempfile::tempdir().unwrap();
    let original = temporary.path().join("original");
    let replacement = temporary.path().join("replacement");
    fs::write(&original, b"same-size").unwrap();
    fs::write(&replacement, b"different").unwrap();
    let original_file = octacity_private_fs::open_regular_file_no_follow(&original).unwrap();
    let modified = original_file.metadata().unwrap().modified().unwrap();
    let replacement_file = octacity_private_fs::open_regular_file_no_follow(&replacement).unwrap();
    replacement_file
      .set_times(FileTimes::new().set_modified(modified))
      .unwrap();

    let original_stamp = FileStamp::read(&original_file, &original).unwrap();
    let replacement_stamp = FileStamp::read(&replacement_file, &replacement).unwrap();
    assert_eq!(original_stamp.len, replacement_stamp.len);
    assert_eq!(original_stamp.modified, replacement_stamp.modified);
    assert_ne!(original_stamp, replacement_stamp);
  }
}
