//! Bounded-lifetime request correlation, isolated by verified connection and session.
use std::{collections::BTreeMap, sync::Mutex};
use tokio::sync::oneshot;

const MAX_ACTIVE_READS: usize = 64;
pub(super) enum Refusal {
  Duplicate,
  Capacity,
  SessionUnavailable,
}

const MAX_SESSIONS: usize = 1024;
const MAX_SESSIONS_PER_CONNECTION: usize = 64;
const SESSION_LIFETIME: std::time::Duration = std::time::Duration::from_secs(30 * 60);
struct Session {
  scope: String,
  expires: tokio::time::Instant,
}
type Key = (String, String, String);
type Entries = BTreeMap<Key, Option<oneshot::Sender<()>>>;
#[derive(Default)]
struct Correlation {
  reads: Entries,
  sessions: BTreeMap<String, Session>,
}
impl Correlation {
  fn current_session(&self, scope: &str, id: &str) -> bool {
    self
      .sessions
      .get(id)
      .is_some_and(|session| session.scope == scope && session.expires > tokio::time::Instant::now())
  }
}
#[derive(Default)]
pub(super) struct PendingReads(Mutex<Correlation>);
impl PendingReads {
  // Protocol correlation only: no token material, principal or authorization grant is retained.
  pub fn initialize(&self, scope: &str) -> Option<String> {
    let mut state = self.0.lock().expect("MCP correlation");
    let now = tokio::time::Instant::now();
    state.sessions.retain(|_, session| session.expires > now);
    if state.sessions.len() >= MAX_SESSIONS
      || state.sessions.values().filter(|session| session.scope == scope).count() >= MAX_SESSIONS_PER_CONNECTION
    {
      return None;
    }
    let id = uuid::Uuid::new_v4().to_string();
    state.sessions.insert(
      id.clone(),
      Session {
        scope: scope.to_owned(),
        expires: now + SESSION_LIFETIME,
      },
    );
    Some(id)
  }
  pub fn accepts_session(&self, scope: &str, id: &str) -> bool {
    self.0.lock().expect("MCP correlation").current_session(scope, id)
  }
  pub fn terminate(&self, scope: &str, id: &str) -> bool {
    let mut state = self.0.lock().expect("MCP correlation");
    if !state.current_session(scope, id) {
      return false;
    }
    state.sessions.remove(id);
    for ((owner, session, _), sender) in &mut state.reads {
      if owner == scope
        && session == id
        && let Some(sender) = sender.take()
      {
        let _ = sender.send(());
      }
    }
    true
  }
  pub fn register(
    &self,
    scope: &str,
    session: &str,
    id: &serde_json::Value,
  ) -> Result<(Registration<'_>, oneshot::Receiver<()>), Refusal> {
    let key = (scope.to_owned(), session.to_owned(), id.to_string());
    let mut state = self.0.lock().expect("MCP correlation");
    // Admission and termination share this lock: a closed session cannot
    // register a read after its HTTP body finished arriving.
    if !session.is_empty() && !state.current_session(scope, session) {
      return Err(Refusal::SessionUnavailable);
    }
    if state.reads.contains_key(&key) {
      return Err(Refusal::Duplicate);
    }
    if state.reads.len() >= MAX_ACTIVE_READS {
      return Err(Refusal::Capacity);
    }
    let (sender, receiver) = oneshot::channel();
    state.reads.insert(key.clone(), Some(sender));
    Ok((Registration { pending: self, key }, receiver))
  }
  pub fn cancel(&self, scope: &str, session: &str, id: &serde_json::Value) {
    if let Some(sender) = self
      .0
      .lock()
      .expect("MCP correlation")
      .reads
      .get_mut(&(scope.to_owned(), session.to_owned(), id.to_string()))
      .and_then(Option::take)
    {
      let _ = sender.send(());
    }
  }
}
pub(super) struct Registration<'a> {
  pending: &'a PendingReads,
  key: Key,
}
impl Drop for Registration<'_> {
  fn drop(&mut self) {
    self.pending.0.lock().expect("MCP correlation").reads.remove(&self.key);
  }
}
