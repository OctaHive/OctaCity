mod protected_read_dispatcher_tests {
  use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
  };

  use super::*;

  struct FileReader {
    expected: FactoryToolActionProposalV3,
    file: std::path::PathBuf,
    reads: AtomicUsize,
  }

  impl ProtectedReadBackend for FileReader {
    fn read<'a>(
      &'a self,
      action: AuthorizedFactoryToolAction,
      max_bytes: u64,
      cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AgentToolActionBrokerError>> + Send + 'a>> {
      Box::pin(async move {
        // The installed backend consumes the proof and checks the entire exact
        // operation. It cannot reinterpret a permitted shell command as a read.
        if action.into_proposal() != self.expected {
          return Err(AgentToolActionBrokerError::Mismatch);
        }
        if cancellation.is_cancelled() {
          return Err(AgentToolActionBrokerError::Cancelled);
        }
        self.reads.fetch_add(1, Ordering::SeqCst);
        octacity_private_fs::read_bounded_regular_file(&self.file, max_bytes)
          .map_err(|_| AgentToolActionBrokerError::Denied)
      })
    }
  }

  pub(super) fn read_proposal() -> FactoryToolActionProposalV3 {
    FactoryToolActionProposalV3 {
      tool: reference("installed-reader"),
      executable: reference("protected-reader"),
      arguments: vec!["/workspace/source/input".to_owned()],
      paths: vec![FactoryToolPathV3 {
        path: "/workspace/source/input".to_owned(),
        access: FactoryToolPathAccessV3::Read,
      }],
      ..proposal()
    }
  }

  fn read_channel_body() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
      "schema_version":1,"operation":"read","proposal":read_proposal()
    }))
    .unwrap()
  }

  struct CancellingReader;

  impl ProtectedReadBackend for CancellingReader {
    fn read<'a>(
      &'a self,
      action: AuthorizedFactoryToolAction,
      _max_bytes: u64,
      cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AgentToolActionBrokerError>> + Send + 'a>> {
      Box::pin(async move {
        assert_eq!(action.into_proposal(), read_proposal());
        cancellation.cancel();
        Ok(b"private result".to_vec())
      })
    }
  }

  struct OversizedReader;

  impl ProtectedReadBackend for OversizedReader {
    fn read<'a>(
      &'a self,
      action: AuthorizedFactoryToolAction,
      _max_bytes: u64,
      _cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AgentToolActionBrokerError>> + Send + 'a>> {
      Box::pin(async move {
        assert_eq!(action.into_proposal(), read_proposal());
        Ok(vec![b'x'; 65])
      })
    }
  }

  #[tokio::test]
  async fn a_backend_response_exceeding_the_installed_read_bound_is_withheld() {
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator, CancellationToken::new()),
      Arc::new(OversizedReader),
      64,
      reader_response_limits(),
    )
    .unwrap();
    assert_eq!(
      dispatcher.read(read_proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::Denied
    );
  }

  #[tokio::test]
  async fn cancellation_during_the_backend_response_withholds_protected_bytes() {
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator, CancellationToken::new()),
      Arc::new(CancellingReader),
      64,
      reader_response_limits(),
    )
    .unwrap();
    assert_eq!(
      dispatcher.read(read_proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::Cancelled
    );
  }

  pub(super) fn read_broker(
    coordinator: Arc<BlockingCoordinator>,
    cancellation: CancellationToken,
  ) -> Arc<AgentToolActionBroker> {
    let mut broker = broker(coordinator, Duration::from_secs(30), cancellation);
    let mut ceiling = permissions();
    ceiling.tools = vec![reference("installed-reader")];
    ceiling.executables = vec![reference("protected-reader")];
    ceiling.commands[0].executable = reference("protected-reader");
    broker.signed_permissions = ceiling.clone();
    broker.local_permissions = ceiling;
    Arc::new(broker)
  }

  #[tokio::test]
  async fn private_reader_authorization_waits_for_the_fenced_answer_without_reading() {
    use tokio::io::AsyncWriteExt as _;

    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator.clone(), CancellationToken::new()),
      Arc::new(CancellingReader),
      64,
      reader_response_limits(),
    )
    .unwrap();
    let body = serde_json::to_vec(&serde_json::json!({
      "schema_version":1,"operation":"authorize","proposal":read_proposal()
    }))
    .unwrap();
    let (mut producer, consumer) = tokio::io::duplex(4096);
    let response = tokio::spawn(async move { dispatcher.read_channel(consumer).await });
    producer.write_u32(u32::try_from(body.len()).unwrap()).await.unwrap();
    producer.write_all(&body).await.unwrap();
    coordinator.entered.notified().await;
    assert!(
      !response.is_finished(),
      "preflight must wait for the fenced disposition"
    );
    let wire = serde_json::to_string(&coordinator.requests.lock().unwrap()[0]).unwrap();
    assert!(!wire.contains("/workspace/source/input"));
    coordinator.release.notify_one();
    assert!(
      response.await.unwrap().unwrap().is_empty(),
      "authorization returns no file bytes"
    );
  }

  #[tokio::test]
  async fn private_reader_rejects_incomplete_malformed_oversized_and_unversioned_frames() {
    let body = serde_json::to_vec(&read_proposal()).unwrap();
    let mut unversioned = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
    unversioned.extend(body);
    let frames = [
      vec![0, 0],
      vec![0, 0, 0, 0],
      vec![0, 0, 0, 1, b'{'],
      vec![0, 0, 0, 3, b'{'],
      u32::MAX.to_be_bytes().to_vec(),
      unversioned,
    ];
    for frame in frames {
      let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
      coordinator.release.notify_one();
      let dispatcher = ProtectedReadDispatcher::new(
        read_broker(coordinator.clone(), CancellationToken::new()),
        Arc::new(CancellingReader),
        64,
        reader_response_limits(),
      )
      .unwrap();
      assert_eq!(
        dispatcher.read_channel(frame.as_slice()).await.unwrap_err(),
        AgentToolActionBrokerError::Denied
      );
      assert!(coordinator.requests.lock().unwrap().is_empty());
    }
  }

  #[tokio::test]
  async fn private_reader_lease_cancellation_interrupts_a_blocked_frame_read() {
    use std::task::{Context, Poll, Waker};

    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let cancellation = CancellationToken::new();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator.clone(), cancellation.clone()),
      Arc::new(CancellingReader),
      64,
      reader_response_limits(),
    )
    .unwrap();
    let (_producer, consumer) = tokio::io::duplex(4);
    let mut response = Box::pin(dispatcher.read_channel(consumer));
    assert!(matches!(
      response.as_mut().poll(&mut Context::from_waker(Waker::noop())),
      Poll::Pending
    ));
    cancellation.cancel();
    assert_eq!(response.await.unwrap_err(), AgentToolActionBrokerError::Cancelled);
    assert!(coordinator.requests.lock().unwrap().is_empty());
  }

  #[tokio::test]
  async fn dispatcher_consumes_exact_authority_before_reading_the_installed_backend() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input");
    std::fs::write(&file, b"protected contents").unwrap();
    let backend = Arc::new(FileReader {
      expected: read_proposal(),
      file,
      reads: AtomicUsize::new(0),
    });

    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator, CancellationToken::new()),
      backend.clone(),
      64,
      reader_response_limits(),
    )
    .unwrap();
    assert_eq!(dispatcher.read(read_proposal()).await.unwrap(), b"protected contents");
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);

    for (coordinator, cancellation, expected) in [
      (
        Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Deny)),
        CancellationToken::new(),
        AgentToolActionBrokerError::Denied,
      ),
      (
        Arc::new(BlockingCoordinator::mismatched()),
        CancellationToken::new(),
        AgentToolActionBrokerError::Mismatch,
      ),
      (
        Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow)),
        {
          let token = CancellationToken::new();
          token.cancel();
          token
        },
        AgentToolActionBrokerError::Cancelled,
      ),
    ] {
      coordinator.release.notify_one();
      let dispatcher = ProtectedReadDispatcher::new(
        read_broker(coordinator, cancellation),
        backend.clone(),
        64,
        reader_response_limits(),
      )
      .unwrap();
      assert_eq!(dispatcher.read(read_proposal()).await.unwrap_err(), expected);
      assert_eq!(
        backend.reads.load(Ordering::SeqCst),
        1,
        "a rejected exchange must not invoke the physical reader"
      );
    }

    // Even a server-authorized argument change cannot substitute a different
    // operation for the installed backend's one immutable read contract.
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
    changed.arguments = vec!["arbitrary-command".to_owned()];
    assert_eq!(
      dispatcher.read(changed).await.unwrap_err(),
      AgentToolActionBrokerError::Mismatch
    );
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn hook_authorization_preserves_the_single_read_response_allowance() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input");
    std::fs::write(&file, b"facts").unwrap();
    let backend = Arc::new(FileReader {
      expected: read_proposal(),
      file,
      reads: AtomicUsize::new(0),
    });
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator.clone(), CancellationToken::new()),
      backend.clone(),
      5,
      octacity_protocol::FactoryToolResponseLimitsV3 {
        per_call_bytes: 5,
        total_bytes: 5,
      },
    )
    .unwrap();
    for (operation, expected) in [("authorize", b"".as_slice()), ("read", b"facts".as_slice())] {
      let body = serde_json::to_vec(&serde_json::json!({
        "schema_version":1,"operation":operation,"proposal":read_proposal()
      }))
      .unwrap();
      let mut frame = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
      frame.extend(body);
      coordinator.release.notify_one();
      assert_eq!(dispatcher.read_channel(frame.as_slice()).await.unwrap(), expected);
      assert_eq!(backend.reads.load(Ordering::SeqCst), usize::from(operation == "read"));
    }
    {
      let requests = coordinator.requests.lock().unwrap();
      assert_eq!(
        requests.len(),
        2,
        "physical I/O must obtain its own current authorization"
      );
      assert_eq!(requests[0].proposal_sha256, requests[1].proposal_sha256);
    }
    coordinator.release.notify_one();
    assert_eq!(
      dispatcher.read(read_proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::Denied,
      "the preflight grants no extra physical read or response bytes"
    );
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn a_private_read_frame_dispatches_one_consumed_authority_and_rejects_malformed_input() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input");
    std::fs::write(&file, b"protected contents").unwrap();
    let backend = Arc::new(FileReader {
      expected: read_proposal(),
      file,
      reads: AtomicUsize::new(0),
    });
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    coordinator.release.notify_one();
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator.clone(), CancellationToken::new()),
      backend.clone(),
      64,
      reader_response_limits(),
    )
    .unwrap();
    let body = read_channel_body();
    let mut frame = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
    frame.extend(body);
    assert_eq!(
      dispatcher.read_channel(frame.as_slice()).await.unwrap(),
      b"protected contents"
    );
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
    assert_eq!(coordinator.requests.lock().unwrap().len(), 1, "authorize exactly once");

    assert_eq!(
      dispatcher
        .read_channel(u32::MAX.to_be_bytes().as_slice())
        .await
        .unwrap_err(),
      AgentToolActionBrokerError::Denied
    );
    assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
    assert_eq!(coordinator.requests.lock().unwrap().len(), 1);
    for invalid in [
      serde_json::json!({"schema_version":2,"operation":"read","proposal":read_proposal()}),
      serde_json::json!({"schema_version":1,"operation":"execute","proposal":read_proposal()}),
      serde_json::json!({"schema_version":1,"operation":"authorize","proposal":read_proposal(),"reuse_grant":true}),
    ] {
      let body = serde_json::to_vec(&invalid).unwrap();
      let mut frame = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
      frame.extend(body);
      coordinator.release.notify_one();
      assert_eq!(
        dispatcher.read_channel(frame.as_slice()).await.unwrap_err(),
        AgentToolActionBrokerError::Denied
      );
      assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
      assert_eq!(coordinator.requests.lock().unwrap().len(), 1);
    }
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn an_owned_private_reader_serves_authorized_bytes_and_removes_its_endpoint() {
    use std::os::unix::fs::PermissionsExt as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = root.join("input");
    std::fs::write(&file, b"protected contents").unwrap();
    let backend = Arc::new(FileReader {
      expected: read_proposal(),
      file,
      reads: AtomicUsize::new(0),
    });
    let socket = root.join("reader.sock");
    let body = read_channel_body();
    for (disposition, expected_status, expected_bytes) in [
      (
        FactoryToolActionDispositionV3::Allow,
        0,
        b"protected contents".as_slice(),
      ),
      (FactoryToolActionDispositionV3::Deny, 1, b"".as_slice()),
    ] {
      let coordinator = Arc::new(BlockingCoordinator::new(disposition));
      coordinator.release.notify_one();
      let dispatcher = Arc::new(
        ProtectedReadDispatcher::new(
          read_broker(coordinator.clone(), CancellationToken::new()),
          backend.clone(),
          64,
          reader_response_limits(),
        )
        .unwrap(),
      );
      let service = ProtectedReadChannel::bind(dispatcher, socket.clone(), 5001).unwrap();
      assert_eq!(service.endpoint().guest_port(), 5001);
      let mut peer = tokio::net::UnixStream::connect(service.endpoint().host_socket())
        .await
        .unwrap();
      peer.write_u32(u32::try_from(body.len()).unwrap()).await.unwrap();
      peer.write_all(&body).await.unwrap();
      assert_eq!(peer.read_u8().await.unwrap(), expected_status);
      let length = peer.read_u32().await.unwrap();
      assert_eq!(length as usize, expected_bytes.len());
      let mut bytes = vec![0; length as usize];
      peer.read_exact(&mut bytes).await.unwrap();
      assert_eq!(bytes, expected_bytes);
      assert_eq!(
        backend.reads.load(Ordering::SeqCst),
        1,
        "denied requests never reach the reader"
      );
      assert_eq!(coordinator.requests.lock().unwrap().len(), 1);
      drop(peer);
      service.shutdown().await.unwrap();
      assert!(!socket.exists(), "the execution owner removes its private endpoint");
    }
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn ending_the_execution_closes_pending_reader_exchanges_without_releasing_bytes() {
    use octacity_execution::ExecutionPaths;
    use std::os::unix::fs::PermissionsExt as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let file = root.join("input");
    std::fs::write(&file, b"protected contents").unwrap();
    let backend = Arc::new(FileReader {
      expected: read_proposal(),
      file,
      reads: AtomicUsize::new(0),
    });
    let socket = root.join("reader.sock");
    let body = read_channel_body();
    for stop in ["wait", "kill", "destroy", "final"] {
      let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
      let dispatcher = Arc::new(
        ProtectedReadDispatcher::new(
          read_broker(coordinator.clone(), CancellationToken::new()),
          backend.clone(),
          64,
          reader_response_limits(),
        )
        .unwrap(),
      );
      let service = ProtectedReadChannel::bind(dispatcher, socket.clone(), 5001).unwrap();
      let paths = ExecutionPaths {
        workspace: root.clone(),
        data_dir: root.join("data"),
        plugin_projection: Some(octacity_execution::ExecutionPluginProjection {
          directory: root.join("plugins"),
          lock: root.join("plugins.lock"),
        }),
        cache: None,
      };
      let execution = crate::lifecycle::tests::execution_fixture(paths.clone(), None, 0);
      let mut execution = service.attach(execution);
      assert_eq!(execution.paths(), &paths);
      let mut peer = tokio::net::UnixStream::connect(&socket).await.unwrap();
      peer.write_u32(u32::try_from(body.len()).unwrap()).await.unwrap();
      peer.write_all(&body).await.unwrap();
      coordinator.entered.notified().await;
      match stop {
        "wait" => {
          assert_eq!(execution.wait().await.unwrap().code, Some(0));
          execution.destroy().await.unwrap();
        }
        "kill" => {
          execution.kill().await.unwrap();
          execution.destroy().await.unwrap();
        }
        "final" => {
          execution.sample_final_usage().await.unwrap();
          let sealed = !socket.exists();
          execution.destroy().await.unwrap();
          assert!(
            sealed,
            "terminal accounting must seal the private channel before sampling"
          );
        }
        "destroy" => execution.destroy().await.unwrap(),
        _ => unreachable!(),
      }
      let mut bytes = Vec::new();
      peer.read_to_end(&mut bytes).await.unwrap();
      assert!(
        bytes.is_empty(),
        "ending the execution cannot release an in-flight read"
      );
      assert!(!socket.exists());
      coordinator.release.notify_one();
      assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
      assert_eq!(coordinator.requests.lock().unwrap().len(), 1);
    }
  }
  struct ResponseBoundReader {
    reader: FileReader,
    bounds: std::sync::Mutex<Vec<u64>>,
  }

  impl ProtectedReadBackend for ResponseBoundReader {
    fn read<'a>(
      &'a self,
      action: AuthorizedFactoryToolAction,
      max_bytes: u64,
      cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AgentToolActionBrokerError>> + Send + 'a>> {
      self.bounds.lock().unwrap().push(max_bytes);
      self.reader.read(action, max_bytes, cancellation)
    }
  }

  #[tokio::test]
  async fn repeated_proposals_share_the_job_response_byte_ceiling_and_stop_physical_reads() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input");
    std::fs::write(&file, b"hi").unwrap();
    let backend = Arc::new(ResponseBoundReader {
      reader: FileReader {
        expected: read_proposal(),
        file,
        reads: AtomicUsize::new(0),
      },
      bounds: std::sync::Mutex::new(vec![]),
    });
    let coordinator = Arc::new(BlockingCoordinator::new(FactoryToolActionDispositionV3::Allow));
    let dispatcher = ProtectedReadDispatcher::new(
      read_broker(coordinator.clone(), CancellationToken::new()),
      backend.clone(),
      64,
      octacity_protocol::FactoryToolResponseLimitsV3 {
        per_call_bytes: 5,
        total_bytes: 8,
      },
    )
    .unwrap();
    for _ in 0..4 {
      coordinator.release.notify_one();
      assert_eq!(dispatcher.read(read_proposal()).await.unwrap(), b"hi");
    }
    coordinator.release.notify_one();
    assert_eq!(
      dispatcher.read(read_proposal()).await.unwrap_err(),
      AgentToolActionBrokerError::Denied
    );
    assert_eq!(*backend.bounds.lock().unwrap(), vec![5, 5, 4, 2]);
    assert_eq!(
      backend.reader.reads.load(Ordering::SeqCst),
      4,
      "the same digest cannot unlock extra bytes or an extra physical read"
    );
  }
}
