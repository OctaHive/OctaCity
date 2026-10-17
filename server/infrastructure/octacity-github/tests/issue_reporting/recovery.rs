//! Recovery through the public provider protocol and a stateful HTTPS peer.
use super::*;
use octacity_work_source_protocol::FailureClass;
use reporting::LostResponse;

#[tokio::test]
async fn stage_effect_presets_apply_distinct_arbitrary_values_without_changing_unrelated_labels() {
  for (status, expected_labels, expected_state, expected_writes) in [
    ("ready / review", vec!["human-owned", "ready / review"], "open", 2),
    ("context requested", vec!["human-owned"], "open", 1),
    ("repair needed", vec!["human-owned", "ready / review"], "closed", 3),
  ] {
    let forge = Forge::start().await;
    *forge.reporting.lock().await = Some(reporting::Issue::new(&["needs context", "human-owned"]));
    let effects = json!({
      "config_version":1,
      "managed_label_groups":{"editorial":["ready / review","needs context"]},
      "statuses":{
        "ready / review":{"add":["ready / review"],"remove":["needs context"]},
        "context requested":{"add":[],"remove":["needs context"]},
        "repair needed":{"add":["ready / review"],"remove":["needs context"],"state":"closed"}
      }
    });
    let (installation, original) = install(&forge, effects);
    let Request::Reporter(reporter::Request {
      command: reporter::Command::Project(mut intent),
      ..
    }) = original
    else {
      unreachable!()
    };
    intent.status = status.into();
    let inspect = Request::Reporter(reporter::Request {
      protocol_version: 2,
      request_id: "stage-effects-inspect".into(),
      command: reporter::Command::Inspect(intent.clone()),
    });
    let Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Planned(mut plan),
      ..
    }) = process(&installation, &inspect).await
    else {
      panic!("expected the exact stage's pinned effect plan")
    };
    let mut dispatched = 0;
    while !plan.receipt.matches {
      assert!(dispatched < 4, "bounded convergence of the selected stage effects");
      let apply = Request::Reporter(reporter::Request {
        protocol_version: 2,
        request_id: format!("stage-effects-apply-{dispatched}"),
        command: reporter::Command::Apply(Box::new(reporter::StepRequest {
          intent: (*intent).clone(),
          step: plan.steps[0].clone(),
        })),
      });
      let Response::Reporter(reporter::Response {
        outcome: reporter::Outcome::Progressed(next),
        ..
      }) = process(&installation, &apply).await
      else {
        panic!("expected one convergent native stage effect")
      };
      dispatched += 1;
      assert_eq!(writes(&forge).await.len(), dispatched);
      assert_eq!(next.receipt.status, status);
      assert_eq!(next.receipt.mapping, intent.mapping);
      plan = next;
    }
    assert_eq!(dispatched, expected_writes);
    {
      let observed = forge.reporting.lock().await;
      let observed = observed.as_ref().unwrap();
      assert_eq!(observed.labels, expected_labels.into_iter().map(String::from).collect());
      assert_eq!(observed.state, expected_state);
    }
    // An observation/restart of this same intent confirms without another mutation.
    let Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Planned(confirmed),
      ..
    }) = process(&installation, &inspect).await
    else {
      panic!("expected read-only confirmation")
    };
    assert!(confirmed.receipt.matches);
    assert_eq!(writes(&forge).await.len(), expected_writes);
  }
}

