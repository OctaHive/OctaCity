//! Mapping publication uses ordinary Issue authority and the existing installed read capability.
use super::*;
use octacity_server_store::{
  IssueField, IssueFieldPolicy, IssueSourceFieldAuthority, IssueSourceMappingDefinition, IssueSourceRefreshStore,
};
#[path = "issue_source_mapping_contract_tests.rs"]
mod contracts;
#[path = "issue_source_mapping_read_tests.rs"]
mod reads;
#[path = "issue_source_mapping_writer_tests.rs"]
mod writer;

fn installed(project_id: ProjectId) -> InstalledIssueSourceMapping {
  InstalledIssueSourceMapping {
    project_id,
    source: selection(project_id).source,
    native_resource: selection(project_id).item.resource,
    readable: octacity_server_factory::WorkSourceReportingPolicy {
      labels: vec!["native-ready".into(), "native-other".into()],
      states: vec!["opaque-state".into(), "closed".into()],
    },
    writer: None,
  }
}

#[test]
fn issue_source_mapping_supports_advertised_opaque_native_values_outside_factory_key_syntax() {
  run_ready(async {
    use octacity_server_store::{IssueSourceStatusCondition, IssueWorkflowStore};
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let value = "Waiting for native review";
    let mut capability = installed(project_id);
    capability.readable.states.push(value.into());
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![capability])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "opaque-native-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let workflow = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let target = workflow.definition.transitions[0].to;
    let mut mapping = mapping_command(&issue, "opaque-native-mapping");
    mapping.intent.definition.status.as_mut().unwrap().pull = vec![
      serde_json::from_value(serde_json::json!({
        "native": {"state":value,"all_of":[],"any_of":[],"none_of":[]}, "status_id":target,
      }))
      .expect("installed native values are data, not Factory identifiers"),
    ];
    commands.handle_command(&context(), mapping).await.unwrap();
    for (observed_at, expected) in [
      (32, IssueSourceStatusCondition::NotFresh),
      (33, IssueSourceStatusCondition::Applied),
    ] {
      let reads = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(source_handlers(
          store.clone(),
          Arc::new(Reader {
            state: Some(value.into()),
            observed_at,
            ..Default::default()
          }),
          policy.clone(),
        )),
      );
      let result = reads
        .handle_command(
          &context(),
          RefreshIssueSourceCommand {
            project_id,
            issue_id: issue.id,
            read: read(&selection(project_id)),
            idempotency_key: key(&format!("opaque-native-read-{observed_at}")),
            refreshed_at: at(34),
          },
        )
        .await
        .unwrap();
      assert_eq!(result.status.unwrap().condition, expected);
    }
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap().status_id, target);
  });
}
fn mapping_command(issue: &octacity_server_store::IssueRevision, identity: &str) -> PublishIssueSourceMappingCommand {
  PublishIssueSourceMappingCommand {
    intent: PublishIssueSourceMappingIntent {
      project_id: issue.project_id,
      issue_id: issue.id,
      expected_issue_version: issue.version,
      expected_mapping: None,
      definition: IssueSourceMappingDefinition {
        source: selection(issue.project_id).source,
        reporter: None,
        push_enabled: false,
        fields: [(IssueField::Status, IssueSourceFieldAuthority::SourceOwnedPull)].into(),
        status: Some(octacity_server_store::IssueSourceStatusMapping {
          workflow: octacity_server_store::IssueWorkflowPin {
            id: issue.workflow_id,
            version: issue.workflow_version,
          },
          pull: vec![],
          push: None,
        }),
      },
    },
    idempotency_key: key(identity),
    published_at: at(31),
  }
}

struct SubstitutedStatusReader;
#[async_trait::async_trait]
impl WorkSourceObservations for SubstitutedStatusReader {
  async fn authorize(
    &self,
    context: &ManagementRequestContext,
    project_id: ProjectId,
    source: WorkSourceBinding,
  ) -> Result<(), ApplicationError> {
    Reader::default().authorize(context, project_id, source).await
  }
  async fn read(
    &self,
    context: &ManagementRequestContext,
    request: ReadWorkSourceObservations,
  ) -> Result<WorkSourceObservationPage, ApplicationError> {
    let mut page = Reader {
      observed_at: 32,
      ..Default::default()
    }
    .read(context, request)
    .await?;
    page.observations[0].snapshot.state = "closed".into();
    // The receipt still certifies the original opaque-state snapshot, not these substituted facts.
    Ok(page)
  }
}

