use std::{collections::BTreeMap, path::Path, process::Command, sync::Arc};

#[cfg(windows)]
use std::sync::atomic::{AtomicU64, Ordering};

use octacity_protocol::{
  BeginCacheSessionResponse, CachePolicy, PlatformArchitecture, PlatformOs, PlatformSpec, RemoteCacheGrant,
  RuntimeTarget,
};

use super::*;

struct FixtureRoot {
  path: PathBuf,
  #[cfg(not(windows))]
  _temporary: tempfile::TempDir,
}

impl FixtureRoot {
  fn path(&self) -> &Path {
    &self.path
  }
}

fn fixture_root() -> FixtureRoot {
  #[cfg(windows)]
  {
    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
    let profile = std::env::var_os("USERPROFILE").expect("Windows tests require USERPROFILE");
    let profile = std::fs::canonicalize(profile).expect("Windows tests require a canonical USERPROFILE");
    let volume_root = profile
      .ancestors()
      .last()
      .expect("Windows profile must have a volume root");
    for _ in 0..16 {
      let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
      let path = volume_root.join(format!(".octacity-cache-test-{}-{sequence}", std::process::id()));
      match octacity_private_fs::create_private_directory(&path) {
        Ok(()) => return FixtureRoot { path },
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => panic!("failed to create protected Windows cache fixture: {error}"),
      }
    }
    panic!("failed to allocate a protected Windows cache fixture")
  }
  #[cfg(not(windows))]
  {
    let temporary = tempfile::tempdir().unwrap();
    FixtureRoot {
      path: temporary.path().to_owned(),
      _temporary: temporary,
    }
  }
}

#[cfg(windows)]
impl Drop for FixtureRoot {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.path);
  }
}

struct SessionFixture {
  _root: FixtureRoot,
  cache: PathBuf,
  job: PathBuf,
}

impl SessionFixture {
  fn new() -> Self {
    let root = fixture_root();
    let cache = root.path().join("cache");
    let job = root.path().join("job");
    octacity_private_fs::create_private_directory(&cache).unwrap();
    octacity_private_fs::create_private_directory(&job).unwrap();
    Self {
      _root: root,
      cache,
      job,
    }
  }
}

fn manager_config(root: &Path) -> CacheSessionManagerConfig {
  CacheSessionManagerConfig {
    root: root.to_owned(),
    capacity: LocalCacheCapacity::new(100, 90, 80).unwrap(),
    max_scopes: 2,
    allow_read: true,
    allow_write: true,
    allowed_origins: vec!["https://cache.example".to_owned()],
    ca_certificate_file: None,
    native_identities: BTreeMap::from([("linux-amd64".to_owned(), "rust-1.98-toolchain-v1".to_owned())]),
    request_timeout_seconds: 10,
    max_parallel_transfers: 2,
  }
}

fn manager(root: &Path) -> CacheSessionManager {
  CacheSessionManager::new(manager_config(root)).unwrap()
}

const CACHE_LOCK_CHILD_ROOT: &str = "OCTACITY_CACHE_LOCK_CHILD_ROOT";

#[test]
fn cache_root_lock_child() {
  let Some(root) = std::env::var_os(CACHE_LOCK_CHILD_ROOT) else {
    return;
  };
  let error = CacheSessionManager::new(manager_config(Path::new(&root))).unwrap_err();
  assert!(error.to_string().contains("already owned"));
}

#[test]
fn one_agent_process_exclusively_owns_a_cache_root() {
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  let first = manager(&cache);

  let status = Command::new(std::env::current_exe().unwrap())
    .args(["--exact", "tests::cache_root_lock_child", "--nocapture"])
    .env(CACHE_LOCK_CHILD_ROOT, &cache)
    .status()
    .unwrap();
  assert!(status.success());

  drop(first);
  CacheSessionManager::new(manager_config(&cache)).unwrap();
}

fn policy() -> CachePolicy {
  CachePolicy {
    namespace: "project/main".to_owned(),
    read: true,
    write: true,
  }
}

