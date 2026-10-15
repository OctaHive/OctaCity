mod rooted_read_backend_tests {
  use super::*;
  use crate::{ProtectedReadInventory, ProtectedReadRecipe, RootedProtectedReadBackend};

  struct FixedInventory;

  impl ProtectedReadInventory for FixedInventory {
    fn installed_rooted_reader(&self) -> Option<ProtectedReadRecipe> {
      Some(
        ProtectedReadRecipe::new(
          reference("installed-reader"),
          reference("protected-reader"),
          limits(),
          outputs(),
        )
        .unwrap(),
      )
    }
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn an_inventoried_rooted_reader_executes_only_the_exact_authorized_read() {
    use protected_read_dispatcher_tests::{read_broker, read_proposal};

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("source");
    let outside = temporary.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(root.join("input"), b"admitted source").unwrap();
    std::fs::write(outside.join("private"), b"outside secret").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("redirect")).unwrap();

    let backend =
      Arc::new(RootedProtectedReadBackend::from_inventory(&FixedInventory, &root, "/workspace/source").unwrap());
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator, CancellationToken::new()),
      backend.clone(),
      64,
      reader_response_limits(),
    )
    .unwrap();
    assert_eq!(dispatcher.read(read_proposal()).await.unwrap(), b"admitted source");

    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator, CancellationToken::new()),
      backend.clone(),
      64,
      reader_response_limits(),
    )
    .unwrap();
    let mut redirected = read_proposal();
    redirected.arguments = vec!["/workspace/source/redirect/private".to_owned()];
    redirected.paths[0].path = "/workspace/source/redirect/private".to_owned();
    assert_eq!(
      dispatcher.read(redirected).await.unwrap_err(),
      AgentToolActionBrokerError::Denied
    );

    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator, CancellationToken::new()),
      backend.clone(),
      64,
      reader_response_limits(),
    )
    .unwrap();
    let mut changed = read_proposal();
    changed.arguments = vec!["arbitrary command".to_owned()];
    assert_eq!(
      dispatcher.read(changed).await.unwrap_err(),
      AgentToolActionBrokerError::Mismatch
    );

    // A wider ordinary permission ceiling can authorize a different installed
    // executable. Its proof still cannot select this reader implementation.
    let mut substituted = read_proposal();
    substituted.executable = reference("another-executable");
    let mut ceiling = permissions();
    ceiling.tools = vec![reference("installed-reader")];
    ceiling.executables = vec![substituted.executable.clone()];
    ceiling.commands[0].executable = substituted.executable.clone();
    let decision = FactoryToolActionDecisionV3::new(
      substituted.clone().canonicalize().unwrap().proposal_sha256(),
      FactoryToolActionDispositionV3::Allow,
      FactoryToolActionDecisionSourceV3::HardPolicy,
      None,
    )
    .unwrap();
    let proof = enforce_factory_tool_action(substituted, &decision, &ceiling, &ceiling).unwrap();
    assert_eq!(
      backend.read(proof, 64, CancellationToken::new()).await.unwrap_err(),
      AgentToolActionBrokerError::Mismatch,
    );

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(
        Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow)),
        cancellation,
      ),
      backend,
      64,
      reader_response_limits(),
    )
    .unwrap();
    assert_eq!(
      dispatcher.read(read_proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::Cancelled
    );
  }
}