#[tokio::test]
async fn ordered_native_reporting_exposes_opaque_fields_and_applies_one_conditional_write_at_a_time() {
  let forge = Forge::start().await;
  *forge.reporting.lock().await = Some(reporting::Issue::new(&["needs context", "human-owned"]));
  let mut effects = mapping();
  effects["statuses"]["needs-human-eyes"]["state"] = json!("closed");
  let (installation, original) = install(&forge, effects);
  let Request::Reporter(reporter::Request {
    command: reporter::Command::Project(intent),
    ..
  }) = original
  else {
    unreachable!()
  };
  let inspect = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "inspect-ordered".into(),
    command: reporter::Command::Inspect(intent.clone()),
  });
  let response = process(&installation, &inspect).await;
  response.validate_for(&inspect).unwrap();
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Planned(mut plan),
    ..
  }) = response
  else {
    panic!("expected read-only plan: {response:?}")
  };
  assert!(plan.fields.len() >= 3);
  assert_eq!(
    plan.steps[0].fields, plan.steps[1].fields,
    "one managed group shares its ownership scope"
  );
  assert!(
    plan.steps[2]
      .fields
      .iter()
      .all(|field| !plan.steps[0].fields.contains(field)),
    "Issue state is an independent managed field"
  );
  assert_eq!(plan.steps.len(), 3);
  assert_eq!(writes(&forge).await.len(), 0);
  let mut count = 0;
  while !plan.receipt.matches {
    let step = plan.steps[0].clone();
    let apply = Request::Reporter(reporter::Request {
      protocol_version: 2,
      request_id: format!("apply-{count}"),
      command: reporter::Command::Apply(Box::new(reporter::StepRequest {
        intent: (*intent).clone(),
        step,
      })),
    });
    let response = process(&installation, &apply).await;
    response.validate_for(&apply).unwrap();
    let Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Progressed(next),
      ..
    }) = response
    else {
      panic!("expected one completed step: {response:?}")
    };
    count += 1;
    assert_eq!(
      writes(&forge).await.len(),
      count,
      "at most one external write per authorization"
    );
    plan = next;
    if count == 1 {
      failure(process(&installation, &apply).await, FailureClass::Conflict);
      assert_eq!(
        writes(&forge).await.len(),
        1,
        "a consumed conditional step cannot be replayed"
      );
    }
    assert!(count <= 3);
  }
  assert_eq!(writes(&forge).await, ["DELETE", "POST", "PATCH"]);
  assert_effects(&forge, "closed").await;
}

#[tokio::test]
async fn lost_success_body_after_apply_is_observed_on_restart_without_repeating_the_write() {
  for fault in [
    LostResponse::Empty,
    LostResponse::Malformed,
    LostResponse::Disconnect,
    LostResponse::Truncated,
    LostResponse::OversizedDeclaration,
    LostResponse::OversizedBody,
  ] {
    let forge = Forge::start().await;
    let mut issue = reporting::Issue::new(&["human-owned"]);
    issue.fault = Some(("POST", fault));
    *forge.reporting.lock().await = Some(issue);
    let (installation, request) = install(&forge, mapping());
    failure(
      installation
        .provider()
        .execute(&request, CancellationToken::new())
        .await,
      FailureClass::Transient,
    );
    assert!(observed(&installation, &request).await);
    assert_eq!(writes(&forge).await, ["POST"]);
    assert_effects(&forge, "open").await;
  }
}

