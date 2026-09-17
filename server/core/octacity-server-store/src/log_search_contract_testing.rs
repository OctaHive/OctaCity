//! Reusable behavioral contract for Build-log search adapters.

use std::sync::Arc;

use octacity_server_domain::{BuildId, ProjectId, Timestamp};

use crate::log_search_testing::InMemoryLogSearchIndex;
use crate::test_support::{id, run_ready};
use crate::{
  BuildLogStream, DeleteLogSearchDocuments, LogIndexPosition, LogSearchDocument, LogSearchError, LogSearchIndex,
  LogSearchMode, LogSearchMutationDisposition, LogSearchOperation, LogSearchQuery, MAX_LOG_SEARCH_SNIPPET_BYTES,
  WriteLogSearchDocument,
};

/// Runs the reusable backend-neutral behavioral contract against one empty adapter.
pub async fn verify_log_search_index_contract<I>(index: Arc<I>)
where
  I: LogSearchIndex + 'static,
{
  let project_id = id::<ProjectId>(1);
  let other_project_id = id::<ProjectId>(2);
  let build_id = id::<BuildId>(10);
  let first = document(
    100,
    project_id,
    build_id,
    1_000,
    BuildLogStream::Stdout,
    "Build compiler failed at [REDACTED]",
  );
  let second = document(
    101,
    project_id,
    build_id,
    2_000,
    BuildLogStream::Stderr,
    "Build error at /workspace/src/main.rs:42",
  );
  let isolated = document(
    102,
    other_project_id,
    id::<BuildId>(11),
    3_000,
    BuildLogStream::Stderr,
    "Build compiler failed outside the project",
  );

  let first_write = write(1, 1, first.clone());
  assert_eq!(
    index.index(first_write.clone()).await.unwrap(),
    LogSearchMutationDisposition::Applied
  );
  assert_eq!(
    index.index(first_write.clone()).await.unwrap(),
    LogSearchMutationDisposition::Replayed
  );
  let mut conflicting = first_write.clone();
  conflicting.document.redacted_text.push_str(" changed");
  assert_eq!(
    index.index(conflicting).await.unwrap_err(),
    LogSearchError::WorkConflict {
      work_id: first_write.work_id
    }
  );
  assert_eq!(
    index.index(write(2, 2, second.clone())).await.unwrap(),
    LogSearchMutationDisposition::Applied
  );
  assert_eq!(
    index.index(write(3, 1, isolated)).await.unwrap(),
    LogSearchMutationDisposition::Applied
  );

  let mut invalid_query = query(project_id, "compiler", LogSearchMode::FullText, 0);
  assert!(matches!(
    index.search(invalid_query.clone()).await.unwrap_err(),
    LogSearchError::InvalidInput {
      operation: LogSearchOperation::Search,
      ..
    }
  ));
  invalid_query.limit = 10;
  assert_eq!(index.search(invalid_query).await.unwrap().hits.len(), 1);

  let mut invalid_write = write(6, 2, first.clone());
  invalid_write.document.first_sequence = 0;
  assert!(matches!(
    index.index(invalid_write).await.unwrap_err(),
    LogSearchError::InvalidInput {
      operation: LogSearchOperation::Index,
      ..
    }
  ));
  assert_eq!(
    index.index(write(6, 2, first.clone())).await.unwrap(),
    LogSearchMutationDisposition::Applied,
    "rejected work must not consume its idempotency identity"
  );
  let mut changed_chunk = first.clone();
  changed_chunk.redacted_text.push_str(" changed");
  assert_eq!(
    index.index(write(7, 3, changed_chunk)).await.unwrap_err(),
    LogSearchError::DocumentConflict {
      chunk_id: first.chunk_id
    },
    "an immutable chunk identity cannot be rebound to different content"
  );

  let terms = query(project_id, "compiler failed", LogSearchMode::FullText, 10);
  let page = index.search(terms).await.unwrap();
  assert_eq!(page.hits.len(), 1, "Project scope must isolate identical terms");
  assert_eq!(page.hits[0].chunk_id, first.chunk_id);
  assert!(page.hits[0].snippet.contains("[REDACTED]"));
  assert!(page.hits[0].snippet.len() <= MAX_LOG_SEARCH_SNIPPET_BYTES);
  assert_eq!((page.hits[0].first_sequence, page.hits[0].last_sequence), (1, 10));
  assert_eq!(page.indexed_through.unwrap().get(), 2);

  let mut literal = query(project_id, "/workspace/src/main.rs:42", LogSearchMode::Literal, 10);
  literal.stream = Some(BuildLogStream::Stderr);
  assert_eq!(index.search(literal).await.unwrap().hits[0].chunk_id, second.chunk_id);

  let first_page = index
    .search(query(project_id, "Build", LogSearchMode::Literal, 1))
    .await
    .unwrap();
  assert_eq!(first_page.hits[0].chunk_id, second.chunk_id);
  let mut following = query(project_id, "Build", LogSearchMode::Literal, 1);
  following.after = first_page.next_cursor;
  let following = index.search(following).await.unwrap();
  assert_eq!(following.hits[0].chunk_id, first.chunk_id);
  assert!(following.next_cursor.is_none());

  let deletion = DeleteLogSearchDocuments {
    work_id: id(4),
    position: LogIndexPosition::new(3).unwrap(),
    project_id,
    build_id,
  };
  assert_eq!(
    index.delete(deletion).await.unwrap(),
    LogSearchMutationDisposition::Applied
  );
  assert_eq!(
    index.delete(deletion).await.unwrap(),
    LogSearchMutationDisposition::Replayed
  );
  assert!(
    index
      .search(query(project_id, "Build", LogSearchMode::Literal, 10))
      .await
      .unwrap()
      .hits
      .is_empty()
  );

  let rebuild = write(5, 4, first.clone());
  assert_eq!(
    index.rebuild(rebuild.clone()).await.unwrap(),
    LogSearchMutationDisposition::Superseded
  );
  assert_eq!(
    index.rebuild(rebuild).await.unwrap(),
    LogSearchMutationDisposition::Replayed
  );
  assert!(
    index
      .search(query(project_id, "compiler", LogSearchMode::FullText, 10))
      .await
      .unwrap()
      .hits
      .is_empty(),
    "rebuild must not resurrect documents hidden by Build retention"
  );
  assert_eq!(index.indexed_through(project_id).await.unwrap().unwrap().get(), 4);

  let gap_project = id::<ProjectId>(9);
  let gap_build = id::<BuildId>(90);
  assert_eq!(
    index
      .index(write(
        8,
        2,
        document(108, gap_project, gap_build, 1, BuildLogStream::Stdout, "later"),
      ))
      .await
      .unwrap(),
    LogSearchMutationDisposition::Applied
  );
  assert_eq!(index.indexed_through(gap_project).await.unwrap(), None);
  assert_eq!(
    index
      .index(write(
        9,
        1,
        document(109, gap_project, gap_build, 0, BuildLogStream::Stdout, "earlier"),
      ))
      .await
      .unwrap(),
    LogSearchMutationDisposition::Applied
  );
  assert_eq!(
    index.indexed_through(gap_project).await.unwrap(),
    LogIndexPosition::new(2).ok(),
    "freshness advances only after every preceding project position is applied"
  );
}