#[test]
fn issue_source_mapping_rejects_status_facts_not_covered_by_the_original_snapshot_digest() {
  run_ready(async {
    use octacity_server_store::{IssueSourceStatusRule, IssueWorkflowStore};
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![installed(project_id)])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "digest-bound-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let workflow = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let mut mapping = mapping_command(&issue, "digest-bound-mapping");
    mapping
      .intent
      .definition
      .status
      .as_mut()
      .unwrap()
      .pull
      .push(IssueSourceStatusRule {
        native: octacity_server_factory::WorkSourcePredicate {
          label_presence: Default::default(),
          state: Some("closed".parse().unwrap()),
          all_of: vec![],
          any_of: vec![],
          none_of: vec![],
        },
        status_id: workflow.definition.transitions[0].to,
      });
    commands.handle_command(&context(), mapping).await.unwrap();
    let before = store.mutation_evidence_counts().await;
    let reads = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(SubstitutedStatusReader),
        policy,
      )),
    );
    assert!(
      reads
        .handle_command(
          &context(),
          RefreshIssueSourceCommand {
            project_id,
            issue_id: issue.id,
            read: read(&selection(project_id)),
            idempotency_key: key("deny-substituted-status"),
            refreshed_at: at(34),
          }
        )
        .await
        .is_err(),
      "status data cannot acquire authority from an unrelated digest"
    );
    assert_eq!(store.issue(project_id, issue.id, None).await.unwrap(), issue);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn issue_source_mapping_rejects_oversized_documents_before_publishing_ownership() {
  run_ready(async {
    use octacity_server_store::{IssueSourceStatusRule, IssueWorkflowStore};
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let mut capability = installed(project_id);
    let labels = vec!["L".repeat(256), "M".repeat(256)];
    capability.readable.labels = labels.clone();
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy)
          .with_source_mappings(vec![capability])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "bounded-mapping-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let workflow = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let mut command = mapping_command(&issue, "oversized-mapping");
    command.intent.definition.status.as_mut().unwrap().pull = vec![
      IssueSourceStatusRule {
        native: octacity_server_factory::WorkSourcePredicate {
          label_presence: Default::default(),
          state: None,
          all_of: labels.clone(),
          any_of: labels,
          none_of: vec![],
        },
        status_id: workflow.definition.transitions[0].to,
      };
      128
    ];
    assert!(serde_json::to_vec(&command.intent.definition).unwrap().len() > 65_536);
    let before = store.mutation_evidence_counts().await;
    assert!(
      commands.handle_command(&context(), command).await.is_err(),
      "bounded row counts cannot replace a document byte ceiling"
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      store.issue_field_policy(project_id, issue.id).await.unwrap(),
      IssueFieldPolicy::default()
    );
  });
}

