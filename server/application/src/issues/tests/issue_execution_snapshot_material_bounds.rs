//! Consumer limits apply to all selected UTF-8 text, including optional aliases.
use super::*;

pub(super) async fn select_contract(f: &mut MaterialFixture, contract: FlowIssueMaterialSelection) {
  let id = FactoryConfigurationId::generate();
  let base = material_configuration(f.issue.project_id, id, contract.source.clone(), contract.required);
  let mut flow = base.flow().clone();
  flow
    .issue_snapshot
    .as_mut()
    .unwrap()
    .materials
    .insert("context".parse().unwrap(), contract);
  let configuration = crate::factory::admission::tests::factory_admission_tests::configuration_with_flow(
    f.issue.project_id,
    id,
    Some(flow),
  );
  crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
    &f.factory,
    PublishedFactoryConfiguration {
      configuration,
      published_at: at(46),
    },
  )
  .unwrap();
  f.command.intent.target.configuration_id = id;
}

#[test]
fn issue_material_snapshot_blocks_oversized_optional_selected_sets_before_reserving_execution() {
  run_ready(async {
    let mut f = MaterialFixture::new(true).await;
    select_contract(
      &mut f,
      FlowIssueMaterialSelection {
        source: FlowIssueMaterialSource::Selected { max_entries: 2 },
        required: false,
        max_bytes: 4096,
      },
    )
    .await;
    let before = f.issues.mutation_evidence_counts().await;
    let factory_before = f.factory.mutation_evidence_counts().await;
    let error = f
      .review(f.readers(Arc::new(TrustedNetworkManagementPolicy)))
      .await
      .expect_err("a count limit is a blocker, not an optional omission");
    let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
      panic!("expected bounded material diagnostic: {error:?}")
    };
    assert_eq!(report.errors[0].field, "/issue_snapshot/materials/context");
    assert_eq!(report.errors[0].code, "issue_snapshot_material_limit_exceeded");
    assert_eq!(f.issues.mutation_evidence_counts().await, before);
    assert_eq!(f.factory.mutation_evidence_counts().await, factory_before);
    f.no_runs().await;
  });
}

#[test]
fn issue_material_snapshot_counts_combined_utf8_bytes_with_an_inclusive_consumer_limit() {
  run_ready(async {
    // The three bodies contain 78 characters but 91 UTF-8 bytes in total.
    for required in [true, false] {
      for max_bytes in [90, 91] {
        let mut f = MaterialFixture::new(true).await;
        select_contract(
          &mut f,
          FlowIssueMaterialSelection {
            source: FlowIssueMaterialSource::Selected { max_entries: 3 },
            required,
            max_bytes,
          },
        )
        .await;
        let before = f.issues.mutation_evidence_counts().await;
        let result = f.review(f.readers(Arc::new(TrustedNetworkManagementPolicy))).await;
        if max_bytes == 90 {
          let error = result.expect_err("combined bytes exceed the bound despite each body fitting");
          let Some(ApplicationError::FactoryValidation(report)) = error.application() else {
            panic!("expected a material byte-limit diagnostic: {error:?}")
          };
          assert_eq!(report.errors[0].code, "issue_snapshot_material_limit_exceeded");
          assert_eq!(report.errors[0].field, "/issue_snapshot/materials/context");
          assert_eq!(f.issues.mutation_evidence_counts().await, before);
        } else {
          let request = result.unwrap().request;
          let published = f
            .publish(&request, f.readers(Arc::new(TrustedNetworkManagementPolicy)))
            .await
            .unwrap();
          let run = f
            .factory
            .factory_run_snapshot(published.association.unwrap().run_id)
            .await
            .unwrap();
          assert_eq!(
            run.flow.data.incoming[0].payload().value()["context"],
            serde_json::json!([
              "COMMENT_SELECTED\r\nёж\n",
              "ISSUE_DOCUMENT\r\nточные байты\n",
              "PROJECT_DOCUMENT\r\nreference\n"
            ])
          );
        }
      }
    }
  });
}