#[tokio::test]
async fn partial_removal_addition_and_state_changes_converge_only_missing_original_effects() {
  for verb in ["DELETE", "POST", "PATCH"] {
    for fault in [
      LostResponse::Disconnect,
      LostResponse::UnavailableBeforeApply,
      LostResponse::UnavailableAfterApply,
    ] {
      let before_apply = matches!(fault, LostResponse::UnavailableBeforeApply);
      let forge = Forge::start().await;
      let mut issue = reporting::Issue::new(&["needs context", "human-owned"]);
      issue.fault = Some((verb, fault));
      *forge.reporting.lock().await = Some(issue);
      let mut effects = mapping();
      effects["statuses"]["needs-human-eyes"]["state"] = json!("closed");
      let (installation, request) = install(&forge, effects);
      failure(
        installation
          .provider()
          .execute(&request, CancellationToken::new())
          .await,
        FailureClass::Transient,
      );
      {
        let locked = forge.reporting.lock().await;
        let issue = locked.as_ref().unwrap();
        assert_eq!(issue.labels.contains("needs context"), verb == "DELETE" && before_apply);
        assert_eq!(
          issue.labels.contains("ready / review"),
          verb == "PATCH" || (verb == "POST" && !before_apply)
        );
        assert_eq!(issue.state == "closed", verb == "PATCH" && !before_apply);
        assert!(issue.labels.contains("human-owned"));
      }
      let checkpoint = forge.requests.lock().await.len();
      // A fresh provider owns no in-memory record of the first attempt.
      let matches = observed(&installation, &request).await;
      assert_eq!(matches, verb == "PATCH" && !before_apply);
      assert!(
        forge.requests.lock().await[checkpoint..]
          .iter()
          .all(|r| r.starts_with("GET "))
      );
      if !matches {
        let response = installation
          .provider()
          .execute(&request, CancellationToken::new())
          .await;
        response.validate_for(&request).unwrap();
        assert!(
          matches!(
            response,
            Response::Reporter(reporter::Response {
              outcome: reporter::Outcome::Applied(_),
              ..
            })
          ),
          "{response:?}"
        );
      }
      let expected = if before_apply {
        match verb {
          "DELETE" => vec!["DELETE", "DELETE", "POST", "PATCH"],
          "POST" => vec!["DELETE", "POST", "POST", "PATCH"],
          _ => vec!["DELETE", "POST", "PATCH", "PATCH"],
        }
      } else {
        vec!["DELETE", "POST", "PATCH"]
      };
      assert_eq!(writes(&forge).await, expected, "{verb}, before apply: {before_apply}");
      assert_effects(&forge, "closed").await;
      assert!(observed(&installation, &request).await);
      assert_eq!(writes(&forge).await, expected);
    }
  }
}

#[tokio::test]
async fn native_process_restart_observes_partial_https_writes_before_finishing_the_original_projection() {
  let forge = Forge::start().await;
  let mut issue = reporting::Issue::new(&["needs context", "human-owned"]);
  issue.fault = Some(("POST", LostResponse::Disconnect));
  *forge.reporting.lock().await = Some(issue);
  let mut effects = mapping();
  effects["statuses"]["needs-human-eyes"]["state"] = json!("closed");
  let (installation, request) = install(&forge, effects);
  failure(process(&installation, &request).await, FailureClass::Transient);
  assert_eq!(writes(&forge).await, ["DELETE", "POST"]);
  let checkpoint = forge.requests.lock().await.len();
  let observation = observe(request.clone());
  let response = process(&installation, &observation).await;
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Observed(receipt),
    ..
  }) = response
  else {
    panic!("expected partial observation: {response:?}");
  };
  assert!(!receipt.matches);
  assert!(
    forge.requests.lock().await[checkpoint..]
      .iter()
      .all(|r| r.starts_with("GET "))
  );
  let response = process(&installation, &request).await;
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Applied(applied),
    ..
  }) = response
  else {
    panic!("expected recovered projection: {response:?}");
  };
  assert!(applied.matches);
  assert_eq!(applied.projection_id, receipt.projection_id);
  assert_eq!(applied.mapping, receipt.mapping);
  assert_eq!(applied.status, receipt.status);
  assert_eq!(writes(&forge).await, ["DELETE", "POST", "PATCH"]);
  assert_effects(&forge, "closed").await;
}

#[tokio::test]
async fn lost_final_confirmation_after_all_writes_allows_read_only_recovery() {
  let forge = Forge::start().await;
  let mut issue = reporting::Issue::new(&["needs context", "human-owned"]);
  issue.confirmation = Some(Reply {
    body: vec![],
    ..Reply::json(Value::Null)
  });
  *forge.reporting.lock().await = Some(issue);
  let mut effects = mapping();
  effects["statuses"]["needs-human-eyes"]["state"] = json!("closed");
  let (installation, request) = install(&forge, effects);
  failure(
    installation
      .provider()
      .execute(&request, CancellationToken::new())
      .await,
    FailureClass::Transient,
  );
  assert_effects(&forge, "closed").await;
  assert_eq!(writes(&forge).await, ["DELETE", "POST", "PATCH"]);
  let checkpoint = forge.requests.lock().await.len();
  assert!(observed(&installation, &request).await);
  assert!(
    forge.requests.lock().await[checkpoint..]
      .iter()
      .all(|r| r.starts_with("GET "))
  );
  assert_eq!(writes(&forge).await, ["DELETE", "POST", "PATCH"]);
}

