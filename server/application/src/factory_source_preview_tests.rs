//! Public read-only preview, independent of Work and future task-card storage.
use super::*;
struct Reader;

#[derive(Clone, Copy)]
enum IssueLinksFailure {
  Unavailable,
  Hidden,
}
#[async_trait]
impl FactoryWorkSourceIssueLinks for IssueLinksFailure {
  async fn visible_issue(
    &self,
    _: &ManagementRequestContext,
    _: ProjectId,
    _: FactoryDigest,
  ) -> Result<Option<WorkSourceIssueLink>, ApplicationError> {
    Err(match self {
      Self::Unavailable => ApplicationError::unavailable(),
      Self::Hidden => ApplicationError::invalid(),
    })
  }
}

#[test]
fn source_preview_does_not_report_an_empty_issue_association_when_its_authoritative_owner_is_unavailable() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let before = fixture.store.mutation_evidence_counts().await;
    for failure in [IssueLinksFailure::Unavailable, IssueLinksFailure::Hidden] {
      let queries = AuthorizedQueryHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(
          fixture
            .handlers()
            .with_preview(Arc::new(Reader))
            .with_issue_links(Arc::new(failure)),
        ),
      );
      let result = queries
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent: fixture.intent.clone(),
            limit: 2,
            observed_at: time(4),
          },
        )
        .await;
      match failure {
        IssueLinksFailure::Unavailable => assert_eq!(
          result.unwrap_err().application().unwrap().classification(),
          ApplicationFailure::Unavailable
        ),
        IssueLinksFailure::Hidden => assert!(
          result
            .unwrap()
            .candidates
            .iter()
            .all(|candidate| candidate.issue.is_none())
        ),
      }
    }
    assert_eq!(fixture.store.mutation_evidence_counts().await, before);
  });
}

#[async_trait]
impl FactoryWorkSourcePreview for Reader {
  async fn preview(&self, request: ReadWorkSourcePreview) -> Result<WorkSourcePreviewPage, ApplicationError> {
    assert_eq!(request.limit, 2);
    Ok(WorkSourcePreviewPage {
      security_scope: crate::ManagementSecurityScope::trusted_network(),
      has_more: false,
      candidates: vec![
        WorkSourcePreviewItem {
          item: WorkSourceItem {
            resource: ExternalWorkIdentity::new("external-project-42").unwrap(),
            item: ExternalWorkIdentity::new("native-19").unwrap(),
          },
          display_id: "TEAM-19".into(),
          title: "Safe title".into(),
          state: "open".into(),
          labels: vec![],
          selections: request
            .inputs
            .iter()
            .map(|i| WorkSourcePreviewSelection {
              input_id: i.id,
              reasons: vec![],
            })
            .collect(),
        },
        WorkSourcePreviewItem {
          item: WorkSourceItem {
            resource: ExternalWorkIdentity::new("external-project-42").unwrap(),
            item: ExternalWorkIdentity::new("native-20").unwrap(),
          },
          display_id: "TEAM-20".into(),
          title: "Excluded".into(),
          state: "closed".into(),
          labels: vec!["on hold".into()],
          selections: request
            .inputs
            .iter()
            .map(|i| WorkSourcePreviewSelection {
              input_id: i.id,
              reasons: vec![WorkSourceSelectionMismatch::StateMismatch],
            })
            .collect(),
        },
      ],
    })
  }
}
#[test]
fn source_preview_explains_matches_without_admission_or_source_progress_and_uses_native_identity() {
  run_ready(async {
    for provider in ["github", "gitlab", "jira", "custom-tracker"] {
      let fixture = Fixture::new(provider).await;
      let before = fixture.store.mutation_evidence_counts().await;
      let queries = AuthorizedQueryHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(fixture.handlers().with_preview(Arc::new(Reader))),
      );
      let mut intent = fixture.intent.clone();
      let first = queries
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent: intent.clone(),
            limit: 2,
            observed_at: time(4),
          },
        )
        .await
        .unwrap();
      assert_eq!(first.failure, None);
      assert_eq!(first.candidates.len(), 2);
      assert_eq!(first.candidates[0].disposition, WorkSourcePreviewDisposition::Selected);
      assert_eq!(first.candidates[1].disposition, WorkSourcePreviewDisposition::Filtered);
      assert_eq!(
        first.candidates[1].summary.selections[0].reasons,
        [WorkSourceSelectionMismatch::StateMismatch]
      );
      intent.inputs[0].id = WorkSourceInputId::generate();
      let second = queries
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent,
            limit: 2,
            observed_at: time(5),
          },
        )
        .await
        .unwrap();
      assert_eq!(
        first.candidates[0].native_correlation, second.candidates[0].native_correlation,
        "native task identity excludes the automatic input id"
      );
      assert_eq!(fixture.store.mutation_evidence_counts().await, before);
      assert!(
        fixture
          .store
          .source_scan_targets(fixture.intent.project_id, None, 1)
          .await
          .unwrap()
          .is_empty()
      );
      assert!(
        fixture
          .store
          .source_input(
            fixture.intent.project_id,
            fixture.intent.inputs[0].id,
            WorkSourceInputVersion::INITIAL
          )
          .await
          .is_err()
      );
    }
  });
}

