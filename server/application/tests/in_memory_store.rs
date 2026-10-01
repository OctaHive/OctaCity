use std::{
  future::Future,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

use octacity_server_application::{
  BuildLogSearch, ManagementAuthorizationGrant, ManagementResource, ManagementResourceIdentity, ManagementResourceKind,
  ManagementVisibility, SearchBuildLogsQuery,
};
use octacity_server_domain::{AttemptId, BuildId, JobId, LogChunkId, LogIndexingWorkId, ProjectId, Timestamp};
use octacity_server_store::{
  BuildLogStream, LogIndexPosition, LogIndexWorkStore, LogSearchCriteria, LogSearchDocument, LogSearchIndex as _,
  LogSearchMode, StoreError, WriteLogSearchDocument,
  testing::{
    InMemoryLogSearchIndex, InMemoryStore, verify_in_memory_log_search_index_contract, verify_in_memory_store_contract,
  },
};

struct CountingWatermarkStore {
  reads: AtomicUsize,
  watermark: Option<LogIndexPosition>,
}

#[async_trait::async_trait]
impl LogIndexWorkStore for CountingWatermarkStore {
  async fn committed_log_index_position(&self, _project_id: ProjectId) -> Result<Option<LogIndexPosition>, StoreError> {
    self.reads.fetch_add(1, Ordering::SeqCst);
    Ok(self.watermark)
  }
}

#[path = "support/management_query.rs"]
mod management_query_support;
use management_query_support::{management_query, management_query_with_grant};

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
    let page = management_query(
      &service,
      SearchBuildLogsQuery {
        search: LogSearchCriteria {
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
      },
    )
    .await
    .unwrap();

    assert_eq!(page.freshness.indexed_through, None);
    assert_eq!(page.freshness.committed_through, Some(2));
    assert!(!page.freshness.caught_up);
  });
}

#[test]
fn hidden_log_search_does_not_disclose_authoritative_freshness() {
  run_ready(async {
    let project_id = id(20);
    let other_project_id = id::<ProjectId>(21);
    let work = Arc::new(CountingWatermarkStore {
      reads: AtomicUsize::new(0),
      watermark: LogIndexPosition::new(7).ok(),
    });
    let service = BuildLogSearch::new(Arc::clone(&work), Arc::new(InMemoryLogSearchIndex::new()));
    let query = SearchBuildLogsQuery {
      search: LogSearchCriteria {
        project_id,
        text: "hidden".to_owned(),
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
    };
    let other_project = ManagementResource::instance(
      ManagementResourceKind::Project,
      ManagementResourceIdentity::new(other_project_id.to_string()).unwrap(),
    )
    .unwrap();

    for visibility in [
      ManagementVisibility::none(),
      ManagementVisibility::restricted([other_project]).unwrap(),
    ] {
      let page = management_query_with_grant(&service, query.clone(), ManagementAuthorizationGrant::new(visibility))
        .await
        .unwrap();
      assert!(page.items.is_empty());
      assert_eq!(page.next_cursor, None);
      assert_eq!(page.freshness.indexed_through, None);
      assert_eq!(page.freshness.committed_through, None);
      assert!(page.freshness.caught_up);
    }
    assert_eq!(work.reads.load(Ordering::SeqCst), 0);
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
