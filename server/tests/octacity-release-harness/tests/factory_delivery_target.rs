#![cfg(unix)]
//! Hermetic HTTPS forge backed by actual Git refs and durable review metadata.
#[path = "factory_pilot/delivery_target.rs"]
mod delivery_target;
use delivery_target::{ForgeResponseMode, GithubForge};
use serde_json::{Value, json};
use std::{fs, future::Future as _, path::Path, process::Command, task::Poll};

fn git(directory: &Path, arguments: &[&str]) -> String {
  let output = Command::new("git")
    .env_clear()
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .arg("-C")
    .arg(directory)
    .args([
      "-c",
      "core.hooksPath=/dev/null",
      "-c",
      "user.name=Pilot",
      "-c",
      "user.email=pilot@example.test",
    ])
    .args(arguments)
    .output()
    .unwrap();
  assert!(output.status.success(), "test-owned Git command failed");
  String::from_utf8(output.stdout).unwrap().trim().into()
}

#[tokio::test]
async fn https_review_reads_actual_git_refs_and_retains_a_review_after_an_unknown_create_response() {
  assert_review_contract(ForgeResponseMode::DropReviewResponse).await;
}

#[tokio::test]
async fn accepted_https_review_is_durable_while_its_acknowledgement_remains_held() {
  assert_review_contract(ForgeResponseMode::HoldReviewResponse).await;
}

async fn assert_review_contract(mode: ForgeResponseMode) {
  let root = tempfile::tempdir().unwrap();
  let source = root.path().join("source");
  let remote = root.path().join("remote.git");
  let state = root.path().join("forge");
  fs::create_dir(&source).unwrap();
  fs::create_dir(&state).unwrap();
  git(&source, &["init", "--quiet", "--initial-branch=main", "--template="]);
  fs::write(source.join("facts.txt"), "before\n").unwrap();
  git(&source, &["add", "facts.txt"]);
  git(&source, &["commit", "--quiet", "-m", "base"]);
  let base = git(&source, &["rev-parse", "HEAD"]);
  git(&source, &["clone", "--bare", "--quiet", ".", remote.to_str().unwrap()]);
  fs::write(source.join("facts.txt"), "actual candidate\n").unwrap();
  git(&source, &["commit", "--quiet", "--all", "-m", "candidate"]);
  let head = git(&source, &["rev-parse", "HEAD"]);
  let forge = GithubForge::start(&state, &remote, mode).await;
  let client = reqwest::Client::builder()
    .tls_certs_merge([reqwest::Certificate::from_pem(forge.root_certificate_pem().as_bytes()).unwrap()])
    .retry(reqwest::retry::never())
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .unwrap();
  let api = format!("{}repos/owner/repository", forge.endpoint());
  let get = |suffix: &str| {
    client
      .get(format!("{api}{suffix}"))
      .bearer_auth("private-delivery-token")
  };
  let observed: Value = get("/git/ref/heads/main").send().await.unwrap().json().await.unwrap();
  assert_eq!(
    observed,
    json!({"ref":"refs/heads/main","object":{"type":"commit","sha":base}})
  );
  assert_eq!(
    get("/git/ref/heads/review/candidate").send().await.unwrap().status(),
    reqwest::StatusCode::NOT_FOUND
  );
  git(
    &source,
    &[
      "push",
      "--quiet",
      remote.to_str().unwrap(),
      "HEAD:refs/heads/review/candidate",
    ],
  );
  let observed: Value = get("/git/ref/heads/review/candidate")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(observed["object"]["sha"], head);
  let review = json!({"title":"Candidate review","body":"Exact frozen review marker", "head":"review/candidate","base":"main","draft":true});
  let request = client
    .post(format!("{api}/pulls"))
    .bearer_auth("private-delivery-token")
    .json(&review)
    .send();
  tokio::pin!(request);
  if matches!(mode, ForgeResponseMode::HoldReviewResponse) {
    tokio::select! {
      _ = forge.wait_for_review() => {},
      response = &mut request => panic!("accepted response must remain held: {}", response.is_ok()),
    }
    assert!(forge.has_review(), "the accepted effect is committed before release");
    assert!(std::future::poll_fn(|cx| Poll::Ready(request.as_mut().poll(cx).is_pending())).await);
    let observation = get("/pulls").send();
    tokio::pin!(observation);
    tokio::select! {
      _ = forge.wait_for_held_observation() => {},
      response = &mut observation => panic!("observation must wait for explicit release: {}", response.is_ok()),
    }
    assert!(std::future::poll_fn(|cx| Poll::Ready(observation.as_mut().poll(cx).is_pending())).await);
    forge.release_response();
    assert!(observation.await.is_err());
  }
  assert!(request.await.is_err());
  assert_eq!(forge.review_posts(), 1);
  let reviews: Value = get("/pulls?state=all&head=owner%3Areview%2Fcandidate&base=main&per_page=100")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(reviews.as_array().unwrap().len(), 1);
  assert_eq!(reviews[0]["number"], 1);
  assert_eq!(reviews[0]["head"]["sha"], head);
  assert_eq!(reviews[0]["base"]["sha"], base);
  assert_eq!(reviews[0]["body"], review["body"]);
  assert_eq!(reviews[0]["title"], review["title"]);
  assert_eq!(reviews[0]["draft"], true);
  assert_eq!(
    forge.review_posts(),
    1,
    "observing an accepted write must stay read-only"
  );
  forge.stop().await;
  let restarted = GithubForge::start(&state, &remote, ForgeResponseMode::Respond).await;
  let client = reqwest::Client::builder()
    .tls_certs_merge([reqwest::Certificate::from_pem(restarted.root_certificate_pem().as_bytes()).unwrap()])
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .unwrap();
  let retained: Value = client
    .get(format!("{}repos/owner/repository/pulls", restarted.endpoint()))
    .bearer_auth("private-delivery-token")
    .send()
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
  assert_eq!(retained, reviews);
  assert_eq!(restarted.review_posts(), 0);
  restarted.stop().await;
}