#[test]
fn source_preview_freezes_exclusive_members_and_explains_ambiguity_before_activation() {
  run_ready(async {
    let fixture = Fixture::new("jira").await;
    let mut intent = fixture.intent.clone();
    let mut second = intent.inputs[0].clone();
    second.id = WorkSourceInputId::generate();
    intent.inputs.push(second);
    for input in &mut intent.inputs {
      input.definition.enabled = false;
    }
    intent.group = Some(WorkSourceGroupIntent {
      id: WorkSourceGroupId::generate(),
      expected_version: None,
      members: intent
        .inputs
        .iter()
        .map(|i| WorkSourceMemberRef {
          id: i.id,
          version: WorkSourceInputVersion::INITIAL,
        })
        .collect(),
    });
    let query = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(fixture.handlers().with_preview(Arc::new(Reader))),
    );
    let result = query
      .handle_query(
        &context(),
        PreviewWorkSourcesQuery {
          intent: intent.clone(),
          limit: 2,
          observed_at: time(4),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      result.candidates[0].disposition,
      WorkSourcePreviewDisposition::Ambiguous
    );
    assert_eq!(result.candidates[1].disposition, WorkSourcePreviewDisposition::Filtered);
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: intent.clone(),
          idempotency_key: IdempotencyKey::new("disabled-preview-group").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    let mut retained = intent.clone();
    retained.inputs.clear();
    retained.group.as_mut().unwrap().expected_version = Some(WorkSourceGroupVersion::INITIAL);
    let result = query
      .handle_query(
        &context(),
        PreviewWorkSourcesQuery {
          intent: retained.clone(),
          limit: 2,
          observed_at: time(5),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      result.candidates[0].disposition,
      WorkSourcePreviewDisposition::Ambiguous
    );
    assert_eq!(
      result.configuration, retained,
      "preview must preserve references to retained member versions"
    );
    let repeated = query
      .handle_query(
        &context(),
        PreviewWorkSourcesQuery {
          intent: result.configuration.clone(),
          limit: 2,
          observed_at: time(6),
        },
      )
      .await
      .unwrap();
    assert_eq!(repeated.configuration, retained);
    let publication = query
      .handle_query(
        &context(),
        ValidateWorkSourceConfigurationQuery {
          intent: result.configuration,
        },
      )
      .await
      .unwrap();
    assert!(
      publication.inputs.is_empty(),
      "unchanged members must not become input replacements"
    );
    assert!(
      publication
        .group
        .unwrap()
        .group
        .members()
        .iter()
        .all(|i| i.version() == WorkSourceInputVersion::INITIAL)
    );
    intent.group.as_mut().unwrap().members[1].version = WorkSourceInputVersion::new(2).unwrap();
    assert!(
      query
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent,
            limit: 2,
            observed_at: time(5)
          }
        )
        .await
        .is_err()
    );
    assert!(
      fixture
        .store
        .source_scan_targets(fixture.intent.project_id, None, 1)
        .await
        .unwrap()
        .is_empty()
    );
  });
}

