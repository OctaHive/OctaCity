//! Qualify the released requirements branch with the same frozen Issue fields.
use super::*;
use crate::factory::execution::tests::factory_requirements_tests::{
  claim_run, ticket_delivery::issue_requirements_fixture,
};

#[test]
fn issue_delivery_large_requirements_retain_specifications_and_choice_based_review() {
  run_ready(async {
    for (component, priority, decision) in [
      ("api", "high", Some(FlowHumanDisposition::Resume)),
      ("api", "high", Some(FlowHumanDisposition::Abort)),
      ("compiler", "high", None),
      ("api", "low", None),
    ] {
      let (f, issues, issue, published) = admitted_issue(
        "feature_request",
        IssueSize::Large,
        component,
        priority,
        "not_applicable",
      )
      .await;
      let original_run = published.association.unwrap().run_id;
      let original = f.store.factory_run_snapshot(original_run).await.unwrap();
      change_issue(issues.as_ref(), &issue).await;
      let selected = advance_gates(&f, original_run, 40).await;
      assert_eq!(next_node(&selected), key("specification_author"));
      assert_eq!(selected.flow.data.incoming, original.flow.data.incoming);
      // The common owner fixture supplies independently qualified capture outputs.
      // This isolates the released requirements branch; the outer test above
      // retains the actual admitted snapshot and its incoming binding unchanged.
      let (admission, execution, registry) = issue_requirements_fixture(&selected);
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      store.seed_project(f.project).unwrap();
      let audit = testing::management_mutation_with_request((), "issue-requirements")
        .audit()
        .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([53; 32]), &audit)
        .unwrap();
      let run = admission.run.id();
      let mut claim = claim_run(store.as_ref(), run, 10).await;
      let owner = FactoryFlowRunner::new(
        store.clone(),
        registry,
        Arc::new(crate::factory_flow_tests::Inputs),
        crate::factory_node_test_support::clock(),
      );
      let mut reviewed = false;
      let mut terminal = None;
      for tick in 31..100 {
        execution.clock.store(tick, std::sync::atomic::Ordering::SeqCst);
        let step = crate::factory_node_test_support::reconcile_admission_then_observe(
          &owner,
          store.as_ref(),
          run,
          claim.id,
          Timestamp::from_unix_millis(tick).unwrap(),
        )
        .await
        .unwrap();
        let state = store.factory_run_snapshot(run).await.unwrap();
        if step == FactoryFlowStep::Waiting
          && !reviewed
          && state
            .flow
            .attempts
            .iter()
            .any(|a| a.node_key() == &key("requirements_review"))
        {
          let decision = decision.expect("unmatched frozen fields cannot request review");
          let input = state
            .flow
            .data
            .inputs
            .iter()
            .find(|i| i.node() == &key("requirements_review"))
            .unwrap();
          assert_eq!(input.payload().value()["priority"], priority);
          assert_eq!(input.payload().value()["component"], component);
          assert_eq!(input.payload().value()["size"], "large");
          assert!(input.payload().value().get("specification").is_some());
          review_requirements(store.clone(), &state, decision, tick).await;
          claim = claim_run(store.as_ref(), run, tick).await;
          reviewed = true;
        }
        if let FactoryFlowStep::Resolved(value) = step {
          terminal = Some(value);
          break;
        }
      }
      assert_eq!(reviewed, decision.is_some());
      assert_eq!(
        terminal,
        Some(key(if decision == Some(FlowHumanDisposition::Abort) {
          "needs_correction"
        } else {
          "ready"
        }))
      );
      let after = store.factory_run_snapshot(run).await.unwrap();
      let accepted_spec = after
        .flow
        .data
        .records
        .iter()
        .find(|r| r.input().node() == &key("specification_accepted"))
        .unwrap();
      let summary = &accepted_spec.payload().value()["summary"];
      assert_eq!(summary["priority"], priority);
      assert_eq!(summary["component"], component);
      assert_eq!(summary["size"], "large");
      let paths = summary["specification"]["manifest"]["changed_paths"]
        .as_array()
        .unwrap();
      for path in [
        "openspec/changes/api/proposal.md",
        "openspec/changes/api/design.md",
        "openspec/changes/api/tasks.md",
        "openspec/changes/api/specs/api/spec.md",
      ] {
        assert!(paths.iter().any(|p| p["path"] == path), "missing retained {path}");
      }
      assert_eq!(
        after.flow.data.build_intents.len(),
        1,
        "one isolated specification invocation precedes tests"
      );
      if decision != Some(FlowHumanDisposition::Abort) {
        let accepted = after
          .flow
          .data
          .records
          .iter()
          .find(|r| {
            matches!(r.outcome().as_str(), "automatic" | "accepted")
              && matches!(
                r.input().node().as_str(),
                "requirements_review_policy" | "requirements_approved"
              )
          })
          .unwrap();
        let contract = AcceptedWorkContract::freeze(
          &after.work,
          &after.admitted_flow,
          &after.flow.data,
          after.flow.runtime(),
          accepted.node_attempt_id(),
        )
        .unwrap();
        assert_eq!(contract.work(), &original.work);
        assert_eq!(accepted.payload().value(), summary);
      }
      assert_eq!(
        f.store.factory_run_snapshot(original_run).await.unwrap(),
        selected,
        "branch qualification cannot mutate the admitted Run"
      );
    }
  });
}
