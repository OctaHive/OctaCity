//! Public installation boundary for pinned non-Octa worker programs.

use std::{fs, path::Path};

use octacity_private_fs::test_support::PrivateDirectoryFixture;
use octacity_runner::verify_standalone_program;

fn write_arm64_worker(path: &Path) {
  let mut bytes = vec![0_u8; 64];
  bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
  bytes[18..20].copy_from_slice(&183_u16.to_le_bytes());
  bytes.extend_from_slice(b"bounded-reader-v1");
  fs::write(path, bytes).unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o500)).unwrap();
  }
}

#[test]
fn pins_a_standalone_guest_entrypoint_without_octa_metadata() {
  let release = PrivateDirectoryFixture::new().unwrap();
  let worker = release.path().join("bounded-reader");
  write_arm64_worker(&worker);
  let program = verify_standalone_program(
    release.path(),
    Path::new("bounded-reader"),
    "f3980a38ac8d4677226f7763ef1081af81916b942e0d4643622029dcd33df0e4",
    "linux-aarch64",
  )
  .unwrap();

  assert_eq!(program.release_root, release.path().canonicalize().unwrap());
  assert_eq!(program.executable, worker.canonicalize().unwrap());
  assert!(program.plugin_projection.is_none());
  assert!(program.external_executables.is_empty());
  assert!(
    verify_standalone_program(
      release.path(),
      Path::new("bounded-reader"),
      &"0".repeat(64),
      "linux-aarch64"
    )
    .is_err()
  );
  assert!(
    verify_standalone_program(
      release.path(),
      Path::new("bounded-reader"),
      "f3980a38ac8d4677226f7763ef1081af81916b942e0d4643622029dcd33df0e4",
      "linux-x86_64",
    )
    .is_err()
  );
}

#[cfg(unix)]
#[test]
fn rejects_a_worker_below_an_untrusted_writable_directory() {
  use std::os::unix::fs::PermissionsExt as _;

  let release = PrivateDirectoryFixture::new().unwrap();
  let nested = release.path().join("replaceable");
  fs::create_dir(&nested).unwrap();
  write_arm64_worker(&nested.join("bounded-reader"));
  fs::set_permissions(&nested, fs::Permissions::from_mode(0o777)).unwrap();

  assert!(
    verify_standalone_program(
      release.path(),
      Path::new("replaceable/bounded-reader"),
      "f3980a38ac8d4677226f7763ef1081af81916b942e0d4643622029dcd33df0e4",
      "linux-aarch64",
    )
    .is_err()
  );
}

#[test]
fn refuses_entrypoints_outside_the_installed_distribution() {
  let release = PrivateDirectoryFixture::new().unwrap();
  write_arm64_worker(&release.path().join("bounded-reader"));
  for entrypoint in [Path::new("../bounded-reader"), Path::new("/bounded-reader")] {
    assert!(
      verify_standalone_program(
        release.path(),
        entrypoint,
        "f3980a38ac8d4677226f7763ef1081af81916b942e0d4643622029dcd33df0e4",
        "linux-aarch64",
      )
      .is_err()
    );
  }
}
