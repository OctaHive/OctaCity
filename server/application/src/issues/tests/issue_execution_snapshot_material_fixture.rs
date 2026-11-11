use super::*;

pub(super) struct MaterialClock;
impl FactoryExecutionClock for MaterialClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(45))
  }
}

pub(super) async fn selected_comment_fixture() -> Fixture {
  let (store, factory, issue, mut command) = snapshot_fixture().await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  let comments = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueDiscussionHandlers::new(
      store.clone(),
      policy.clone(),
      Arc::new(MaterialClock),
    )),
  );
  let comment = comments
    .handle_command(
      &context(),
      AppendIssueCommentCommand {
        intent: AppendIssueCommentIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          parent_id: None,
          body: IssueCommentBody::try_new("COMMENT_SELECTED\r\nёж\n").unwrap(),
        },
        idempotency_key: key("snapshot-comment"),
      },
    )
    .await
    .unwrap()
    .comment;
  comments
    .handle_command(
      &context(),
      AppendIssueCommentCommand {
        intent: AppendIssueCommentIntent {
          project_id: issue.project_id,
          issue_id: issue.id,
          parent_id: None,
          body: IssueCommentBody::try_new("UNSELECTED_DISCUSSION").unwrap(),
        },
        idempotency_key: key("unselected-comment"),
      },
    )
    .await
    .unwrap();
  assert!(
    store
      .issue(issue.project_id, issue.id, None)
      .await
      .unwrap()
      .materials
      .is_empty()
  );
  let selections = AuthorizedCommandHandler::new(
    policy.clone(),
    Arc::new(IssueMaterialHandlers::new(
      store.clone(),
      factory.clone(),
      policy,
      Arc::new(MaterialClock),
    )),
  );
  let issue = selections
    .handle_command(
      &context(),
      EditIssueMaterialsCommand {
        intent: EditIssueMaterialsIntent {
          project_id: issue.project_id,
          id: issue.id,
          expected_version: issue.version,
          references: vec![IssueMaterialReference::Comment {
            project_id: issue.project_id,
            issue_id: issue.id,
            comment_id: comment.id,
            body_digest: comment.body_digest,
          }],
        },
        idempotency_key: key("select-snapshot-comment"),
      },
    )
    .await
    .unwrap()
    .issue;
  let id = FactoryConfigurationId::generate();
  let configuration = material_configuration(
    issue.project_id,
    id,
    FlowIssueMaterialSource::Exact {
      references: vec![FlowIssueMaterialReference::Comment {
        comment_id: comment.id,
        body_digest: comment.body_digest,
      }],
    },
    true,
  );
  crate::factory::admission::tests::factory_admission_tests::seed_permitted_configuration(
    &factory,
    PublishedFactoryConfiguration {
      configuration,
      published_at: at(46),
    },
  )
  .unwrap();
  command.intent.target.configuration_id = id;
  command.intent.expected_version = issue.version;
  command.requested_at = at(50);
  (store, factory, issue, command)
}

pub(super) fn material_configuration(
  project: ProjectId,
  id: FactoryConfigurationId,
  source: FlowIssueMaterialSource,
  required: bool,
) -> FactoryConfiguration {
  let base = snapshot_configuration_with_schema(
    project,
    id,
    0,
    [(
      "title".parse().unwrap(),
      FlowIssueFieldSelection {
        source: FlowIssueFieldSource::Title,
        required: true,
      },
    )]
    .into(),
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
          if required {
            FlowFieldSchema::required(FlowValueSchema::Array {
              item: Box::new(FlowValueSchema::String {
                min_bytes: 0,
                max_bytes: 4096,
              }),
              min_items: u16::from(required),
              max_items: 4,
            })
          } else {
            FlowFieldSchema::optional(FlowValueSchema::Array {
              item: Box::new(FlowValueSchema::String {
                min_bytes: 0,
                max_bytes: 4096,
              }),
              min_items: 0,
              max_items: 4,
            })
          },
        ),
      ]
      .into(),
    },
  );
  let mut flow = base.flow().clone();
  flow.issue_snapshot.as_mut().unwrap().materials.insert(
    "context".parse().unwrap(),
    FlowIssueMaterialSelection {
      source,
      required,
      max_bytes: 4096,
    },
  );
  crate::factory::admission::tests::factory_admission_tests::configuration_with_flow(project, id, Some(flow))
}
