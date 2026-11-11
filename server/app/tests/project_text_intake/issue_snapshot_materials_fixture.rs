//! Same selected-input constructor and authorized readers as the ordinary manual runtime.
use super::*;

pub(super) fn material_flow() -> FactoryFlowConfiguration {
  let schema = FlowDataSchema::new(
    "issue.materials".parse().unwrap(),
    "v1".parse().unwrap(),
    FlowValueSchema::Object {
      fields: [
        (
          "title".parse().unwrap(),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 512,
          }),
        ),
        (
          "context".parse().unwrap(),
          FlowFieldSchema::required(FlowValueSchema::Array {
            item: Box::new(FlowValueSchema::String {
              min_bytes: 0,
              max_bytes: 4096,
            }),
            min_items: 1,
            max_items: 3,
          }),
        ),
      ]
      .into(),
    },
  )
  .unwrap();
  snapshot_flow_with_input(
    FlowIssueSnapshotInput {
      version: 1,
      schema: schema.reference().clone(),
      definition_version: 0,
      admission: None,
      fields: [(
        "title".parse().unwrap(),
        FlowIssueFieldSelection {
          source: FlowIssueFieldSource::Title,
          required: true,
        },
      )]
      .into(),
      materials: [(
        "context".parse().unwrap(),
        FlowIssueMaterialSelection {
          source: FlowIssueMaterialSource::Selected { max_entries: 3 },
          required: true,
          max_bytes: 4096,
        },
      )]
      .into(),
      evidence: Default::default(),
    },
    schema,
  )
}

pub(super) fn readers(
  store: Arc<PostgresStore>,
  bytes: Arc<Bytes>,
  clock: Arc<Clock>,
) -> Arc<IssueSnapshotMaterialReaders> {
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let docs = Arc::new(
    ProjectTextIntake::new(
      store.clone(),
      bytes,
      Arc::new(Resolver(AtomicUsize::new(0))),
      policy.clone(),
      clock.clone(),
    )
    .with_issue_documents(store.clone()),
  );
  Arc::new(
    IssueSnapshotMaterialReaders::new(Arc::new(AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(IssueDiscussionHandlers::new(store.clone(), policy.clone(), clock)),
    )))
    .with_documents(
      Arc::new(AuthorizedQueryHandler::new(policy.clone(), docs.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, docs)),
      store,
    ),
  )
}

pub(super) fn material_publisher(
  store: Arc<PostgresStore>,
  bytes: Arc<Bytes>,
  reader: Arc<IssueSnapshotMaterialReaders>,
  resolved: usize,
  now: i64,
) -> AuthorizedCommandHandler<IssueExecutionPublisher<PostgresStore, PostgresStore, Bytes>> {
  AuthorizedCommandHandler::new(
    Arc::new(TrustedNetworkManagementPolicy),
    Arc::new(
      IssueExecutionPublisher::new(
        store.clone(),
        store,
        bytes,
        Arc::new(Resolver(AtomicUsize::new(resolved))),
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(Clock(AtomicI64::new(now))),
      )
      .with_materials(reader),
    ),
  )
}
