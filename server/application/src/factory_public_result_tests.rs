//! Ordinary authorized diagnostic reads disclose only immutable opted-in outcomes.
use super::*;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::testing::*;
use serde_json::json;

#[test]
fn retained_public_results_read_classification_without_mutation_or_private_context() {
  crate::factory_admission_tests::run_ready(async {
    let template = crate::factory_node_test_support::fixtures::supplied_triage_template().unwrap();
    let schema = template
      .data_schemas()
      .unwrap()
      .remove(&"summary".parse().unwrap())
      .unwrap();
    let assessment = json!({"priority_assessment":75,"work_kind":"feature_request","severity":"advisory",
      "size":"small","component":"server","risk":"low","dependencies":[],"recommended_route":"requirements","reproducibility":"not_applicable"});
    for public in [true, false] {
      let mut fixture = factory_public_result_journal_fixture(schema.clone(), public);
      let value = json!({"assessment":assessment,"admission_priority":fixture.admission.work.priority(),
        "admission_risk":fixture.admission.work.risk(),"accepted":true,"evidence":"absent"});
      fixture.observation = Some(value.clone());
      let store = Arc::new(InMemoryFactoryConfigurationStore::new());
      store
        .seed_factory_admission(
          fixture.admission.clone(),
          octacity_server_store::FactoryWorkSourceScope {
            source: "manual".parse().unwrap(),
            security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
          },
          FactoryDigest::from_bytes([1; 32]),
          management_mutation_with_request((), "public-result-admission").audit(),
        )
        .unwrap();
      verify_factory_node_journal_contract(&*store, &*store, &fixture).await;
      let before = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      let mutations = store.mutation_evidence_counts().await;
      let queries = crate::AuthorizedQueryHandler::new(
        Arc::new(crate::TrustedNetworkManagementPolicy),
        Arc::new(crate::FactoryDiscoveryHandlers::new(store.clone())),
      );
      for _ in 0..2 {
        let page = queries
          .handle_query(
            &crate::factory_admission_tests::context(),
            ListFactoryDiagnosticsQuery::try_new(
              fixture.admission.run.id(),
              FactoryDiagnosticCollection::NodeCompletions,
              None,
              1,
            )
            .unwrap(),
          )
          .await
          .unwrap();
        let wire = serde_json::to_value(page).unwrap();
        let row = &wire["items"][0]["record"];
        assert_eq!(row["output_schema"], json!(schema.reference()));
        if public {
          assert_eq!(row["public_result"], value, "read the original schema-checked result");
        } else {
          assert!(row.get("public_result").is_none(), "legacy outcomes disclose no body");
        }
        for private in ["input", "context", "build", "ownership", "artifacts"] {
          assert!(
            row.get(private).is_none(),
            "private {private} is never a result projection"
          );
        }
      }
      let after = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
      assert_eq!(
        before, after,
        "reads cannot dispatch, retry, reclassify Work or edit Issue metadata"
      );
      assert_eq!(store.mutation_evidence_counts().await, mutations);
      assert_eq!(after.work.priority(), fixture.admission.work.priority());
      assert_ne!(json!(after.work.priority()), assessment["priority_assessment"]);
      assert!(
        after.flow.data.records[0].input().context().is_some(),
        "private provenance remains retained"
      );
      assert!(after.flow.data.records[0].build_provenance().is_some());
    }
  });
}
