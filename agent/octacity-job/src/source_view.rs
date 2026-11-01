//! Private, bounded projection of selected paths/lines before any workload starts.
//! The exact checkout remains available only to trusted reconstruction/capture.
use crate::{JobError, create_private_directory, filesystem};
use octacity_protocol::{FactorySourceLinesV1, FactorySourceScopeV1, FactorySourceSelectionV1};
use std::{
  fs,
  path::{Path, PathBuf},
};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub(super) fn prepare(
  source: &Path,
  view: &Path,
  scope: &FactorySourceScopeV1,
  maximum: u64,
  deadline: Instant,
  cancellation: &CancellationToken,
) -> Result<PathBuf, JobError> {
  let reader = octacity_private_fs::RootedReadDirectory::open(source)
    .map_err(|error| filesystem("open selected source", source, error))?;
  create_private_directory(view)?;
  let mut remaining = maximum;
  // Keep one directory iterator per depth, rather than accumulating an entire
  // untrusted checkout's filenames before enforcing the deadline or copying.
  enum Visit {
    Entry(PathBuf),
    Directory(PathBuf, Box<fs::ReadDir>),
  }
  for (index, selected) in scope.selections.iter().enumerate() {
    let mut pending = vec![Visit::Entry(PathBuf::from(&selected.path))];
    while let Some(visit) = pending.pop() {
      check(deadline, cancellation)?;
      let relative = match visit {
        Visit::Entry(relative) => relative,
        Visit::Directory(parent, mut entries) => {
          if let Some(entry) = entries.next() {
            let entry = entry.map_err(|error| filesystem("list selected source", &source.join(&parent), error))?;
            let relative = parent.join(entry.file_name());
            pending.push(Visit::Directory(parent, entries));
            pending.push(Visit::Entry(relative));
          }
          continue;
        }
      };
      if scope.selections[..index].iter().any(|prior| covers(prior, &relative)) {
        continue;
      }
      let path = source.join(&relative);
      // This checkout is private and has no untrusted writer before spawn.
      // Reject linked ancestors before even enumerating an empty directory;
      // regular-file reads additionally use retained no-follow handles.
      if path
        .canonicalize()
        .map_err(|error| filesystem("resolve selected source", &path, error))?
        != path
      {
        return Err(JobError::Invalid("source selection contains a symlink".to_owned()));
      }
      let metadata =
        fs::symlink_metadata(&path).map_err(|error| filesystem("inspect selected source", &path, error))?;
      if metadata.file_type().is_symlink() {
        return Err(JobError::Invalid("source selection contains a symlink".to_owned()));
      }
      if metadata.is_dir() && selected.lines.is_none() {
        create_view_directory(view, &relative)?;
        let entries = fs::read_dir(&path).map_err(|error| filesystem("list selected source", &path, error))?;
        pending.push(Visit::Directory(relative, Box::new(entries)));
        continue;
      } else if metadata.is_file() {
        let mut contents = reader
          .read_bounded(&relative, remaining)
          .map_err(|error| filesystem("read selected source", &path, error))?;
        if !scope
          .selections
          .iter()
          .any(|selection| selection.lines.is_none() && covers(selection, &relative))
        {
          let ranges: Vec<_> = scope
            .selections
            .iter()
            .filter(|selection| Path::new(&selection.path) == relative)
            .filter_map(|selection| selection.lines)
            .collect();
          project_lines(&mut contents, &ranges);
        }
        remaining = remaining
          .checked_sub(contents.len() as u64)
          .ok_or_else(|| JobError::Invalid("selected source exceeds its disk bound".to_owned()))?;
        let destination = view.join(&relative);
        if let Some(parent) = relative.parent() {
          create_view_directory(view, parent)?;
        }
        fs::write(&destination, contents)
          .map_err(|error| filesystem("publish selected source", &destination, error))?;
        #[cfg(unix)]
        {
          use std::os::unix::fs::PermissionsExt;
          // Retain repository execution semantics within the private Job;
          // never copy special bits or grant access to other host users.
          let mode = 0o600 | (metadata.permissions().mode() & 0o100);
          fs::set_permissions(&destination, fs::Permissions::from_mode(mode))
            .map_err(|error| filesystem("set selected source permissions", &destination, error))?;
        }
      } else {
        return Err(JobError::Invalid(
          "source selection requires a regular file or directory".to_owned(),
        ));
      }
    }
  }
  Ok(view.to_owned())
}

fn create_view_directory(view: &Path, relative: &Path) -> Result<(), JobError> {
  let mut path = view.to_owned();
  for component in relative.components() {
    path.push(component);
    if !path.exists() {
      create_private_directory(&path)?;
    }
  }
  Ok(())
}

fn covers(selection: &FactorySourceSelectionV1, relative: &Path) -> bool {
  let path = Path::new(&selection.path);
  path == relative || selection.lines.is_none() && relative.starts_with(path)
}

fn project_lines(bytes: &mut Vec<u8>, ranges: &[FactorySourceLinesV1]) {
  let last = ranges.iter().map(|range| range.last).max().unwrap();
  let mut read = 0;
  let mut written = 0;
  for number in 1..=u64::from(last) {
    if read == bytes.len() {
      break;
    }
    let end = bytes[read..]
      .iter()
      .position(|byte| *byte == b'\n')
      .map_or(bytes.len(), |index| read + index + 1);
    if ranges
      .iter()
      .any(|range| (u64::from(range.first)..=u64::from(range.last)).contains(&number))
    {
      bytes.copy_within(read..end, written);
      written += end - read;
    } else {
      bytes[written] = b'\n';
      written += 1;
    }
    read = end;
  }
  bytes.truncate(written);
}
fn check(deadline: Instant, cancellation: &CancellationToken) -> Result<(), JobError> {
  if cancellation.is_cancelled() {
    Err(JobError::Cancelled)
  } else if Instant::now() >= deadline {
    Err(JobError::TimedOut)
  } else {
    Ok(())
  }
}