async fn observed(installation: &Installation, original: &Request) -> bool {
  let request = observe(original.clone());
  let response = installation
    .provider()
    .execute(&request, CancellationToken::new())
    .await;
  response.validate_for(&request).unwrap();
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Observed(receipt),
    ..
  }) = response
  else {
    panic!("expected recovery observation: {response:?}");
  };
  let Request::Reporter(reporter::Request {
    command: reporter::Command::Observe(intent),
    ..
  }) = request
  else {
    unreachable!()
  };
  assert_eq!(receipt.projection_id, intent.projection_id);
  assert_eq!(receipt.mapping, intent.mapping);
  assert_eq!(receipt.status, intent.status);
  receipt.matches
}
async fn writes(forge: &Forge) -> Vec<String> {
  forge
    .requests
    .lock()
    .await
    .iter()
    .filter(|r| !r.starts_with("GET "))
    .map(|r| r.split(' ').next().unwrap().into())
    .collect()
}
async fn assert_effects(forge: &Forge, state: &str) {
  let locked = forge.reporting.lock().await;
  let issue = locked.as_ref().unwrap();
  assert_eq!(
    issue.labels,
    std::collections::BTreeSet::from(["human-owned".into(), "ready / review".into()])
  );
  assert_eq!(issue.state, state);
}
async fn process(installation: &Installation, request: &Request) -> Response {
  use tokio::io::AsyncWriteExt as _;
  let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_octacity-github"))
    .current_dir(installation.root.path())
    .env_clear()
    .kill_on_drop(true)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()
    .unwrap();
  let mut bytes = serde_json::to_vec(request).unwrap();
  bytes.push(b'\n');
  child.stdin.take().unwrap().write_all(&bytes).await.unwrap();
  let output = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait_with_output())
    .await
    .unwrap()
    .unwrap();
  assert!(output.status.success());
  assert!(output.stderr.is_empty());
  octacity_work_source_protocol::decode_response(&output.stdout, request).unwrap()
}

#[tokio::test]
async fn conditional_steps_preserve_unrelated_human_labels_and_reject_changed_managed_fields() {
  let forge = Forge::start().await;
  *forge.reporting.lock().await = Some(reporting::Issue::new(&["needs context", "human-owned"]));
  let (installation, request) = install(&forge, mapping());
  let Request::Reporter(reporter::Request {
    command: reporter::Command::Project(intent),
    ..
  }) = request
  else {
    unreachable!()
  };
  let inspect = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "human-inspect".into(),
    command: reporter::Command::Inspect(intent.clone()),
  });
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Planned(plan),
    ..
  }) = installation
    .provider()
    .execute(&inspect, CancellationToken::new())
    .await
  else {
    panic!("expected inspection")
  };
  forge
    .reporting
    .lock()
    .await
    .as_mut()
    .unwrap()
    .labels
    .insert("another-human-label".into());
  let apply = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "human-apply".into(),
    command: reporter::Command::Apply(Box::new(reporter::StepRequest {
      intent: (*intent).clone(),
      step: plan.steps[0].clone(),
    })),
  });
  let response = installation.provider().execute(&apply, CancellationToken::new()).await;
  response.validate_for(&apply).unwrap();
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Progressed(plan),
    ..
  }) = response
  else {
    panic!("unrelated labels must not invalidate a managed-field check: {response:?}")
  };
  assert_eq!(writes(&forge).await, ["DELETE"]);
  forge
    .reporting
    .lock()
    .await
    .as_mut()
    .unwrap()
    .labels
    .insert("needs context".into());
  let apply = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "human-conflict".into(),
    command: reporter::Command::Apply(Box::new(reporter::StepRequest {
      intent: (*intent).clone(),
      step: plan.steps[0].clone(),
    })),
  });
  failure(
    installation.provider().execute(&apply, CancellationToken::new()).await,
    FailureClass::Conflict,
  );
  assert_eq!(writes(&forge).await, ["DELETE"]);
  let locked = forge.reporting.lock().await;
  let labels = &locked.as_ref().unwrap().labels;
  assert!(labels.contains("human-owned") && labels.contains("another-human-label") && labels.contains("needs context"));
}

