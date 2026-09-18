use std::{
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
};

use octacity_server_application::{BuildLogSearch, QueryHandler, SearchBuildLogsQuery};
use octacity_server_domain::{AttemptId, BuildId, JobId, LogChunkId, LogIndexingWorkId, ProjectId, Timestamp};
use octacity_server_store::{
  BuildLogStream, LogIndexPosition, LogSearchDocument, LogSearchIndex as _, LogSearchMode, LogSearchQuery,
  WriteLogSearchDocument,
  testing::{
    InMemoryLogSearchIndex, InMemoryStore, verify_in_memory_log_search_index_contract, verify_in_memory_store_contract,
  },
};

#[test]
fn application_tests_use_the_store_port_without_external_services() {
  verify_in_memory_store_contract();
}

#[test]
fn application_tests_use_the_log_search_port_without_external_services() {
  verify_in_memory_log_search_index_contract();
}

#[test]
fn log_search_freshness_uses_the_authoritative_watermark() {
  run_ready(async {
    let project_id = id(1);
    let work = Arc::new(InMemoryStore::new());
    work
      .seed_committed_log_index_position(project_id, LogIndexPosition::new(2).unwrap())
      .unwrap();
    let index = Arc::new(InMemoryLogSearchIndex::new());
    index
      .index(WriteLogSearchDocument {
        work_id: id(2),
        position: LogIndexPosition::new(2).unwrap(),
        document: LogSearchDocument {
          chunk_id: id(3),
          project_id,
          build_id: id(4),
          attempt_id: id(5),
          job_id: id(6),
          stream: BuildLogStream::Stdout,
          first_sequence: 1,
          last_sequence: 1,
          occurred_at: Timestamp::from_unix_millis(1).unwrap(),
          redacted_text: "later".to_owned(),
        },
      })
      .await
      .unwrap();
    let service = BuildLogSearch::new(work, index);
    let page = service
      .handle_query(SearchBuildLogsQuery {
        search: LogSearchQuery {
          project_id,
          text: "later".to_owned(),
          mode: LogSearchMode::Literal,
          build_id: None,
          attempt_id: None,
          job_id: None,
          stream: None,
          occurred_from: None,
          occurred_through: None,
          after: None,
          limit: 10,
        },
      })
      .await
      .unwrap();

    assert_eq!(page.freshness.indexed_through, None);
    assert_eq!(page.freshness.committed_through, LogIndexPosition::new(2).ok());
    assert!(!page.freshness.is_caught_up());
  });
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory application test unexpectedly waited for external I/O"),
  }
}

fn id<T>(value: u128) -> T
where
  T: FromUuid,
{
  T::from_uuid(uuid::Uuid::from_u128(value))
}

trait FromUuid {
  fn from_uuid(value: uuid::Uuid) -> Self;
}

macro_rules! impl_from_uuid {
  ($($type:ty),+ $(,)?) => {$(
    impl FromUuid for $type {
      fn from_uuid(value: uuid::Uuid) -> Self {
        <$type>::from_uuid(value).unwrap()
      }
    }
  )+};
}

impl_from_uuid!(ProjectId, BuildId, AttemptId, JobId, LogChunkId, LogIndexingWorkId);
