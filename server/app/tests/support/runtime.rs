//! Literal runtime resources for released PostgreSQL/HTTP tests.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_server::ServerConfig;
use std::path::Path;

pub fn runtime_config(directory: &Path, postgres_url: &str, object_endpoint: &str) -> ServerConfig {
  ServerConfig::parse_toml(&runtime_config_source(directory, postgres_url, object_endpoint, "")).unwrap()
}

pub fn runtime_config_source(directory: &Path, postgres_url: &str, object_endpoint: &str, extra: &str) -> String {
  let postgres_url_file = directory.join("postgres-url");
  let access_key = directory.join("object-access-key");
  let secret_key = directory.join("object-secret-key");
  let signing_key = directory.join("signing-key");
  let enrollment_key = directory.join("agent-enrollment-key");
  let cache_key = directory.join("cache-credential-key");
  let job_spec_policy = directory.join("job-spec-policy.json");
  std::fs::write(&postgres_url_file, postgres_url).unwrap();
  std::fs::write(&access_key, "octacity").unwrap();
  std::fs::write(&secret_key, "octacity-secret").unwrap();
  std::fs::write(&signing_key, STANDARD.encode([7_u8; 32])).unwrap();
  std::fs::write(&enrollment_key, STANDARD.encode([8_u8; 32])).unwrap();
  std::fs::write(&cache_key, STANDARD.encode([9_u8; 32])).unwrap();
  std::fs::write(
    &job_spec_policy,
    r#"{"source":{"provider":"git","plugin_version":"0.1.0","plugin_sha256":"3333333333333333333333333333333333333333333333333333333333333333","repository_parameter":"url"},"octa":{"version":"0.3.0","runner_sha256":"1111111111111111111111111111111111111111111111111111111111111111","runner_protocol":1,"event_schema":1,"plugin_protocol":1,"plugin_digests":{"shell":"2222222222222222222222222222222222222222222222222222222222222222"}},"validity":3600}"#,
  )
  .unwrap();
  #[cfg(unix)]
  for path in [
    &postgres_url_file,
    &access_key,
    &secret_key,
    &signing_key,
    &enrollment_key,
    &cache_key,
  ] {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
  }
  let path = |path: &Path| path.display().to_string().replace('\\', "\\\\");
  format!(
    r#"
management_bind = "127.0.0.1:0"
agent_bind = "127.0.0.1:0"
shutdown_grace_milliseconds = 3000
readiness_check_interval_milliseconds = 50
readiness_check_timeout_milliseconds = 1000
agent_registration_lifetime_milliseconds = 600000
agent_enrollment_lifetime_milliseconds = 600000
agent_lease_lifetime_milliseconds = 60000
supported_pipeline_capabilities = ["native"]

[postgres]
url_file = "{}"

[object_storage]
endpoint = "{object_endpoint}"
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = "{}"
secret_key_file = "{}"

[signing]
key_id = "test-key"
key_file = "{}"

[agent_credentials]
enrollment_key_file = "{}"

[cache]
endpoint = "https://cache.example"
credential_key_file = "{}"
session_lifetime_milliseconds = 300000

[job_spec]
policy_file = "{}"
{extra}
"#,
    path(&postgres_url_file),
    path(&access_key),
    path(&secret_key),
    path(&signing_key),
    path(&enrollment_key),
    path(&cache_key),
    path(&job_spec_policy),
  )
}