/// Runs the complete contract against a new in-memory adapter without an async runtime.
pub fn verify_in_memory_log_search_index_contract() {
  run_ready(
    verify_log_search_index_contract(Arc::new(InMemoryLogSearchIndex::new())),
    "the in-memory log-search adapter unexpectedly yielded to an external runtime",
  );
}

fn document(
  value: u64,
  project_id: ProjectId,
  build_id: BuildId,
  occurred_at: i64,
  stream: BuildLogStream,
  redacted_text: &str,
) -> LogSearchDocument {
  LogSearchDocument {
    chunk_id: id(value),
    project_id,
    build_id,
    attempt_id: id(value + 1_000),
    job_id: id(value + 2_000),
    stream,
    first_sequence: 1,
    last_sequence: 10,
    occurred_at: Timestamp::from_unix_millis(occurred_at).unwrap(),
    redacted_text: redacted_text.to_owned(),
  }
}

fn write(work: u64, position: u64, document: LogSearchDocument) -> WriteLogSearchDocument {
  WriteLogSearchDocument {
    work_id: id(work),
    position: LogIndexPosition::new(position).unwrap(),
    document,
  }
}

fn query(project_id: ProjectId, text: &str, mode: LogSearchMode, limit: u16) -> LogSearchQuery {
  LogSearchQuery {
    project_id,
    text: text.to_owned(),
    mode,
    build_id: None,
    attempt_id: None,
    job_id: None,
    stream: None,
    occurred_from: None,
    occurred_through: None,
    after: None,
    limit,
  }
}
