use std::{
  collections::VecDeque,
  future::Future,
  sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
  time::Duration,
};

use async_trait::async_trait;
use octacity_server_application::{
  JobEventLongPoll, JobEventWaiter, ManagementAuthorizationGrant, ManagementResource, ManagementResourceIdentity,
  ManagementResourceKind, ManagementVisibility, ReadJobEventsQuery,
};
use octacity_server_domain::{JobId, Timestamp};
use octacity_server_store::{
  DurableJobEvent, EventSequence, JobEventKind, JobEventPage, JobEventReadStore, ReadJobEvents, StoreError,
};
use serde_json::json;

#[path = "support/management_query.rs"]
mod management_query_support;
use management_query_support::{management_query, management_query_with_grant};

struct ScriptedStore {
  pages: Mutex<VecDeque<JobEventPage>>,
  active_reads: AtomicUsize,
  reads: AtomicUsize,
  requests: Mutex<Vec<ReadJobEvents>>,
}

impl ScriptedStore {
  fn new(pages: impl IntoIterator<Item = JobEventPage>) -> Self {
    Self {
      pages: Mutex::new(pages.into_iter().collect()),
      active_reads: AtomicUsize::new(0),
      reads: AtomicUsize::new(0),
      requests: Mutex::new(Vec::new()),
    }
  }
}

#[async_trait]
impl JobEventReadStore for ScriptedStore {
  async fn read_job_events(&self, request: ReadJobEvents) -> Result<JobEventPage, StoreError> {
    self.active_reads.fetch_add(1, Ordering::SeqCst);
    self.reads.fetch_add(1, Ordering::SeqCst);
    self.requests.lock().unwrap().push(request);
    let page = self.pages.lock().unwrap().pop_front().unwrap();
    self.active_reads.fetch_sub(1, Ordering::SeqCst);
    Ok(page)
  }
}

struct ObservingWaiter {
  store: Arc<ScriptedStore>,
  waits: AtomicUsize,
}

#[async_trait]
impl JobEventWaiter for ObservingWaiter {
  async fn wait_for_job_events(&self, _job_identity: &str, _observed_cursor: u64, _timeout: Duration) {
    assert_eq!(self.store.active_reads.load(Ordering::SeqCst), 0);
    self.waits.fetch_add(1, Ordering::SeqCst);
  }
}

#[test]
fn returns_ordered_durable_replay_without_waiting() {
  let store = Arc::new(ScriptedStore::new([page(&[1, 2, 3], 3)]));
  let waiter = Arc::new(ObservingWaiter {
    store: Arc::clone(&store),
    waits: AtomicUsize::new(0),
  });
  let service = JobEventLongPoll::new(Arc::clone(&store), Arc::clone(&waiter));

  let result = run_ready(management_query(&service, query(Duration::from_secs(10)))).unwrap();

  assert_eq!(
    result.events.iter().map(|event| event.sequence).collect::<Vec<_>>(),
    [1, 2, 3]
  );
  assert_eq!(result.cursor, 3);
  assert_eq!(store.reads.load(Ordering::SeqCst), 1);
  assert_eq!(waiter.waits.load(Ordering::SeqCst), 0);
}

#[test]
fn restricted_visibility_reaches_the_store_and_the_page_is_not_post_filtered() {
  let store = Arc::new(ScriptedStore::new([page(&[1, 2], 2)]));
  let waiter = Arc::new(ObservingWaiter {
    store: Arc::clone(&store),
    waits: AtomicUsize::new(0),
  });
  let service = JobEventLongPoll::new(Arc::clone(&store), waiter);
  let job_id = query(Duration::ZERO).job_id;
  let resource = ManagementResource::instance(
    ManagementResourceKind::Job,
    ManagementResourceIdentity::new(job_id.to_string()).unwrap(),
  )
  .unwrap();
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::restricted([resource]).unwrap());

  let result = run_ready(management_query_with_grant(&service, query(Duration::ZERO), grant)).unwrap();

  assert_eq!(
    result.events.iter().map(|event| event.sequence).collect::<Vec<_>>(),
    [1, 2]
  );
  let requests = store.requests.lock().unwrap();
  assert_eq!(requests.len(), 1);
  assert!(requests[0].visibility.allows(&job_id));
  assert_eq!(
    requests[0].visibility.kind(),
    octacity_server_store::ReadVisibilityKind::Restricted
  );
}

