//! Both Source installations share the ordinary authorized, owner-aware material readers.
use octacity_artifact_store::{ArtifactBytePublisher, ArtifactByteReader};
use octacity_server_application::*;
use octacity_server_store_postgres::PostgresStore;
use std::sync::Arc;

pub(in crate::runtime) fn reader<B: ArtifactBytePublisher + ArtifactByteReader + 'static>(
  store: Arc<PostgresStore>,
  bytes: Arc<B>,
  revisions: Arc<dyn RevisionResolver>,
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  clock: Arc<dyn FactoryExecutionClock>,
) -> Arc<dyn FactorySourceIssueSnapshots> {
  let documents = Arc::new(
    ProjectTextIntake::new(store.clone(), bytes, revisions, policy.clone(), clock.clone())
      .with_issue_documents(store.clone()),
  );
  let comments = Arc::new(IssueDiscussionHandlers::new(store.clone(), policy.clone(), clock));
  let materials = IssueSnapshotMaterialReaders::new(Arc::new(AuthorizedQueryHandler::new(policy.clone(), comments)))
    .with_documents(
      Arc::new(AuthorizedQueryHandler::new(policy.clone(), documents.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy.clone(), documents)),
      store.clone(),
    );
  Arc::new(IssueSourceSnapshotReader::new(store, policy).with_materials(Arc::new(materials)))
}