fn runtime() -> RuntimeTarget {
  RuntimeTarget::Native {
    platform: PlatformSpec {
      os: PlatformOs::Linux,
      architecture: PlatformArchitecture::Amd64,
    },
  }
}

fn lifetime() -> CacheSessionContext {
  CacheSessionContext {
    now: 1,
    remaining_job: Duration::from_secs(20),
    cancellation: CancellationToken::new(),
  }
}

fn local_grant(scope: &str) -> BeginCacheSessionResponse {
  BeginCacheSessionResponse {
    protocol_version: 1,
    request_id: "request".to_owned(),
    session_id: format!("session-{scope}"),
    scope_id: scope.to_owned(),
    remote: None,
  }
}

#[tokio::test]
async fn prepares_persistent_l1_and_revokes_private_bearer() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let prepared = manager(&cache)
    .prepare(
      &policy(),
      BeginCacheSessionResponse {
        protocol_version: 1,
        request_id: "request".to_owned(),
        session_id: "session".to_owned(),
        scope_id: "scope".to_owned(),
        remote: Some(RemoteCacheGrant {
          endpoint: "https://cache.example/base".to_owned(),
          bearer_token: "secret-token".to_owned(),
          expires_at: 100,
        }),
      },
      &runtime(),
      &NetworkPolicy::Unrestricted,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  let token = prepared.execution().token_file.clone().unwrap();
  assert_eq!(fs::read(&token).await.unwrap(), b"secret-token");
  assert!(prepared.execution().local_directory.is_dir());
  let access_marker = cache
    .join(ACCESS_LAYOUT_DIRECTORY)
    .join(prepared.execution().local_directory.file_name().unwrap());
  assert!(access_marker.is_file());
  assert!(!access_marker.starts_with(&prepared.execution().local_directory));
  assert_eq!(
    prepared.runner().runtime,
    RuntimeIdentity::native(
      octa_cache_protocol::PlatformOs::Linux,
      octa_cache_protocol::PlatformArchitecture::Amd64,
      "rust-1.98-toolchain-v1",
    )
    .unwrap()
  );
  assert!(!format!("{prepared:?}").contains("secret-token"));
  prepared.revoke().await.unwrap();
  assert!(!token.exists());
}

#[tokio::test]
async fn rejects_unapproved_expired_and_unidentified_sessions() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let grant = |endpoint: &str, expires_at| BeginCacheSessionResponse {
    protocol_version: 1,
    request_id: "request".to_owned(),
    session_id: "session".to_owned(),
    scope_id: "scope".to_owned(),
    remote: Some(RemoteCacheGrant {
      endpoint: endpoint.to_owned(),
      bearer_token: "token".to_owned(),
      expires_at,
    }),
  };
  assert!(
    manager(&cache)
      .prepare(
        &policy(),
        grant("https://other.example", 100),
        &runtime(),
        &NetworkPolicy::Unrestricted,
        &job,
        lifetime(),
      )
      .await
      .is_err()
  );
  assert!(!cache.join("v1").exists());
  assert!(
    manager(&cache)
      .prepare(
        &policy(),
        grant("https://cache.example", 30),
        &runtime(),
        &NetworkPolicy::Unrestricted,
        &job,
        lifetime(),
      )
      .await
      .unwrap_err()
      .to_string()
      .contains("before the job")
  );
  assert!(
    manager(&cache)
      .prepare(
        &policy(),
        grant("https://cache.example", 5),
        &runtime(),
        &NetworkPolicy::Unrestricted,
        &job,
        lifetime(),
      )
      .await
      .is_err()
  );
  let mut no_identity = manager(&cache);
  no_identity.native_identities.clear();
  assert!(
    no_identity
      .prepare(
        &policy(),
        grant("https://cache.example", 100),
        &runtime(),
        &NetworkPolicy::Unrestricted,
        &job,
        lifetime(),
      )
      .await
      .is_err()
  );
}