struct FailureReader(Option<WorkSourceFailure>);
#[async_trait]
impl FactoryWorkSourcePreview for FailureReader {
  async fn preview(&self, _: ReadWorkSourcePreview) -> Result<WorkSourcePreviewPage, ApplicationError> {
    match self.0 {
      Some(class) => Err(ApplicationError::SourceDependency {
        class,
        retry_after_millis: Some(u32::MAX),
      }),
      None => Ok(WorkSourcePreviewPage {
        security_scope: crate::ManagementSecurityScope::trusted_network(),
        candidates: vec![],
        has_more: false,
      }),
    }
  }
}
#[test]
fn source_preview_distinguishes_empty_from_throttling_and_unavailability_without_scheduling() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let before = fixture.store.mutation_evidence_counts().await;
    for failure in [
      None,
      Some(WorkSourceFailure::RateLimited),
      Some(WorkSourceFailure::Unavailable),
    ] {
      let queries = AuthorizedQueryHandler::new(
        Arc::new(TrustedNetworkManagementPolicy),
        Arc::new(fixture.handlers().with_preview(Arc::new(FailureReader(failure)))),
      );
      let result = queries
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent: fixture.intent.clone(),
            limit: 2,
            observed_at: time(4),
          },
        )
        .await
        .unwrap();
      assert_eq!(result.failure, failure);
      assert_eq!(result.retry_after_millis, failure.map(|_| 3_600_000));
      assert!(result.candidates.is_empty());
      assert!(!result.has_more);
    }
    assert_eq!(fixture.store.mutation_evidence_counts().await, before);
    assert!(
      fixture
        .store
        .source_scan_targets(fixture.intent.project_id, None, 1)
        .await
        .unwrap()
        .is_empty()
    );
  });
}
#[test]
fn source_preview_and_diagnostics_identify_exact_effective_selectors() {
  run_ready(async {
    let fixture = Fixture::new("gitlab").await;
    let labels = WorkSourceSelectionLabels {
      label_presence: Default::default(),
      state: key("open"),
      all_of: vec!["customer request".into()],
      any_of: vec![],
      none_of: vec!["do not automate".into()],
    };
    let handlers = fixture
      .handlers()
      .with_selection_labels(fixture.capability.clone(), labels.clone())
      .unwrap()
      .with_preview(Arc::new(Reader));
    let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers));
    let preview = queries
      .handle_query(
        &context(),
        PreviewWorkSourcesQuery {
          intent: fixture.intent.clone(),
          limit: 2,
          observed_at: time(4),
        },
      )
      .await
      .unwrap();
    assert_eq!(preview.configuration, fixture.intent);
    assert_eq!(preview.selectors[0].input_id, fixture.intent.inputs[0].id);
    assert_eq!(preview.selectors[0].binding, fixture.capability.binding);
    assert_eq!(preview.selectors[0].selection, Some(labels.clone()));
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: fixture.intent.clone(),
          idempotency_key: IdempotencyKey::new("diagnostic-selector").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    let diagnostic = queries
      .handle_query(
        &context(),
        GetWorkSourceDiagnosticsQuery {
          project_id: fixture.intent.project_id,
          target: WorkSourceScanTarget::Input {
            id: fixture.intent.inputs[0].id,
            version: WorkSourceInputVersion::INITIAL,
          },
          after: None,
          limit: 1,
        },
      )
      .await
      .unwrap();
    assert_eq!(diagnostic.selectors[0].selection, Some(labels));
    assert_eq!(diagnostic.selectors[0].binding, fixture.capability.binding);
  });
}
struct NeverPreview;
#[async_trait]
impl FactoryWorkSourcePreview for NeverPreview {
  async fn preview(&self, _: ReadWorkSourcePreview) -> Result<WorkSourcePreviewPage, ApplicationError> {
    panic!("denied reads cannot reach the provider")
  }
}
#[test]
fn source_preview_requires_current_dependency_visibility_and_connection_use_before_reading() {
  run_ready(async {
    let fixture = Fixture::new("jira").await;
    for (kind, action) in [
      (ManagementResourceKind::Project, ManagementAction::View),
      (ManagementResourceKind::Repository, ManagementAction::View),
      (ManagementResourceKind::FactoryConfiguration, ManagementAction::View),
      (ManagementResourceKind::IntegrationConnection, ManagementAction::View),
      (ManagementResourceKind::IntegrationConnection, ManagementAction::Execute),
    ] {
      let policy = Arc::new(DenySourceResource(kind, action));
      let handler = FactorySourceConfigurationHandlers::new(
        fixture.store.clone(),
        policy.clone(),
        vec![fixture.capability.clone()],
      )
      .unwrap()
      .with_preview(Arc::new(NeverPreview));
      assert!(
        AuthorizedQueryHandler::new(policy, Arc::new(handler))
          .handle_query(
            &context(),
            PreviewWorkSourcesQuery {
              intent: fixture.intent.clone(),
              limit: 2,
              observed_at: time(4)
            }
          )
          .await
          .is_err()
      );
    }
  });
}
#[test]
fn source_preview_rejects_duplicate_input_definitions_instead_of_silently_selecting_one() {
  run_ready(async {
    let fixture = Fixture::new("jira").await;
    let mut intent = fixture.intent.clone();
    intent.group = Some(WorkSourceGroupIntent {
      id: WorkSourceGroupId::generate(),
      expected_version: None,
      members: vec![WorkSourceMemberRef {
        id: intent.inputs[0].id,
        version: WorkSourceInputVersion::INITIAL,
      }],
    });
    intent.inputs.push(intent.inputs[0].clone());
    let queries = AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(fixture.handlers().with_preview(Arc::new(NeverPreview))),
    );
    assert!(
      queries
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent,
            limit: 2,
            observed_at: time(4)
          }
        )
        .await
        .is_err()
    );
  });
}

