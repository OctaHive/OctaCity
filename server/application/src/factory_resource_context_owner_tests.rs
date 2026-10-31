//! The installed Artifact context owner must use the selected node resource before reading bytes.
use super::*;
use crate::{
  FactoryArtifactFlowInputs, FactoryArtifactReader, FactoryFlowInputPreparer, FactoryNodeTarget,
  ProjectDocumentSubmission, ProjectTextIntake, TrustedNetworkManagementPolicy,
};
use octacity_server_factory::{
  ContextSourceKind, FactoryContextReference, FactoryFlowConfiguration, FlowContextBinding, FlowContextSelection,
  FlowDataMapping, FlowDefinition, FlowDefinitionId, FlowDefinitionInput, FlowDefinitionVersion, FlowValueMapping,
  PinnedFlowDefinitionClosure,
};
use octacity_server_store::{
  AttemptRecord, BuildQueryStore, BuildRecord, FactoryConfigurationStore as _, JobRecord, StoreError,
  testing::{ArtifactLeaseFixture, InMemoryArtifactRecordStore},
};
use std::sync::atomic::Ordering;

struct Clock(i64);
impl crate::FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, crate::ApplicationError> {
    Ok(time(self.0))
  }
}
struct NoProducingBuilds;
#[async_trait::async_trait]
impl BuildQueryStore for NoProducingBuilds {
  async fn build(&self, _: octacity_server_domain::BuildId) -> Result<BuildRecord, StoreError> {
    panic!("Project documents have no producing Build")
  }
  async fn attempt(&self, _: octacity_server_domain::AttemptId) -> Result<AttemptRecord, StoreError> {
    unreachable!()
  }
  async fn latest_attempt(&self, _: octacity_server_domain::BuildId) -> Result<AttemptRecord, StoreError> {
    unreachable!()
  }
  async fn job(&self, _: octacity_server_domain::JobId) -> Result<JobRecord, StoreError> {
    unreachable!()
  }
}

#[test]
fn installed_artifact_context_owner_prepares_selected_project_documents_for_no_code_and_other_repository_steps() {
  selected_documents(false);
}

#[test]
fn unsupported_repository_context_blocks_before_reading_any_selected_document() {
  selected_documents(true);
}

