//! Actual private transport and source-mount isolation; not ToolRisk qualification.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;

use octacity_execution::FactoryToolChannel;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

#[path = "support/guest_probe.rs"]
mod guest_probe;

#[tokio::test]
#[ignore = "requires pinned Microsandbox/firmware/image and a compiled LinuxARM vsock probe"]
async fn private_channel_reads_an_unmounted_source_without_exposing_it_to_the_guest() {
  let fixture = guest_probe::GuestProbe::new("OCTACITY_CONTRACT_VSOCK_PROBE");
  let control_directory = tempfile::Builder::new()
    .prefix("control-")
    .tempdir_in(fixture.work_root())
    .unwrap();
  let control = control_directory.path();
  std::fs::set_permissions(control, std::fs::Permissions::from_mode(0o700)).unwrap();
  let socket = control.join("tool.sock");
  let listener = tokio::net::UnixListener::bind(&socket).unwrap();
  std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
  let channel = FactoryToolChannel::new(socket, 5001).unwrap();
  let physical_file = fixture.source_file();
  let server = tokio::spawn(async move {
    let (mut peer, _) = listener.accept().await.unwrap();
    let mut request = [0; 4];
    peer.read_exact(&mut request).await.unwrap();
    assert_eq!(&request, b"PING");
    let bytes = octacity_private_fs::read_bounded_regular_file(&physical_file, 64).unwrap();
    peer.write_all(&bytes).await.unwrap();
    // Keep the host endpoint alive until the guest closes its side. Immediate
    // close can discard queued bytes in the pinned vsock transport.
    let mut end = [0; 1];
    let _ = peer.read(&mut end).await;
  });
  let (exit, bytes) = fixture.run(Some(channel), 8).await;
  server.abort();
  let _ = server.await;
  assert_eq!(
    exit,
    Some(0),
    "the probe refuses direct source access before using the controlled channel"
  );
  assert_eq!(bytes, b"backend-only-source");
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox/firmware/image and a compiled LinuxARM process-limit probe"]
async fn factory_process_limit_bounds_the_live_runner_descendants() {
  let fixture = guest_probe::GuestProbe::new("OCTACITY_CONTRACT_PROCESS_LIMIT_PROBE");
  let limit = 8;
  let (exit, bytes) = fixture.run(None, limit).await;
  assert_eq!(exit, Some(0), "the bounded probe must finish and reap its children");
  let values = std::str::from_utf8(&bytes)
    .unwrap()
    .split_whitespace()
    .map(|value| value.parse::<u64>().unwrap())
    .collect::<Vec<_>>();
  assert_eq!(
    values.len(),
    4,
    "probe reports UID, live child count, no_new_privs and effective capabilities"
  );
  assert_ne!(
    values[0], 0,
    "Factory runner privileges must not bypass the process ceiling"
  );
  assert!(
    values[1] < u64::from(limit),
    "real UID {} created {} simultaneous children plus its runner despite process_limit {limit}",
    values[0],
    values[1]
  );
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox/firmware/image and a compiled LinuxARM process-limit probe"]
async fn factory_runner_cannot_regain_privileges_through_guest_setuid_executables() {
  let fixture = guest_probe::GuestProbe::new("OCTACITY_CONTRACT_PROCESS_LIMIT_PROBE");
  let (exit, bytes) = fixture.run(None, 8).await;
  assert_eq!(exit, Some(0));
  let values = std::str::from_utf8(&bytes)
    .unwrap()
    .split_whitespace()
    .map(|value| value.parse::<u64>().unwrap())
    .collect::<Vec<_>>();
  assert_eq!(values.len(), 4);
  assert_ne!(values[0], 0);
  assert_eq!(values[3], 0, "Factory runner must have no effective capabilities");
  assert_eq!(
    values[2], 1,
    "no_new_privs must prohibit setuid/file-capability elevation after exec"
  );
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and the actual LinuxARM reader worker"]
async fn standalone_reader_uses_its_actual_entrypoint_without_an_octa_distribution() {
  let fixture = guest_probe::GuestProbe::new("OCTACITY_CONTRACT_PROTECTED_READER");
  let (exit, response) = fixture
    .run_read_worker(br#"{"schema_version":1,"relative_path":"input","max_bytes":32}"#)
    .await;
  assert_eq!(exit, Some(0));
  assert_eq!(response.first(), Some(&0));
  assert_eq!(u32::from_be_bytes(response[1..5].try_into().unwrap()), 19);
  assert_eq!(&response[5..], b"backend-only-source");
}

#[tokio::test]
#[ignore = "requires pinned Microsandbox environment and the actual LinuxARM reader worker"]
async fn owned_writable_disk_excludes_readonly_inputs_and_other_execution_roots() {
  let fixture = guest_probe::GuestProbe::new("OCTACITY_CONTRACT_PROTECTED_READER");
  let (exit, response, usage) = fixture
    .run_with_owned_disk(br#"{"schema_version":1,"relative_path":"input","max_bytes":32}"#)
    .await;
  assert_eq!(exit, Some(0));
  assert_eq!(&response[5..], b"backend-only-source");
  assert_eq!(usage.disk_current_bytes, 4 * 1024 * 1024);
  assert_eq!(usage.disk_peak_bytes, 4 * 1024 * 1024);
}