#[test]
fn source_group_preview_does_not_reveal_hidden_member_existence() {
  run_ready(async {
    let fixture = Fixture::new("jira").await;
    AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()))
      .handle_command(
        &context(),
        PublishWorkSourceConfigurationCommand {
          intent: fixture.intent.clone(),
          idempotency_key: IdempotencyKey::new("hidden-member").unwrap(),
          published_at: time(4),
        },
      )
      .await
      .unwrap();
    let policy = Arc::new(DenySourceResource(
      ManagementResourceKind::FactoryConfiguration,
      ManagementAction::View,
    ));
    let handlers =
      FactorySourceConfigurationHandlers::new(fixture.store.clone(), policy.clone(), vec![fixture.capability.clone()])
        .unwrap()
        .with_preview(Arc::new(NeverPreview));
    let queries = AuthorizedQueryHandler::new(policy, Arc::new(handlers));
    for member in [fixture.intent.inputs[0].id, WorkSourceInputId::generate()] {
      let intent = WorkSourceConfigurationIntent {
        project_id: fixture.intent.project_id,
        inputs: vec![],
        group: Some(WorkSourceGroupIntent {
          id: WorkSourceGroupId::generate(),
          expected_version: None,
          members: vec![WorkSourceMemberRef {
            id: member,
            version: WorkSourceInputVersion::INITIAL,
          }],
        }),
      };
      let error = queries
        .handle_query(
          &context(),
          PreviewWorkSourcesQuery {
            intent,
            limit: 2,
            observed_at: time(5),
          },
        )
        .await
        .unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::NotFound,
        "hidden and missing members must use the same protected read boundary"
      );
    }
  });
}

#[test]
fn preview_describes_strict_unlabeled_selection_without_losing_its_presence_requirement() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let mut intent = fixture.intent.clone();
    intent.inputs[0].definition.predicate = Some(
      serde_json::from_value(serde_json::json!({
        "state":"todo","all_of":[],"any_of":[],"none_of":["on hold"],"label_presence":"none"
      }))
      .unwrap(),
    );
    let handlers = fixture
      .handlers()
      .with_selection_labels(
        fixture.capability.clone(),
        WorkSourceSelectionLabels {
          label_presence: Default::default(),
          state: key("any"),
          all_of: vec![],
          any_of: vec![],
          none_of: vec![],
        },
      )
      .unwrap()
      .with_predicate_states(fixture.capability.clone(), key("any"), vec![key("todo")])
      .unwrap()
      .with_predicate_label_presence(fixture.capability.clone())
      .unwrap()
      .with_preview(Arc::new(Reader));
    let result = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers))
      .handle_query(
        &context(),
        PreviewWorkSourcesQuery {
          intent,
          limit: 2,
          observed_at: time(4),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(&result.selectors[0].selection).unwrap(),
      serde_json::json!({
        "state":"todo","all_of":[],"any_of":[],"none_of":["on hold"],"label_presence":"none"
      })
    );
  });
}

#[test]
fn preview_presents_the_effective_custom_predicate_instead_of_the_installed_base() {
  run_ready(async {
    let fixture = Fixture::new("custom-tracker").await;
    let mut intent = fixture.intent.clone();
    intent.inputs[0].definition.predicate = Some(octacity_server_factory::WorkSourcePredicate {
      label_presence: Default::default(),
      state: Some(key("todo")),
      all_of: vec!["literal, label".into()],
      any_of: vec![],
      none_of: vec!["on hold".into()],
    });
    let handlers = fixture
      .handlers()
      .with_selection_labels(
        fixture.capability.clone(),
        WorkSourceSelectionLabels {
          label_presence: Default::default(),
          state: key("any"),
          all_of: vec![],
          any_of: vec![],
          none_of: vec![],
        },
      )
      .unwrap()
      .with_predicate_states(fixture.capability.clone(), key("any"), vec![key("todo")])
      .unwrap()
      .with_preview(Arc::new(Reader));
    let result = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers))
      .handle_query(
        &context(),
        PreviewWorkSourcesQuery {
          intent,
          limit: 2,
          observed_at: time(4),
        },
      )
      .await
      .unwrap();
    assert_eq!(
      result.selectors[0].selection,
      Some(WorkSourceSelectionLabels {
        label_presence: Default::default(),
        state: key("todo"),
        all_of: vec!["literal, label".into()],
        any_of: vec![],
        none_of: vec!["on hold".into()],
      })
    );
  });
}
