use std::{collections::BTreeMap, path::Path};

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

fn lifetime() -> CacheSessionLifetime {
  CacheSessionLifetime {
    now: 1,
    remaining_job: Duration::from_secs(20),
  }
}

#[tokio::test]
async fn prepares_persistent_l1_and_revokes_private_bearer() {
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  let job = temporary.path().join("job");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  octacity_private_fs::create_private_directory(&job).unwrap();
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
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  let job = temporary.path().join("job");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  octacity_private_fs::create_private_directory(&job).unwrap();
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
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  let job = temporary.path().join("job");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  octacity_private_fs::create_private_directory(&job).unwrap();
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
      .prepare(&policy(), grant(), &runtime(), &denied, &job, lifetime())
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
async fn bounds_the_number_of_persistent_trust_scopes() {
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  let job = temporary.path().join("job");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  octacity_private_fs::create_private_directory(&job).unwrap();
  let manager = manager(&cache);
  let grant = |scope: &str| BeginCacheSessionResponse {
    protocol_version: 1,
    request_id: "request".to_owned(),
    session_id: format!("session-{scope}"),
    scope_id: scope.to_owned(),
    remote: None,
  };

  for scope in ["scope-a", "scope-b"] {
    manager
      .prepare(
        &policy(),
        grant(scope),
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
  assert!(
    manager
      .prepare(
        &policy(),
        grant("scope-c"),
        &runtime(),
        &NetworkPolicy::Disabled,
        &job,
        lifetime(),
      )
      .await
      .unwrap_err()
      .to_string()
      .contains("maximum of 2")
  );
  assert_eq!(std::fs::read_dir(cache.join("v1")).unwrap().count(), 2);
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
      .contains("unsupported Native")
  );
}

#[tokio::test]
async fn narrows_signed_permissions_and_uses_the_remaining_job_deadline() {
  let temporary = fixture_root();
  let cache = temporary.path().join("cache");
  let job = temporary.path().join("job");
  octacity_private_fs::create_private_directory(&cache).unwrap();
  octacity_private_fs::create_private_directory(&job).unwrap();

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
        CacheSessionLifetime {
          now: 1,
          remaining_job: Duration::from_millis(1_001),
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
      CacheSessionLifetime {
        now: 1,
        remaining_job: Duration::from_millis(1_001),
      },
    )
    .await
    .unwrap();
  assert_eq!(prepared.runner().mode, CacheMode::ReadOnly);
  prepared.revoke().await.unwrap();
}