async fn ordered_plan(installation: &Installation, request: Request) -> reporter::ProjectionPlan {
  let Request::Reporter(reporter::Request {
    command: reporter::Command::Project(intent),
    ..
  }) = request
  else {
    unreachable!()
  };
  let inspect = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "identity-inspection".into(),
    command: reporter::Command::Inspect(intent),
  });
  let response = installation
    .provider()
    .execute(&inspect, CancellationToken::new())
    .await;
  response.validate_for(&inspect).unwrap();
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Planned(plan),
    ..
  }) = response
  else {
    panic!("expected ordered native plan: {response:?}")
  };
  plan
}
#[tokio::test]
async fn managed_field_identity_overlaps_after_native_label_rename_or_name_recreation() {
  let forge = Forge::start().await;
  *forge.reporting.lock().await = Some(reporting::Issue::new(&["human-owned"]));
  let effects = |name: &str| json!({"config_version":1,"managed_label_groups":{"custom":[name]},"statuses":{"needs-human-eyes":{"add":[name],"remove":[]}}});
  let (first, request) = install(&forge, effects("ready / review"));
  let original = ordered_plan(&first, request).await;
  {
    let mut locked = forge.reporting.lock().await;
    let catalog = &mut locked.as_mut().unwrap().catalog;
    catalog.remove("ready / review");
    catalog.insert("needs context".into(), 101);
  }
  let (renamed, request) = install(&forge, effects("needs context"));
  let renamed = ordered_plan(&renamed, request).await;
  assert!(
    original.fields.iter().any(|field| renamed.fields.contains(field)),
    "renaming a label must not make an in-flight write look independent of that same native label"
  );
  forge
    .reporting
    .lock()
    .await
    .as_mut()
    .unwrap()
    .catalog
    .insert("ready / review".into(), 999);
  let (recreated, request) = install(&forge, effects("ready / review"));
  let recreated = ordered_plan(&recreated, request).await;
  assert!(
    original.fields.iter().any(|field| recreated.fields.contains(field)),
    "a late name-addressed request may resolve a recreated label and still needs the same alias fence"
  );
  assert!(forge.requests.lock().await.iter().all(|r| r.starts_with("GET ")));
}
#[tokio::test]
async fn ordered_scope_supports_all_valid_managed_labels_with_native_and_alias_identity() {
  let forge = Forge::start().await;
  let mut issue = reporting::Issue::new(&[]);
  let labels: Vec<_> = (0..128).map(|i| format!("label-{i}")).collect();
  issue.catalog = labels
    .iter()
    .enumerate()
    .map(|(i, name)| (name.clone(), 1000 + i as u64))
    .collect();
  *forge.reporting.lock().await = Some(issue);
  let effects = json!({"config_version":1,"managed_label_groups":{"one":labels[..64],"two":labels[64..]},"statuses":{"needs-human-eyes":{"add":["label-0","label-64"],"remove":[],"state":"closed"}}});
  let (installation, request) = install(&forge, effects);
  let plan = ordered_plan(&installation, request).await;
  assert_eq!(
    plan.fields.len(),
    257,
    "128 labels each need native and name identity, plus the Issue state"
  );
  assert_eq!(plan.steps.len(), 3);
  assert!(forge.requests.lock().await.iter().all(|r| r.starts_with("GET ")));
}

