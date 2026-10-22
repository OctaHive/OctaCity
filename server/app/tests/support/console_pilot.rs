//! One launcher for real HTTP browser qualification; no fixture or local task authority.
pub async fn run(origin: &str, issue: Option<&str>) -> std::process::Output {
  run_named(
    origin,
    issue,
    if issue.is_some() {
      "original executions"
    } else {
      "manual task"
    },
  )
  .await
}
pub async fn run_named(origin: &str, issue: Option<&str>, name: &str) -> std::process::Output {
  let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui");
  let mut command = tokio::process::Command::new("pnpm");
  command
    .current_dir(ui)
    .args([
      "exec",
      "playwright",
      "test",
      "tests/browser/postgres-issue-pilot.spec.ts",
      "--workers=1",
    ])
    .env("OCTACITY_PILOT_API_ORIGIN", origin);
  if let Some(issue) = issue {
    command.env("OCTACITY_PILOT_ISSUE_PATH", issue);
  } else {
    command.env_remove("OCTACITY_PILOT_ISSUE_PATH");
  }
  command.args(["--grep", name]);
  command.output().await.expect("run locked Console browser dependencies")
}