#[test]
fn issue_source_status_pull_uses_the_shared_edge_once_and_preserves_echo_and_source_provenance() {
  run_ready(async {
    use octacity_server_store::{IssueSourceStatusCondition, IssueSourceStatusRule, IssueWorkflowStore};
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![installed(project_id)])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "status-link"))
        .await
        .unwrap(),
    );
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    let workflow = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let edge = &workflow.definition.transitions[0];
    let mut command = mapping_command(&before, "source-status-ownership");
    command
      .intent
      .definition
      .status
      .as_mut()
      .unwrap()
      .pull
      .push(IssueSourceStatusRule {
        native: octacity_server_factory::WorkSourcePredicate {
          label_presence: Default::default(),
          state: Some("opaque-state".parse().unwrap()),
          all_of: vec![],
          any_of: vec![],
          none_of: vec![],
        },
        status_id: edge.to,
      });
    let mapping = commands.handle_command(&context(), command).await.unwrap().mapping;
    for (observed_at, expected) in [
      (32, IssueSourceStatusCondition::Applied),
      (33, IssueSourceStatusCondition::Echo),
    ] {
      let reads = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(source_handlers(
          store.clone(),
          Arc::new(Reader {
            observed_at,
            ..Default::default()
          }),
          policy.clone(),
        )),
      );
      let outcome = reads
        .handle_command(
          &context(),
          RefreshIssueSourceCommand {
            project_id,
            issue_id: before.id,
            read: read(&selection(project_id)),
            idempotency_key: key(&format!("mapped-status-read-{observed_at}")),
            refreshed_at: at(34),
          },
        )
        .await
        .unwrap();
      assert_eq!(outcome.status.unwrap().condition, expected);
      let current = store.issue(project_id, before.id, None).await.unwrap();
      assert_eq!(current.status_id, edge.to);
      assert_eq!(current.version.get(), 2, "an echo must not create another revision");
      assert_eq!(current.content, before.content);
    }
    let history = store
      .issue_status_history(project_id, before.id, None, 10)
      .await
      .unwrap();
    assert_eq!(history.items.len(), 2);
    let moved = history.items.last().unwrap();
    assert_eq!(moved.origin, IssueTransitionOrigin::Source);
    assert_eq!(moved.transition_id, Some(edge.id));
    assert_eq!(moved.source.as_ref().unwrap().mapping, mapping.reference());
    let review = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(source_handlers(
        store.clone(),
        Arc::new(Reader {
          denied: true,
          ..Default::default()
        }),
        policy,
      )),
    )
    .handle_query(
      &context(),
      GetIssueSynchronizationQuery {
        project_id,
        issue_id: before.id,
      },
    )
    .await
    .unwrap();
    let wire = serde_json::to_value(review).unwrap();
    assert_eq!(wire["observation"]["values"]["state"], "opaque-state");
    assert_eq!(wire["observation"]["values"]["labels"], serde_json::json!([]));
    assert_eq!(
      wire["observation"]["values"]["mapping"],
      serde_json::json!(mapping.reference())
    );
    assert_eq!(wire["observation"]["accepted"]["observed_at"], 33);
    assert!(
      store.retained_issue_push_intents().unwrap().is_empty(),
      "pull echoes cannot emit an action"
    );
    assert_eq!(store.issue(project_id, before.id, None).await.unwrap().version.get(), 2);
  });
}

#[test]
fn issue_source_status_mapping_cannot_reinterpret_an_observation_from_before_its_publication() {
  run_ready(async {
    use octacity_server_store::{IssueSourceStatusCondition, IssueSourceStatusRule, IssueWorkflowStore};
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
          .with_source_mappings(vec![installed(project_id)])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "pre-mapping-link"))
        .await
        .unwrap(),
    );
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    let workflow = store.default_issue_workflow(project_id).await.unwrap().workflow;
    let mut mapping = mapping_command(&before, "mapping-after-cached-read");
    mapping
      .intent
      .definition
      .status
      .as_mut()
      .unwrap()
      .pull
      .push(IssueSourceStatusRule {
        native: octacity_server_factory::WorkSourcePredicate {
          label_presence: Default::default(),
          state: Some("opaque-state".parse().unwrap()),
          all_of: vec![],
          any_of: vec![],
          none_of: vec![],
        },
        status_id: workflow.definition.transitions[0].to,
      });
    commands.handle_command(&context(), mapping).await.unwrap();
    for (observed_at, expected) in [
      (21, IssueSourceStatusCondition::NotFresh),
      (31, IssueSourceStatusCondition::NotFresh),
      (32, IssueSourceStatusCondition::Applied),
    ] {
      let reads = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(source_handlers(
          store.clone(),
          Arc::new(Reader {
            observed_at,
            ..Default::default()
          }),
          policy.clone(),
        )),
      );
      let outcome = reads
        .handle_command(
          &context(),
          RefreshIssueSourceCommand {
            project_id,
            issue_id: before.id,
            read: read(&selection(project_id)),
            idempotency_key: key(&format!("read-across-mapping-{observed_at}")),
            refreshed_at: at(34),
          },
        )
        .await
        .unwrap();
      assert_eq!(outcome.status.unwrap().condition, expected);
      let state = store
        .issue_source_refresh_state(project_id, before.id, &context().security_scope().to_store())
        .await
        .unwrap()
        .unwrap();
      assert_eq!(
        serde_json::to_value(state).unwrap().get("values").is_some(),
        observed_at > 31,
        "a cached read from before mapping publication cannot be relabeled as new-policy values"
      );
      assert_eq!(
        store.issue(project_id, before.id, None).await.unwrap().version.get(),
        if observed_at <= 31 { 1 } else { 2 }
      );
    }
  });
}