fn selected_documents(unsupported_repository_context: bool) {
  run_ready(async {
    let CodeFixture {
      store,
      repositories,
      mut request,
      ..
    } = code_fixture().await;
    let bytes = Arc::new(crate::factory_test_support::InMemoryArtifactBytes::default());
    let resolver = Arc::new(RecordingResolver::default());
    let document = ProjectTextIntake::new(
      store.clone(),
      bytes.clone(),
      resolver.clone(),
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(Clock(20)),
    )
    .publish_document(
      &context(),
      ProjectDocumentSubmission {
        project_id: request.project_id,
        name: "Component guidance".parse().unwrap(),
        content: "Components are documented here; missing values remain unresolved.".into(),
        idempotency_key: "selected-component-guidance".parse().unwrap(),
      },
    )
    .await
    .unwrap();
    assert_eq!(resolver.call_count(), 0);
    let original = store
      .factory_configuration_version(request.configuration_id, request.configuration_version)
      .await
      .unwrap()
      .configuration;
    let flow = original.flow();
    let old = flow.closure.definition(flow.closure.root()).unwrap();
    let mut selections = std::collections::BTreeMap::from([(
      key("guidance"),
      FlowContextSelection {
        when_present: None,
        source_kind: ContextSourceKind::Specification,
        source: FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::to_value(FactoryContextReference::Artifact(document.artifact.clone())).unwrap(),
        })
        .unwrap(),
      },
    )]);
    if unsupported_repository_context {
      selections.insert(
        key("z-unsupported"),
        FlowContextSelection {
          when_present: None,
          source_kind: ContextSourceKind::Specification,
          source: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::to_value(FactoryContextReference::RepositoryRange(
              octacity_server_factory::FactoryRepositoryRange::new(
                RepositoryId::generate(),
                "declared-revision".parse().unwrap(),
                octacity_server_factory::FactoryRepositoryPath::new("src/lib.rs").unwrap(),
                1,
                5,
              )
              .unwrap(),
            ))
            .unwrap(),
          })
          .unwrap(),
        },
      );
    }
    let binding = FlowContextBinding::new(None, selections).unwrap();
    let mut recipients = vec![];
    let nodes = old
      .nodes()
      .iter()
      .map(|node| {
        if node.code() == Some(&FlowCodeBinding::None)
          || matches!(node.code(),Some(FlowCodeBinding::Source {source,..}) if source.as_str() == "first")
        {
          recipients.push(node.key().clone());
          node.clone().with_context_binding(binding.clone()).unwrap()
        } else {
          node.clone()
        }
      })
      .collect();
    let root = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: old.input_schema().cloned(),
      entry: old.entry().clone(),
      nodes,
      transitions: old.transitions().to_vec(),
      terminals: old.terminals().to_vec(),
      context_projections: old.context_projections().to_vec(),
      data_projections: old.data_projections().to_vec(),
      execution: old.execution().clone(),
    })
    .unwrap()
    .with_code_sources(old.code_sources().unwrap().clone())
    .unwrap();
    let flow = FactoryFlowConfiguration::new(
      PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap(),
      flow.limits.clone(),
      flow.data_schemas.clone(),
      flow.pools.clone(),
    )
    .unwrap();
    request.configuration_id = FactoryConfigurationId::generate();
    request.admitted_at = time(22);
    store
      .seed_factory_configuration_version(PublishedFactoryConfiguration {
        configuration: configuration_with_flow(request.project_id, request.configuration_id, Some(flow)),
        published_at: time(21),
      })
      .unwrap();
    let admission = admit(
      &FactoryAdmissionHandlers::new(store.clone(), resolver.clone()).with_repositories(repositories),
      request,
    )
    .await
    .unwrap();
    let snapshot = store.factory_run_snapshot(admission.factory_run_id).await.unwrap();
    let fixture = octacity_server_store::testing::authoritative_store_contract_fixture();
    let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
      access: octacity_server_store::LeaseAccess {
        lease_id: octacity_server_domain::LeaseId::generate(),
        fence: octacity_server_store::LeaseFence::from_bytes([7; 32]),
        agent_id: fixture.agent_id,
        registration_epoch: fixture.registration_epoch,
      },
      build_id: fixture.request.build.id,
      attempt_id: fixture.request.attempt_id,
      attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
      job_id: fixture.request.jobs[0].id,
      expires_at: time(10000),
    }));
    let reader = Arc::new(
      FactoryArtifactReader::new(artifacts, Arc::new(NoProducingBuilds), bytes.clone()).with_project_inputs(store),
    );
    let preparer = FactoryArtifactFlowInputs::new(reader, Arc::new(Clock(20)));
    let before = bytes.2.load(Ordering::SeqCst);
    for node in recipients {
      let target = FactoryNodeTarget {
        flow_run_id: snapshot.admitted_flow.root_run().id(),
        cycle_id: snapshot.admitted_flow.initial_cycle().id(),
        node,
      };
      let prepared = preparer.prepare(&snapshot, &target).await;
      if unsupported_repository_context {
        assert!(prepared.is_err());
        assert_eq!(
          bytes.2.load(Ordering::SeqCst),
          before,
          "all selected input kinds must qualify before any protected byte read"
        );
        continue;
      }
      assert!(
        prepared.is_ok(),
        "the installed context owner must support explicit resource provenance: {prepared:?}"
      );
      let prepared = prepared.unwrap();
      let context = prepared.context().unwrap();
      assert_eq!(context.subject(), &prepared.task_subject().unwrap());
      assert_eq!(context.entries().len(), 1);
      assert_eq!(
        context.entries()[0].source(),
        &FactoryContextReference::Artifact(document.artifact.clone())
      );
    }
    assert_eq!(
      bytes.2.load(Ordering::SeqCst) - before,
      if unsupported_repository_context { 0 } else { 2 },
      "each selected recipient reads only its explicit document"
    );
    assert_eq!(
      resolver.call_count(),
      2,
      "Context preparation must not resolve source refs again"
    );
  });
}
