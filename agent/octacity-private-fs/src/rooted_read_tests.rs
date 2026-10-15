#[cfg(unix)]
#[test]
fn a_retained_read_root_rejects_symlink_ancestors_and_outside_paths() {
  let temporary = tempfile::tempdir().unwrap();
  let root_path = temporary.path().join("root");
  let outside = temporary.path().join("outside");
  std::fs::create_dir(&root_path).unwrap();
  std::fs::create_dir(&outside).unwrap();
  std::fs::write(root_path.join("visible"), b"admitted source").unwrap();
  std::fs::write(outside.join("private"), b"outside secret").unwrap();
  std::os::unix::fs::symlink(&outside, root_path.join("redirect")).unwrap();

  let root = RootedReadDirectory::open(&root_path).unwrap();
  assert_eq!(root.read_bounded(Path::new("visible"), 64).unwrap(), b"admitted source");
  assert!(root.read_bounded(Path::new("redirect/private"), 64).is_err());
  assert!(root.read_bounded(Path::new("../outside/private"), 64).is_err());
  assert!(root.read_bounded(&outside.join("private"), 64).is_err());
}

#[test]
fn an_invalid_read_bound_is_rejected_before_opening_the_path() {
  let temporary = tempfile::tempdir().unwrap();
  assert_eq!(
    read_bounded_regular_file(&temporary.path().join("missing"), 0)
      .unwrap_err()
      .kind(),
    std::io::ErrorKind::InvalidInput,
  );
}

#[cfg(unix)]
#[test]
fn a_read_root_keeps_its_identity_after_path_replacement() {
  let temporary = tempfile::tempdir().unwrap();
  let source = temporary.path().join("source");
  let outside = temporary.path().join("outside");
  std::fs::create_dir(&source).unwrap();
  std::fs::create_dir(&outside).unwrap();
  std::fs::write(source.join("input"), b"original source").unwrap();
  std::fs::write(outside.join("input"), b"replacement secret").unwrap();
  let reader = RootedReadDirectory::open(&source).unwrap();
  std::fs::rename(&source, temporary.path().join("retained-source")).unwrap();
  std::os::unix::fs::symlink(&outside, &source).unwrap();
  assert!(RootedReadDirectory::open(&source).is_err());
  assert_eq!(reader.read_bounded(Path::new("input"), 64).unwrap(), b"original source");
}

#[cfg(windows)]
#[test]
fn rooted_reads_refuse_an_unqualified_platform() {
  let temporary = tempfile::tempdir().unwrap();
  match RootedReadDirectory::open(temporary.path()) {
    Ok(_) => panic!("an unqualified platform must not enable protected reads"),
    Err(error) => assert_eq!(error.kind(), std::io::ErrorKind::Unsupported),
  }
}