#[test]
fn issue_source_mapping_rejects_backdated_replacement_epochs() {
  run_ready(async {
    use octacity_server_store::IssueSourceMappingStore;
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let commands = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(
        source_handlers(store.clone(), Arc::new(Reader::default()), policy)
          .with_source_mappings(vec![installed(project_id)])
          .unwrap(),
      ),
    );
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "mapping-clock-link"))
        .await
        .unwrap(),
    );
    let issue = store.issue(project_id, link.issue_id, None).await.unwrap();
    let mut command = mapping_command(&issue, "first-mapping-clock");
    command.intent.definition.fields = [(IssueField::Description, IssueSourceFieldAuthority::SourceOwnedPull)].into();
    command.intent.definition.status = None;
    command.published_at = at(50);
    let original = commands
      .handle_command(&context(), command.clone())
      .await
      .unwrap()
      .mapping;
    command.intent.expected_mapping = Some(original.reference());
    command
      .intent
      .definition
      .fields
      .insert(IssueField::Title, IssueSourceFieldAuthority::SourceOwnedPull);
    command.idempotency_key = key("backdated-mapping-clock");
    command.published_at = at(40);
    let before = store.mutation_evidence_counts().await;
    assert!(
      commands.handle_command(&context(), command).await.is_err(),
      "a replacement cannot reopen an older observation epoch"
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
    assert_eq!(
      store
        .issue_source_mapping(project_id, issue.id, &context().security_scope().to_store(), None)
        .await
        .unwrap(),
      Some(original)
    );
  });
}

#[test]
fn issue_source_mapping_publication_installs_reviewed_ownership_without_editing_the_issue() {
  run_ready(async {
    let store = Arc::new(InMemoryProjectStore::new());
    let project_id = project(&store).await;
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = source_handlers(store.clone(), Arc::new(Reader::default()), policy.clone())
      .with_source_mappings(vec![InstalledIssueSourceMapping {
        project_id,
        source: selection(project_id).source,
        native_resource: selection(project_id).item.resource,
        readable: octacity_server_factory::WorkSourceReportingPolicy {
          labels: vec![],
          states: vec![],
        },
        writer: None,
      }])
      .unwrap();
    let commands = AuthorizedCommandHandler::new(policy.clone(), Arc::new(handlers));
    let link = linked(
      commands
        .handle_command(&context(), import(selection(project_id), "mapped-task"))
        .await
        .unwrap(),
    );
    let before = store.issue(project_id, link.issue_id, None).await.unwrap();
    let command = PublishIssueSourceMappingCommand {
      intent: octacity_server_store::PublishIssueSourceMappingIntent {
        project_id,
        issue_id: link.issue_id,
        expected_issue_version: before.version,
        expected_mapping: None,
        definition: IssueSourceMappingDefinition {
          source: selection(project_id).source,
          reporter: None,
          push_enabled: false,
          fields: [(IssueField::Description, IssueSourceFieldAuthority::SourceOwnedPull)].into(),
          status: None,
        },
      },
      idempotency_key: key("publish-source-ownership"),
      published_at: at(31),
    };
    let original = commands.handle_command(&context(), command.clone()).await.unwrap();
    assert_eq!(store.issue(project_id, link.issue_id, None).await.unwrap(), before);
    assert_eq!(
      store.issue_field_policy(project_id, link.issue_id).await.unwrap(),
      IssueFieldPolicy {
        source_owned: [IssueField::Description].into(),
        ..Default::default()
      }
    );
    let manual = AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(store.clone(), policy)));
    let mut content = before.content.clone();
    content.description = "Attempted local replacement".into();
    assert!(
      manual
        .handle_command(
          &context(),
          EditIssueCommand {
            intent: EditIssueIntent {
              project_id,
              id: before.id,
              expected_version: before.version,
              content
            },
            idempotency_key: key("deny-owned-description"),
            edited_at: at(32),
          }
        )
        .await
        .is_err()
    );
    let mut content = before.content.clone();
    content.priority = Some(3);
    manual
      .handle_command(
        &context(),
        EditIssueCommand {
          intent: EditIssueIntent {
            project_id,
            id: before.id,
            expected_version: before.version,
            content,
          },
          idempotency_key: key("local-priority-remains-editable"),
          edited_at: at(32),
        },
      )
      .await
      .unwrap();
    let replay = commands.handle_command(&context(), command).await.unwrap();
    assert_eq!(replay.mapping, original.mapping);
    assert_eq!(replay.disposition, octacity_server_store::MutationDisposition::Replayed);
  });
}
