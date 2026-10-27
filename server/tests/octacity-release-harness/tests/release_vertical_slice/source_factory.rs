//! Packaged Factory admission shares the normal Source installation and public management API.
use super::*;

pub(super) async fn run() {
  let mut environment = source_environment::SourceEnvironment::start_with_factory().await;
  let inventory = get_json(
    &environment.client,
    format!("{}/api/v1/integration-installations", environment.origin),
  )
  .await;
  let secondary = inventory["items"][0]["resources"]
    .as_array()
    .unwrap()
    .iter()
    .find(|r| r["native_id"] == "5678")
    .expect("the packaged installation must advertise a second independently grantable native resource")
    .clone();
  let project = source_setup::enroll_writer(&environment.client, &environment.origin, &environment.installation).await;
  let second_project = resource_id(
    &post_management(
      &environment.client,
      &environment.origin,
      "/api/v1/projects",
      &Uuid::new_v4().to_string(),
      json!({"name":format!("Research {}",Uuid::new_v4())}),
    )
    .await,
  );
  let import_checkpoint = source_checkpoint::Checkpoint::import(&project).await;
  let result = environment
    .browser_phase("factory-source-prepare.spec.ts", &project)
    .await;
  if result.is_ok() {
    let issues = get_json(
      &environment.client,
      format!("{}/api/v1/projects/{project}/issues", environment.origin),
    )
    .await;
    let issue = issues["items"][0]["id"].as_str().unwrap();
    let sync = get_json(
      &environment.client,
      format!(
        "{}/api/v1/projects/{project}/issues/{issue}/synchronization",
        environment.origin
      ),
    )
    .await;
    let connection = sync["link"]["selection"]["connection"]["id"].as_str().unwrap();
    let choices = get_json(
      &environment.client,
      format!(
        "{}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1",
        environment.origin
      ),
    )
    .await;
    assert!(
      choices["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["purpose"] == "task_read"),
      "a separately installed preparation must be available for reviewed execution: {choices}"
    );
    import_checkpoint.wait().await;
    import_checkpoint.release().await;
    environment.restart().await;
    let after_restart = get_json(
      &environment.client,
      format!("{}/api/v1/projects/{project}/issues", environment.origin),
    )
    .await;
    assert_eq!(
      after_restart["items"], issues["items"],
      "import restart retains one original canonical card"
    );
    environment
      .install_factory(&source_factory_configuration::catalog(&[
        project.clone(),
        second_project.clone(),
      ]))
      .await;
    let (target, repository) = source_factory_configuration::publish(&environment, &project, None).await;
    let source = choices["items"]
      .as_array()
      .unwrap()
      .iter()
      .find(|c| c["purpose"] == "task_read")
      .unwrap();
    let mut works = Vec::new();
    for index in 0..2 {
      let id = Uuid::new_v4();
      let path = format!("/api/v1/projects/{project}/factory-source-inputs/{id}/versions");
      let mut intent = json!({"project_id":project,"group":null,"inputs":[{"id":id,"expected_version":null,
        "definition":{"target":target,"repository":repository,"source":source["binding"],"enabled":true,"reporter":null,
          "connection":{"id":connection,"version":1,"access_version":1,
            "source_configuration":source["connection_configuration"],"reporter_configuration":null}}}]});
      let checkpoint = if index == 0 {
        Some(source_checkpoint::Checkpoint::admission(&project).await)
      } else {
        None
      };
      source_setup::publish(
        &environment.client,
        &environment.origin,
        &path,
        &id.to_string(),
        "if-none-match",
        "*",
        json!({"intent":intent}),
      )
      .await;
      let diagnostics_path = format!("{}{path}/1/diagnostics", environment.origin);
      let deadline = Instant::now() + Duration::from_secs(20);
      let admitted = loop {
        let diagnostics = get_json(&environment.client, diagnostics_path.clone()).await;
        if diagnostics["admissions"].as_array().unwrap().len() == 1
          && diagnostics["admissions"][0]["issue"]["issue_id"] == issue
        {
          break diagnostics;
        }
        assert!(
          Instant::now() < deadline,
          "enabled input must associate its original Work with the imported Issue: {diagnostics}"
        );
        sleep(Duration::from_millis(100)).await;
      };
      let association = &admitted["admissions"][0]["association"];
      let run = association["run_id"].as_str().unwrap();
      let work_path = format!("{}/api/v1/factory-runs/{run}/work", environment.origin);
      let original_work = get_json(&environment.client, work_path.clone()).await;
      let work = original_work["items"][0]["record"].clone();
      assert_eq!(work["subject"]["repository_id"], repository["id"]);
      assert_eq!(work["subject"]["base_revision"], "a".repeat(40));
      assert_eq!(
        admitted["admissions"][0]["reporting"],
        json!([]),
        "a read-only Factory input creates no Reporter producer"
      );
      if let Some(checkpoint) = checkpoint {
        checkpoint.wait().await;
        let mut changed = source_provider::issue(9001, 17);
        changed["body"] =
          json!("Later native requirements\n\n## Acceptance\nLater acceptance must not replace an admitted snapshot.");
        let mut replies = environment.provider.replies.lock().await;
        replies.clear();
        for _ in 0..128 {
          replies.push_back(source_provider::Reply::json(json!([changed.clone()])));
        }
        drop(replies);
        checkpoint.release().await;
        environment.restart().await;
        let retained = get_json(&environment.client, diagnostics_path.clone()).await;
        assert_eq!(retained["admissions"][0]["association"], *association);
        assert_eq!(
          get_json(&environment.client, work_path).await,
          original_work,
          "restart/native edits cannot replace immutable Work inputs"
        );
      }
      works.push(work);
      intent["inputs"][0]["expected_version"] = json!(1);
      intent["inputs"][0]["definition"]["enabled"] = json!(false);
      source_setup::publish(
        &environment.client,
        &environment.origin,
        &path,
        &format!("{id}-disable"),
        "if-match",
        "\"1\"",
        json!({"intent":intent}),
      )
      .await;
      let disabled = get_json(
        &environment.client,
        format!("{}{path}/2/diagnostics", environment.origin),
      )
      .await;
      assert_eq!(
        disabled["admissions"],
        json!([]),
        "a disabled new version cannot borrow the previous version's Work"
      );
      assert_eq!(
        get_json(&environment.client, diagnostics_path).await["admissions"]
          .as_array()
          .unwrap()
          .len(),
        1
      );
    }
    assert_ne!(
      works[0]["id"], works[1]["id"],
      "distinct inputs require distinct Work identities"
    );
    assert_ne!(
      works[0]["external_identity"], works[1]["external_identity"],
      "per-input identity remains distinct for one native task"
    );
    assert!(
      environment
        .provider
        .requests
        .lock()
        .await
        .iter()
        .all(|r| r.starts_with("GET ")),
      "import/admission alone cannot authorize provider effects"
    );
    let second = source_factory_second::run(&environment, &second_project, &secondary).await;
    assert_eq!(
      get_json(
        &environment.client,
        format!("{}/api/v1/projects/{project}/issues", environment.origin)
      )
      .await["items"]
        .as_array()
        .unwrap()
        .len(),
      1
    );
    fs::write(environment.evidence.join("factory-observations.json"),serde_json::to_vec_pretty(&json!({
      "project_id":project,"canonical_issue":issue,"source_works":works,"second":second,
      "execution_boundary":"real Factory human gate and explicitly configured local Issue action; no model/Agent Build"
    })).unwrap()).unwrap();
    let workflow = get_json(
      &environment.client,
      format!(
        "{}/api/v1/projects/{project}/issue-workflow-default",
        environment.origin
      ),
    )
    .await;
    let workflow = &workflow["workflow"];
    let (action, _) = source_factory_configuration::publish(
      &environment,
      &project,
      Some(json!({
        "workflow_id":workflow["id"],"workflow_version":1,
        "transition_id":workflow["definition"]["transitions"][0]["id"],"outcome":"approved"
      })),
    )
    .await;
    source_factory_effect::run(&mut environment, &project, action["id"].as_str().unwrap()).await;
    source_revocation::run(&environment, &project, connection, &target, &repository).await;
    environment.stop().await;
    environment.record(&project, "packaged-factory.json").await;
    return;
  }
  environment.stop().await;
  result.unwrap();
}