#[tokio::test]
async fn narrows_remote_access_through_the_signed_network_policy() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let grant = || BeginCacheSessionResponse {
    protocol_version: 1,
    request_id: "request".to_owned(),
    session_id: "session".to_owned(),
    scope_id: "scope".to_owned(),
    remote: Some(RemoteCacheGrant {
      endpoint: "https://cache.example/base".to_owned(),
      bearer_token: "secret-token".to_owned(),
      expires_at: 100,
    }),
  };

  let local_only = manager(&cache)
    .prepare(
      &policy(),
      grant(),
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  assert!(local_only.runner().remote_endpoint.is_none());
  assert!(local_only.execution().token_file.is_none());
  local_only.revoke().await.unwrap();

  let denied = NetworkPolicy::Restricted {
    allowed_hosts: vec!["source.example".to_owned()],
  };
  assert!(
    manager(&cache)
      .prepare(&policy(), grant(), &runtime(), &denied, &job, lifetime(),)
      .await
      .unwrap_err()
      .to_string()
      .contains("signed network allowlist")
  );

  let allowed = NetworkPolicy::Restricted {
    allowed_hosts: vec!["cache.example".to_owned()],
  };
  let prepared = manager(&cache)
    .prepare(&policy(), grant(), &runtime(), &allowed, &job, lifetime())
    .await
    .unwrap();
  assert_eq!(
    prepared.runner().remote_endpoint.as_deref(),
    Some("https://cache.example/base")
  );
  prepared.revoke().await.unwrap();
}