#[tokio::test]
async fn managed_fields_namespace_distinct_provider_instances_with_coincident_native_ids() {
  let first = Forge::start().await;
  let second = Forge::start().await;
  let mut scopes = Vec::new();
  for forge in [&first, &second] {
    *forge.reporting.lock().await = Some(reporting::Issue::new(&["needs context"]));
    let (installation, original) = install(forge, mapping());
    let Request::Reporter(reporter::Request {
      command: reporter::Command::Project(intent),
      ..
    }) = original
    else {
      unreachable!()
    };
    let inspect = Request::Reporter(reporter::Request {
      protocol_version: 2,
      request_id: "instance-scope".into(),
      command: reporter::Command::Inspect(intent),
    });
    let Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Planned(plan),
      ..
    }) = process(&installation, &inspect).await
    else {
      panic!("expected native managed identities")
    };
    assert!(!plan.fields.is_empty());
    scopes.push(plan.fields);
  }
  assert!(
    scopes[0].iter().all(|field| !scopes[1].contains(field)),
    "equal label/repository/Issue ids on different API instances are independent managed fields"
  );
}

#[tokio::test]
async fn a_single_change_in_a_large_managed_group_has_bounded_native_request_cost() {
  let forge = Forge::start().await;
  let mut issue = reporting::Issue::new(&["human-owned"]);
  let mut group = vec!["ready / review".to_string(), "needs context".to_string()];
  for index in 0..62 {
    let name = format!("managed-{index}");
    issue.catalog.insert(name.clone(), 1000 + index);
    group.push(name);
  }
  *forge.reporting.lock().await = Some(issue);
  let effects = json!({"config_version":1,"managed_label_groups":{"editorial":group},
    "statuses":{"needs-human-eyes":{"add":["ready / review"],"remove":[]}}});
  let (installation, original) = install(&forge, effects);
  let Request::Reporter(reporter::Request {
    command: reporter::Command::Project(intent),
    ..
  }) = original
  else {
    unreachable!()
  };
  let inspect = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "bounded-catalog".into(),
    command: reporter::Command::Inspect(intent.clone()),
  });
  // Metadata discovery and the independently fenced observation are both read-only.
  let mut step = None;
  for _ in 0..2 {
    let Response::Reporter(reporter::Response {
      outcome: reporter::Outcome::Planned(plan),
      ..
    }) = process(&installation, &inspect).await
    else {
      panic!("expected plan")
    };
    assert_eq!(plan.steps.len(), 1);
    step = plan.steps.into_iter().next();
  }
  let requests_before = forge.requests.lock().await.len();
  assert!(
    requests_before <= 8,
    "two read-only plans should use at most eight HTTP requests, got {requests_before}"
  );
  let apply = Request::Reporter(reporter::Request {
    protocol_version: 2,
    request_id: "bounded-addition".into(),
    command: reporter::Command::Apply(Box::new(reporter::StepRequest {
      intent: (*intent).clone(),
      step: step.unwrap(),
    })),
  });
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Progressed(plan),
    ..
  }) = process(&installation, &apply).await
  else {
    panic!("expected one native addition")
  };
  assert!(plan.receipt.matches);
  let Response::Reporter(reporter::Response {
    outcome: reporter::Outcome::Planned(plan),
    ..
  }) = process(&installation, &inspect).await
  else {
    panic!("expected final observation")
  };
  assert!(plan.receipt.matches);
  assert_eq!(writes(&forge).await.len(), 1);
  let count = forge.requests.lock().await.len();
  assert!(
    count <= 20,
    "full single-write cycle has a constant catalogue request budget, got {count}"
  );
  assert!(
    forge
      .reporting
      .lock()
      .await
      .as_ref()
      .unwrap()
      .labels
      .contains("human-owned")
  );
}
