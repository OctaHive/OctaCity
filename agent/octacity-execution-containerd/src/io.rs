//! Owns private FIFO creation and conversion to execution streams.

use super::*;

pub(super) struct ContainerIo {
  pub(super) stdin_path: PathBuf,
  pub(super) stdout_path: PathBuf,
  pub(super) stderr_path: PathBuf,
  pub(super) stdin: File,
  pub(super) stdout: File,
  pub(super) stderr: File,
}

impl ContainerIo {
  pub(super) fn create(directory: &Path) -> Result<Self, ExecutionError> {
    fs::create_dir(directory).map_err(ExecutionError::Io)?;
    set_private_permissions(directory)?;
    let stdin_path = directory.join("stdin");
    let stdout_path = directory.join("stdout");
    let stderr_path = directory.join("stderr");
    create_fifo(&stdin_path)?;
    create_fifo(&stdout_path)?;
    create_fifo(&stderr_path)?;
    let stdin = OpenOptions::new()
      .read(true)
      .write(true)
      .custom_flags(libc::O_NONBLOCK)
      .open(&stdin_path)
      .map_err(ExecutionError::Io)?;
    let stdout = OpenOptions::new()
      .read(true)
      .custom_flags(libc::O_NONBLOCK)
      .open(&stdout_path)
      .map_err(ExecutionError::Io)?;
    let stderr = OpenOptions::new()
      .read(true)
      .custom_flags(libc::O_NONBLOCK)
      .open(&stderr_path)
      .map_err(ExecutionError::Io)?;
    Ok(Self {
      stdin_path,
      stdout_path,
      stderr_path,
      stdin,
      stdout,
      stderr,
    })
  }

  pub(super) fn into_execution_io(self) -> Result<ExecutionIo, ExecutionError> {
    // Opening a FIFO without O_NONBLOCK can deadlock startup while containerd
    // is opening the opposite endpoints. Once the task is running all peers
    // exist, and blocking descriptors give AsyncRead/AsyncWrite normal pipe
    // semantics instead of exposing transient EAGAIN as a protocol failure.
    set_blocking(&self.stdin)?;
    set_blocking(&self.stdout)?;
    set_blocking(&self.stderr)?;
    Ok(ExecutionIo {
      stdin: Box::pin(tokio::fs::File::from_std(self.stdin)) as ExecutionWriter,
      stdout: Box::pin(tokio::fs::File::from_std(self.stdout)) as ExecutionReader,
      stderr: Box::pin(tokio::fs::File::from_std(self.stderr)) as ExecutionReader,
    })
  }
}

fn set_blocking(file: &File) -> Result<(), ExecutionError> {
  let descriptor = file.as_raw_fd();
  // SAFETY: F_GETFL and F_SETFL borrow a valid descriptor owned by `file`.
  let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
  if flags < 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  if flags & libc::O_NONBLOCK != 0 && unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags & !libc::O_NONBLOCK) } < 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  Ok(())
}