#[tokio::test]
#[ignore = "requires the explicitly built native Github provider executable"]
async fn installed_github_process_observes_actual_https_git_absence() {
  use sha2::{Digest as _, Sha256};
  use std::os::unix::fs::PermissionsExt as _;
  use tokio::io::AsyncWriteExt as _;
  let root = tempfile::tempdir().unwrap();
  let source = root.path().join("repository");
  let adapters = root.path().join("adapters");
  fs::create_dir(&adapters).unwrap();
  let installed = adapters.join("github");
  fs::create_dir(&source).unwrap();
  fs::create_dir(&installed).unwrap();
  fs::set_permissions(&installed, fs::Permissions::from_mode(0o700)).unwrap();
  git(&source, &["init", "--quiet", "--initial-branch=main", "--template="]);
  fs::write(source.join("facts.txt"), "base").unwrap();
  git(&source, &["add", "."]);
  git(&source, &["commit", "--quiet", "-m", "base"]);
  let base = git(&source, &["rev-parse", "HEAD"]);
  let bare = root.path().join("remote.git");
  git(&source, &["clone", "--bare", "--quiet", ".", bare.to_str().unwrap()]);
  let forge = GithubForge::start(root.path(), &bare, ForgeResponseMode::Respond).await;
  for name in ["credentials", "inputs"] {
    let path = installed.join(name);
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
  }
  let system_git = std::env::var_os("OCTACITY_CONTRACT_GIT_EXECUTABLE").unwrap();
  let system_git = Path::new(&system_git).canonicalize().unwrap();
  let remote = "https://pilot.invalid/repository.git";
  let config = toml::to_string(&json!({"config_version":1,
    "api_base":forge.endpoint(),"api_version":"2022-11-28",
    "api_root_certificates_pem":[forge.root_certificate_pem()],
    "git_path":system_git,"git_sha256":hex::encode(Sha256::digest(fs::read(&system_git).unwrap())),
    "credential_directory":"credentials","input_directory":"inputs",
    "repositories":{(remote):{"repository_id":"repository-1","owner":"owner","name":"repository","remote":remote}}
  }))
  .unwrap();
  let config_path = installed.join("github-plugin.toml");
  fs::write(&config_path, &config).unwrap();
  fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
  let handle = "protected-delivery-grant";
  let credential_path = installed
    .join("credentials")
    .join(format!("{}.json", hex::encode(Sha256::digest(handle.as_bytes()))));
  fs::write(
    &credential_path,
    json!({"class":"delivery_write","token":"private-delivery-token","repositories":[remote]}).to_string(),
  )
  .unwrap();
  fs::set_permissions(&credential_path, fs::Permissions::from_mode(0o600)).unwrap();
  let mut request: Value = serde_json::from_str(include_str!(
    "../../../protocols/octacity-delivery-protocol/fixtures/publish-v1.json"
  ))
  .unwrap();
  request["command"]["operation"] = json!("observe");
  let intent = &mut request["command"]["payload"];
  intent["base_revision"] = json!(base);
  intent["repository"]["repository_locator"] = json!(remote);
  intent["network_hosts"] = json!(["localhost", "pilot.invalid"]);
  intent["configuration_digest"] = json!(hex::encode(Sha256::digest(config.as_bytes())));
  let mut child = tokio::process::Command::new(std::env::var_os("OCTACITY_FACTORY_GITHUB_PLUGIN").unwrap())
    .env_clear()
    .current_dir(&installed)
    .kill_on_drop(true)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .unwrap();
  let mut stdin = child.stdin.take().unwrap();
  stdin.write_all(format!("{request}\n").as_bytes()).await.unwrap();
  drop(stdin);
  let output = tokio::time::timeout(std::time::Duration::from_secs(12), child.wait_with_output())
    .await
    .unwrap()
    .unwrap();
  assert!(
    output.status.success(),
    "the installed provider process must return a bounded correlated response"
  );
  let response: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(response["request_id"], "request-1");
  assert_eq!(response["outcome"], json!({"status":"absent"}));
  assert_eq!(
    forge.versioned_requests(),
    2,
    "both actual reference requests carry exactly one installed API version header"
  );
  let executable = installed.join("octacity-github");
  fs::copy(std::env::var_os("OCTACITY_FACTORY_GITHUB_PLUGIN").unwrap(), &executable).unwrap();
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
  let sha = hex::encode(Sha256::digest(fs::read(&executable).unwrap()));
  fs::write(installed.join("adapter.toml"), format!(
    "manifest_version=1\nadapter_id=\"github\"\nexecutable=\"octacity-github\"\nexecutable_sha256={sha:?}\n[protocol]\nmin=1\nmax=1\n[capabilities]\nobserve=true\npublish=true\ncancel=true\nidempotency=\"observe_before_retry\"\ncredential_class=\"delivery_write\"\nconfiguration_digest={:?}\n",
    hex::encode(Sha256::digest(config.as_bytes())))).unwrap();
  let registry = octacity_server_delivery::DeliveryAdapterRegistry::discover(&adapters).unwrap();
  let hosted = registry
    .resolve("github", &sha)
    .unwrap()
    .execute(
      serde_json::from_value(request).unwrap(),
      std::time::Duration::from_secs(10),
      std::time::Duration::from_secs(1),
      tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("the real public process host must preserve the provider observation");
  assert_eq!(serde_json::to_value(hosted).unwrap(), json!({"status":"absent"}));
  assert_eq!(forge.versioned_requests(), 4);
  assert!(!forge.has_review());
  assert_eq!(forge.review_posts(), 0);
  forge.stop().await;
}