#[test]
fn empty_timeout_releases_store_and_requeries_durable_state() {
  let store = Arc::new(ScriptedStore::new([page(&[], 7), page(&[], 7)]));
  let waiter = Arc::new(ObservingWaiter {
    store: Arc::clone(&store),
    waits: AtomicUsize::new(0),
  });
  let service = JobEventLongPoll::new(Arc::clone(&store), Arc::clone(&waiter));

  let result = run_ready(management_query(&service, query(Duration::from_secs(10)))).unwrap();

  assert!(result.events.is_empty());
  assert_eq!(result.cursor, 7);
  assert_eq!(store.reads.load(Ordering::SeqCst), 2);
  assert_eq!(waiter.waits.load(Ordering::SeqCst), 1);
}

#[test]
fn notification_loss_cannot_hide_an_event_committed_during_wait() {
  let store = Arc::new(ScriptedStore::new([page(&[], 0), page(&[1], 1)]));
  let waiter = Arc::new(ObservingWaiter {
    store: Arc::clone(&store),
    waits: AtomicUsize::new(0),
  });
  let service = JobEventLongPoll::new(store, waiter);

  let result = run_ready(management_query(&service, query(Duration::from_secs(10)))).unwrap();

  assert_eq!(result.events[0].sequence, 1);
  assert_eq!(result.cursor, 1);
}

#[test]
fn a_fresh_service_after_restart_reads_events_without_process_notifications() {
  let store = Arc::new(ScriptedStore::new([page(&[1, 2], 2)]));
  let waiter = Arc::new(ObservingWaiter {
    store: Arc::clone(&store),
    waits: AtomicUsize::new(0),
  });

  let restarted_service = JobEventLongPoll::new(store, Arc::clone(&waiter));
  let result = run_ready(management_query(&restarted_service, query(Duration::from_secs(10)))).unwrap();

  assert_eq!(
    result.events.iter().map(|event| event.sequence).collect::<Vec<_>>(),
    [1, 2]
  );
  assert_eq!(waiter.waits.load(Ordering::SeqCst), 0);
}

#[test]
fn rejects_an_unbounded_wait_before_reading_the_store() {
  let store = Arc::new(ScriptedStore::new([]));
  let waiter = Arc::new(ObservingWaiter {
    store: Arc::clone(&store),
    waits: AtomicUsize::new(0),
  });
  let service = JobEventLongPoll::new(Arc::clone(&store), waiter);

  assert!(run_ready(management_query(&service, query(Duration::from_secs(31)))).is_err());
  assert_eq!(store.reads.load(Ordering::SeqCst), 0);
}

fn query(wait: Duration) -> ReadJobEventsQuery {
  ReadJobEventsQuery {
    job_id: JobId::from_uuid(uuid::Uuid::from_u128(1)).unwrap(),
    after_sequence: 0,
    limit: 10,
    wait,
  }
}

fn page(sequences: &[u64], cursor: u64) -> JobEventPage {
  JobEventPage {
    events: sequences
      .iter()
      .copied()
      .map(|sequence| {
        DurableJobEvent::new(
          EventSequence::new(sequence).unwrap(),
          JobEventKind::new("progress").unwrap(),
          Timestamp::from_unix_millis(i64::try_from(sequence).unwrap()).unwrap(),
          json!({"sequence": sequence}),
        )
        .unwrap()
      })
      .collect(),
    cursor,
  }
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("scripted long-poll dependency unexpectedly waited"),
  }
}
