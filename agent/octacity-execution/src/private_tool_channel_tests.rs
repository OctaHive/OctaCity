#[cfg(unix)]
#[test]
fn a_private_tool_channel_requires_an_owned_socket_and_redacts_its_host_path() {
  use std::os::unix::{fs::PermissionsExt as _, net::UnixListener};

  let temporary = tempfile::tempdir().unwrap();
  let directory = temporary.path().canonicalize().unwrap();
  std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
  let socket = directory.join("private-tool.sock");
  let _listener = UnixListener::bind(&socket).unwrap();
  std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
  let channel = FactoryToolChannel::new(socket.clone(), 5001).unwrap();
  assert_eq!(channel.host_socket(), socket);
  assert_eq!(channel.guest_port(), 5001);
  assert!(!format!("{channel:?}").contains(directory.to_str().unwrap()));
  for port in [0, u32::MAX] {
    assert!(FactoryToolChannel::new(socket.clone(), port).is_err());
  }
  let alias = directory.join("alias.sock");
  std::os::unix::fs::symlink(&socket, &alias).unwrap();
  assert!(FactoryToolChannel::new(alias, 5001).is_err());
  std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o666)).unwrap();
  assert!(FactoryToolChannel::new(socket, 5001).is_err());
}

#[cfg(unix)]
#[test]
fn a_controlled_read_channel_requires_the_physical_source_to_remain_hidden() {
  use std::os::unix::{fs::PermissionsExt as _, net::UnixListener};
  let (_temporary, mut request, _) = factory_execution_fixture();
  let control = request.workspace_root.parent().unwrap().join("control");
  octacity_private_fs::create_private_directory(&control).unwrap();
  let socket = control.join("tool.sock");
  let _listener = UnixListener::bind(&socket).unwrap();
  std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
  let factory = request.factory.as_mut().unwrap();
  factory.source_visible = false;
  factory.source_read_only = true;
  factory.tool_channel = Some(FactoryToolChannel::new(socket, 5001).unwrap());
  let platform = ExecutionPlatform {
    os: ExecutionOs::Linux,
    architecture: ExecutionArchitecture::Amd64,
  };
  request.root = ExecutionTarget::Oci {
    reference: format!("example.invalid/factory@sha256:{}", "0".repeat(64)),
    platform,
    isolation: OciIsolation::Hypervisor,
  };
  assert!(request.validate().is_ok());
  request.factory.as_mut().unwrap().source_visible = true;
  assert!(
    request.validate().is_err(),
    "a direct source mount bypasses the controlled reader"
  );
}

#[cfg(unix)]
#[test]
fn a_private_tool_channel_refuses_execution_roots_without_a_guest_transport() {
  use std::os::unix::{fs::PermissionsExt as _, net::UnixListener};
  let (_temporary, mut request, _) = factory_execution_fixture();
  let control = request.workspace_root.parent().unwrap().join("control");
  octacity_private_fs::create_private_directory(&control).unwrap();
  let socket = control.join("tool.sock");
  let _listener = UnixListener::bind(&socket).unwrap();
  std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
  let factory = request.factory.as_mut().unwrap();
  factory.source_visible = false;
  factory.source_read_only = true;
  factory.tool_channel = Some(FactoryToolChannel::new(socket, 5001).unwrap());
  let platform = ExecutionPlatform {
    os: ExecutionOs::Linux,
    architecture: ExecutionArchitecture::Amd64,
  };
  for root in [
    ExecutionTarget::Host { platform },
    ExecutionTarget::Native { platform },
    ExecutionTarget::Oci {
      reference: format!("example.invalid/factory@sha256:{}", "0".repeat(64)),
      platform,
      isolation: OciIsolation::Process,
    },
  ] {
    request.root = root;
    assert!(
      request.validate().is_err(),
      "a backend must never silently ignore the private channel"
    );
  }
}

#[cfg(unix)]
#[test]
fn a_private_tool_channel_refuses_an_unprojected_socket_inside_the_job_tree() {
  use std::os::unix::{fs::PermissionsExt as _, net::UnixListener};
  let (_temporary, mut request, _) = factory_execution_fixture();
  let factory = request.factory.as_mut().unwrap();
  let control = factory.source.parent().unwrap().join("ctl");
  octacity_private_fs::create_private_directory(&control).unwrap();
  let socket = control.join("tool.sock");
  let _listener = UnixListener::bind(&socket).unwrap();
  std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
  factory.source_visible = false;
  factory.source_read_only = true;
  factory.tool_channel = Some(FactoryToolChannel::new(socket, 5001).unwrap());
  if let ExecutionTarget::Oci { isolation, .. } = &mut request.root {
    *isolation = OciIsolation::Hypervisor;
  }
  assert!(
    matches!(request.validate(), Err(ExecutionError::Invalid(_))),
    "a private control socket belongs outside the complete Job tree, even when its directory is unprojected"
  );
}