#[tokio::test]
async fn bounds_trust_scopes_by_reclaiming_the_oldest_inactive_scope() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let manager = manager(&cache);
  for scope in ["scope-a", "scope-b"] {
    manager
      .prepare(
        &policy(),
        local_grant(scope),
        &runtime(),
        &NetworkPolicy::Disabled,
        &job,
        lifetime(),
      )
      .await
      .unwrap()
      .revoke()
      .await
      .unwrap();
  }
  let previous = [
    manager
      .local_directory(
        "scope-a",
        &runtime_identity(&runtime(), &manager.native_identities).unwrap(),
      )
      .unwrap(),
    manager
      .local_directory(
        "scope-b",
        &runtime_identity(&runtime(), &manager.native_identities).unwrap(),
      )
      .unwrap(),
  ];
  manager
    .prepare(
      &policy(),
      local_grant("scope-c"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap()
    .revoke()
    .await
    .unwrap();
  assert_eq!(std::fs::read_dir(cache.join("v1")).unwrap().count(), 2);
  assert_eq!(previous.iter().filter(|path| path.exists()).count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_admissions_never_exceed_the_scope_limit() {
  let fixture = SessionFixture::new();
  let mut config = manager_config(&fixture.cache);
  config.max_scopes = 2;
  let manager = Arc::new(CacheSessionManager::new(config).unwrap());
  let barrier = Arc::new(tokio::sync::Barrier::new(32));
  let mut tasks = Vec::new();

  for index in 0..32 {
    let manager = manager.clone();
    let barrier = barrier.clone();
    let job = fixture.job.clone();
    tasks.push(tokio::spawn(async move {
      barrier.wait().await;
      let result = manager
        .prepare(
          &policy(),
          local_grant(&format!("scope-{index}")),
          &runtime(),
          &NetworkPolicy::Disabled,
          &job,
          lifetime(),
        )
        .await;
      match result {
        Ok(session) => session.revoke().await.unwrap(),
        Err(error) => assert!(
          error.to_string().contains("no inactive scope"),
          "unexpected error: {error}"
        ),
      }
    }));
  }
  for task in tasks {
    task.await.unwrap();
  }

  assert!(
    std::fs::read_dir(fixture.cache.join(SCOPE_LAYOUT_DIRECTORY))
      .unwrap()
      .count()
      <= 2
  );
}

#[tokio::test]
async fn cancellation_stops_scope_limit_reclamation_before_admission() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let mut config = manager_config(&cache);
  config.max_scopes = 1;
  let manager = CacheSessionManager::new(config).unwrap();
  let first = manager
    .prepare(
      &policy(),
      local_grant("scope-a"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  let first_directory = first.execution().local_directory.clone();
  first.revoke().await.unwrap();

  let context = lifetime();
  context.cancellation.cancel();
  assert!(matches!(
    manager
      .prepare(
        &policy(),
        local_grant("scope-b"),
        &runtime(),
        &NetworkPolicy::Disabled,
        &job,
        context,
      )
      .await,
    Err(CacheSessionError::Cancelled)
  ));
  assert!(first_directory.is_dir());
}

#[tokio::test]
async fn reclamation_never_removes_an_active_scope() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let manager = manager(&cache);
  manager
    .prepare(
      &policy(),
      local_grant("inactive"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap()
    .revoke()
    .await
    .unwrap();
  let active = manager
    .prepare(
      &policy(),
      local_grant("active"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  let active_path = active.execution().local_directory.clone();
  let report = manager
    .reclaim_inactive(u64::MAX, CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(report.removed_scopes, 1);
  assert!(active_path.is_dir());
  active.revoke().await.unwrap();
}

#[tokio::test]
async fn active_scopes_cannot_be_evicted_to_exceed_the_scope_limit() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let mut config = manager_config(&cache);
  config.max_scopes = 1;
  let manager = CacheSessionManager::new(config).unwrap();
  let active = manager
    .prepare(
      &policy(),
      local_grant("active"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  assert!(
    manager
      .prepare(
        &policy(),
        local_grant("second"),
        &runtime(),
        &NetworkPolicy::Disabled,
        &job,
        lifetime(),
      )
      .await
      .unwrap_err()
      .to_string()
      .contains("no inactive scope")
  );
  active.revoke().await.unwrap();
}

#[tokio::test]
async fn reclamation_leaves_unrecognized_cache_state_untouched() {
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  let layout = cache.join("v1");
  octacity_private_fs::create_private_directory(&layout).unwrap();
  let foreign = layout.join("operator-note");
  std::fs::write(&foreign, "do not delete").unwrap();

  let error = manager(&cache)
    .reclaim_inactive(u64::MAX, CancellationToken::new())
    .await
    .unwrap_err();

  assert!(error.to_string().contains("unexpected entry"));
  assert_eq!(std::fs::read_to_string(foreign).unwrap(), "do not delete");
}

#[cfg(windows)]
#[tokio::test]
async fn scope_admission_rejects_directory_junctions() {
  let fixture = SessionFixture::new();
  let manager = manager(&fixture.cache);
  let layout = fixture.cache.join(SCOPE_LAYOUT_DIRECTORY);
  octacity_private_fs::create_private_directory(&layout).unwrap();
  let outside = fixture._root.path().join("outside");
  octacity_private_fs::create_private_directory(&outside).unwrap();
  std::fs::write(outside.join("keep"), "outside cache scope").unwrap();
  let junction = layout.join("a".repeat(SCOPE_DIRECTORY_HEX_LENGTH));
  let status = Command::new("cmd")
    .args(["/C", "mklink", "/J"])
    .arg(&junction)
    .arg(&outside)
    .status()
    .unwrap();
  assert!(status.success(), "failed to create the test directory junction");

  let error = manager
    .prepare(
      &policy(),
      local_grant("new-scope"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &fixture.job,
      lifetime(),
    )
    .await
    .unwrap_err();
  assert!(error.to_string().contains("non-redirecting directory"));
  assert!(outside.join("keep").is_file());
}

#[tokio::test]
async fn cancelled_reclamation_leaves_an_inactive_scope_recoverable() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let manager = manager(&cache);
  let prepared = manager
    .prepare(
      &policy(),
      BeginCacheSessionResponse {
        protocol_version: 1,
        request_id: "request".to_owned(),
        session_id: "session".to_owned(),
        scope_id: "scope".to_owned(),
        remote: None,
      },
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  let scope = prepared.execution().local_directory.clone();
  prepared.revoke().await.unwrap();
  let cancellation = CancellationToken::new();
  cancellation.cancel();

  assert!(matches!(
    manager.reclaim_inactive(u64::MAX, cancellation).await,
    Err(CacheSessionError::Cancelled)
  ));
  assert!(scope.is_dir());

  let report = manager
    .reclaim_inactive(u64::MAX, CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(report.removed_scopes, 1);
  assert!(!scope.exists());
}

#[tokio::test]
async fn abandoned_tombstone_is_removed_before_the_scope_is_readmitted() {
  let fixture = SessionFixture::new();
  let manager = manager(&fixture.cache);
  let prepared = manager
    .prepare(
      &policy(),
      local_grant("reused"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &fixture.job,
      lifetime(),
    )
    .await
    .unwrap();
  let live = prepared.execution().local_directory.clone();
  prepared.revoke().await.unwrap();
  std::fs::write(live.join("partial-output"), "must not be readmitted").unwrap();

  let tombstone = fixture
    .cache
    .join(RECLAIM_LAYOUT_DIRECTORY)
    .join(live.file_name().unwrap());
  std::fs::rename(&live, &tombstone).unwrap();

  let replacement = manager
    .prepare(
      &policy(),
      local_grant("reused"),
      &runtime(),
      &NetworkPolicy::Disabled,
      &fixture.job,
      lifetime(),
    )
    .await
    .unwrap();
  assert_eq!(replacement.execution().local_directory, live);
  assert!(!replacement.execution().local_directory.join("partial-output").exists());
  assert!(!tombstone.exists());
  replacement.revoke().await.unwrap();
}

#[tokio::test]
async fn construction_defers_abandoned_tombstone_cleanup_to_a_cancellable_pass() {
  let fixture = SessionFixture::new();
  let reclaim = fixture.cache.join(RECLAIM_LAYOUT_DIRECTORY);
  octacity_private_fs::create_private_directory(&reclaim).unwrap();
  let tombstone = reclaim.join("a".repeat(SCOPE_DIRECTORY_HEX_LENGTH));
  octacity_private_fs::create_private_directory(&tombstone).unwrap();
  std::fs::write(tombstone.join("partial-output"), "stale").unwrap();

  let manager = manager(&fixture.cache);
  assert!(tombstone.exists(), "construction must not perform recursive cleanup");

  let cancellation = CancellationToken::new();
  cancellation.cancel();
  assert!(matches!(
    manager.reclaim_inactive(u64::MAX, cancellation).await,
    Err(CacheSessionError::Cancelled)
  ));
  assert!(tombstone.exists(), "a cancelled pass must leave recoverable state");

  let report = manager
    .reclaim_inactive(u64::MAX, CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(report.removed_scopes, 1);
  assert!(!tombstone.exists());
}

#[tokio::test]
async fn reclamation_rejects_a_corrupt_host_only_access_marker() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();
  let manager = manager(&cache);
  let prepared = manager
    .prepare(
      &policy(),
      BeginCacheSessionResponse {
        protocol_version: 1,
        request_id: "request".to_owned(),
        session_id: "session".to_owned(),
        scope_id: "scope".to_owned(),
        remote: None,
      },
      &runtime(),
      &NetworkPolicy::Disabled,
      &job,
      lifetime(),
    )
    .await
    .unwrap();
  let scope = prepared.execution().local_directory.clone();
  prepared.revoke().await.unwrap();
  let marker = cache.join(ACCESS_LAYOUT_DIRECTORY).join(scope.file_name().unwrap());
  std::fs::remove_file(&marker).unwrap();
  octacity_private_fs::create_private_directory(&marker).unwrap();

  let error = manager
    .reclaim_inactive(u64::MAX, CancellationToken::new())
    .await
    .unwrap_err();

  assert!(error.to_string().contains("not a regular file"));
  assert!(scope.is_dir());
}

#[test]
fn construction_enforces_the_complete_local_policy_boundary() {
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  octacity_private_fs::create_private_directory(&cache).unwrap();

  let mut config = manager_config(&cache);
  config.allow_read = false;
  config.allow_write = false;
  assert!(
    CacheSessionManager::new(config)
      .unwrap_err()
      .to_string()
      .contains("reading, writing")
  );

  let mut config = manager_config(&cache);
  config.allowed_origins = vec![
    "https://cache.example".to_owned(),
    "https://CACHE.example:443".to_owned(),
  ];
  assert!(
    CacheSessionManager::new(config)
      .unwrap_err()
      .to_string()
      .contains("duplicate")
  );

  let mut config = manager_config(&cache);
  config.request_timeout_seconds = octa_cache_protocol::MAX_REMOTE_CACHE_REQUEST_TIMEOUT_SECONDS + 1;
  assert!(
    CacheSessionManager::new(config)
      .unwrap_err()
      .to_string()
      .contains("request timeout")
  );

  let mut config = manager_config(&cache);
  config.max_parallel_transfers = octa_cache_protocol::MAX_REMOTE_CACHE_PARALLEL_TRANSFERS + 1;
  assert!(
    CacheSessionManager::new(config)
      .unwrap_err()
      .to_string()
      .contains("transfer concurrency")
  );

  let mut config = manager_config(&cache);
  config.native_identities = BTreeMap::from([("plan9-amd64".to_owned(), "toolchain-v1".to_owned())]);
  assert!(
    CacheSessionManager::new(config)
      .unwrap_err()
      .to_string()
      .contains("unsupported platform")
  );
}

#[test]
fn native_platform_keys_map_to_their_exact_octa_identity() {
  let identities = BTreeMap::from([("windows-arm64".to_owned(), "msvc-1".to_owned())]);
  let identity = runtime_identity(
    &RuntimeTarget::Native {
      platform: PlatformSpec {
        os: PlatformOs::Windows,
        architecture: PlatformArchitecture::Arm64,
      },
    },
    &identities,
  )
  .unwrap();

  assert!(matches!(
    identity,
    RuntimeIdentity::Native {
      os: octa_cache_protocol::PlatformOs::Windows,
      architecture: octa_cache_protocol::PlatformArchitecture::Arm64,
      ..
    }
  ));
}

#[tokio::test]
async fn narrows_signed_permissions_and_uses_the_remaining_job_deadline() {
  let fixture = SessionFixture::new();
  let cache = fixture.cache.clone();
  let job = fixture.job.clone();

  let mut config = manager_config(&cache);
  config.allow_write = false;
  let read_only = CacheSessionManager::new(config).unwrap();
  let grant = || BeginCacheSessionResponse {
    protocol_version: 1,
    request_id: "request".to_owned(),
    session_id: "session".to_owned(),
    scope_id: "scope".to_owned(),
    remote: Some(RemoteCacheGrant {
      endpoint: "https://cache.example/base".to_owned(),
      bearer_token: "secret-token".to_owned(),
      expires_at: 14,
    }),
  };
  assert!(
    read_only
      .prepare(
        &policy(),
        grant(),
        &runtime(),
        &NetworkPolicy::Unrestricted,
        &job,
        CacheSessionContext {
          now: 1,
          remaining_job: Duration::from_millis(1_001),
          cancellation: CancellationToken::new(),
        },
      )
      .await
      .unwrap_err()
      .to_string()
      .contains("operator-authorized")
  );

  let read_policy = CachePolicy {
    namespace: "project/main".to_owned(),
    read: true,
    write: false,
  };
  let prepared = read_only
    .prepare(
      &read_policy,
      grant(),
      &runtime(),
      &NetworkPolicy::Unrestricted,
      &job,
      CacheSessionContext {
        now: 1,
        remaining_job: Duration::from_millis(1_001),
        cancellation: CancellationToken::new(),
      },
    )
    .await
    .unwrap();
  assert_eq!(prepared.runner().mode, CacheMode::ReadOnly);
  prepared.revoke().await.unwrap();
}
