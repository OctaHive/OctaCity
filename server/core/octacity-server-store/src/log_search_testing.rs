//! Deterministic in-memory Build-log search adapter.

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{BuildId, LogChunkId, LogIndexingWorkId, ProjectId};

use crate::{
  DeleteLogSearchDocuments, IndexedLogSearchPage, LogIndexPosition, LogSearchCursor, LogSearchDocument, LogSearchError,
  LogSearchHit, LogSearchIndex, LogSearchMode, LogSearchMutationDisposition, LogSearchOperation, LogSearchQuery,
  WriteLogSearchDocument, bounded_log_search_snippet,
};

/// Deterministic process-local log-search projection for application tests.
#[derive(Default)]
pub struct InMemoryLogSearchIndex {
  state: Mutex<LogSearchMemoryState>,
}

impl InMemoryLogSearchIndex {
  /// Creates an empty deterministic projection.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  fn lock(&self) -> Result<MutexGuard<'_, LogSearchMemoryState>, LogSearchError> {
    self.state.lock().map_err(|_| LogSearchError::Unavailable)
  }
}

#[derive(Default)]
struct LogSearchMemoryState {
  documents: BTreeMap<LogChunkId, LogSearchDocument>,
  work: BTreeMap<LogIndexingWorkId, RecordedWork>,
  projects: BTreeMap<ProjectId, ProjectIndexState>,
}

#[derive(Default)]
struct ProjectIndexState {
  applied_positions: BTreeSet<LogIndexPosition>,
  indexed_through: Option<LogIndexPosition>,
  deleted_builds: BTreeMap<BuildId, LogIndexPosition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordedWork {
  Index(WriteLogSearchDocument),
  Delete(DeleteLogSearchDocuments),
  Rebuild(WriteLogSearchDocument),
}

#[async_trait]
impl LogSearchIndex for InMemoryLogSearchIndex {
  async fn search(&self, query: LogSearchQuery) -> Result<IndexedLogSearchPage, LogSearchError> {
    query
      .validate()
      .map_err(|source| LogSearchError::invalid(LogSearchOperation::Search, source))?;
    let state = self.lock()?;
    let mut documents: Vec<_> = state
      .documents
      .values()
      .filter(|document| matches_query(document, &query))
      .collect();
    documents.sort_unstable_by_key(|document| std::cmp::Reverse((document.occurred_at, document.chunk_id)));
    if let Some(after) = query.after {
      documents.retain(|document| (document.occurred_at, document.chunk_id) < (after.occurred_at, after.chunk_id));
    }

    let has_more = documents.len() > usize::from(query.limit);
    documents.truncate(usize::from(query.limit));
    let hits: Vec<_> = documents.into_iter().map(|document| hit(document, &query)).collect();
    let next_cursor = has_more.then(|| {
      let last = hits.last().expect("a page with more results cannot be empty");
      LogSearchCursor {
        occurred_at: last.occurred_at,
        chunk_id: last.chunk_id,
      }
    });
    Ok(IndexedLogSearchPage {
      hits,
      next_cursor,
      indexed_through: indexed_through(&state, query.project_id),
    })
  }

  async fn index(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
    let mut state = self.lock()?;
    apply_document(&mut state, RecordedWork::Index(request), LogSearchOperation::Index)
  }

  async fn indexed_through(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, LogSearchError> {
    let state = self.lock()?;
    Ok(indexed_through(&state, project_id))
  }

  async fn delete(&self, request: DeleteLogSearchDocuments) -> Result<LogSearchMutationDisposition, LogSearchError> {
    let mut state = self.lock()?;
    let work = RecordedWork::Delete(request);
    if let Some(disposition) = replay_disposition(&state, request.work_id, &work)? {
      return Ok(disposition);
    }
    state
      .documents
      .retain(|_, document| document.project_id != request.project_id || document.build_id != request.build_id);
    let project = state.projects.entry(request.project_id).or_default();
    project
      .deleted_builds
      .entry(request.build_id)
      .and_modify(|position| *position = (*position).max(request.position))
      .or_insert(request.position);
    advance_freshness(project, request.position);
    state.work.insert(request.work_id, work);
    Ok(LogSearchMutationDisposition::Applied)
  }

  async fn rebuild(&self, request: WriteLogSearchDocument) -> Result<LogSearchMutationDisposition, LogSearchError> {
    let mut state = self.lock()?;
    apply_document(&mut state, RecordedWork::Rebuild(request), LogSearchOperation::Rebuild)
  }
}

fn apply_document(
  state: &mut LogSearchMemoryState,
  work: RecordedWork,
  operation: LogSearchOperation,
) -> Result<LogSearchMutationDisposition, LogSearchError> {
  let request = match &work {
    RecordedWork::Index(request) | RecordedWork::Rebuild(request) => request,
    RecordedWork::Delete(_) => unreachable!("document writes cannot contain deletion work"),
  };
  request
    .document
    .validate()
    .map_err(|source| LogSearchError::invalid(operation, source))?;
  if let Some(disposition) = replay_disposition(state, request.work_id, &work)? {
    return Ok(disposition);
  }
  let project = state.projects.entry(request.document.project_id).or_default();
  if project.deleted_builds.contains_key(&request.document.build_id) {
    advance_freshness(project, request.position);
    state.work.insert(request.work_id, work);
    return Ok(LogSearchMutationDisposition::Superseded);
  }
  if state
    .documents
    .get(&request.document.chunk_id)
    .is_some_and(|existing| existing != &request.document)
  {
    return Err(LogSearchError::DocumentConflict {
      chunk_id: request.document.chunk_id,
    });
  }
  state
    .documents
    .insert(request.document.chunk_id, request.document.clone());
  advance_freshness(project, request.position);
  state.work.insert(request.work_id, work);
  Ok(LogSearchMutationDisposition::Applied)
}

fn replay_disposition(
  state: &LogSearchMemoryState,
  work_id: LogIndexingWorkId,
  request: &RecordedWork,
) -> Result<Option<LogSearchMutationDisposition>, LogSearchError> {
  match state.work.get(&work_id) {
    Some(existing) if existing == request => Ok(Some(LogSearchMutationDisposition::Replayed)),
    Some(_) => Err(LogSearchError::WorkConflict { work_id }),
    None => Ok(None),
  }
}

fn advance_freshness(project: &mut ProjectIndexState, position: LogIndexPosition) {
  project.applied_positions.insert(position);
  let mut next = project
    .indexed_through
    .map_or(1, |current| current.get().saturating_add(1));
  while let Ok(position) = LogIndexPosition::new(next) {
    if !project.applied_positions.remove(&position) {
      break;
    }
    project.indexed_through = Some(position);
    next = match next.checked_add(1) {
      Some(next) => next,
      None => break,
    };
  }
}

fn indexed_through(state: &LogSearchMemoryState, project_id: ProjectId) -> Option<LogIndexPosition> {
  state
    .projects
    .get(&project_id)
    .and_then(|project| project.indexed_through)
}

fn matches_query(document: &LogSearchDocument, query: &LogSearchQuery) -> bool {
  document.project_id == query.project_id
    && query.build_id.is_none_or(|build_id| document.build_id == build_id)
    && query
      .attempt_id
      .is_none_or(|attempt_id| document.attempt_id == attempt_id)
    && query.job_id.is_none_or(|job_id| document.job_id == job_id)
    && query.stream.is_none_or(|stream| document.stream == stream)
    && query.occurred_from.is_none_or(|from| document.occurred_at >= from)
    && query
      .occurred_through
      .is_none_or(|through| document.occurred_at <= through)
    && text_matches(&document.redacted_text, &query.text, query.mode)
}

fn text_matches(document: &str, query: &str, mode: LogSearchMode) -> bool {
  match mode {
    LogSearchMode::Literal => document.contains(query),
    LogSearchMode::FullText => {
      let document = document.to_lowercase();
      query
        .split_whitespace()
        .map(str::to_lowercase)
        .all(|term| document.contains(&term))
    }
  }
}

fn hit(document: &LogSearchDocument, query: &LogSearchQuery) -> LogSearchHit {
  LogSearchHit {
    chunk_id: document.chunk_id,
    build_id: document.build_id,
    attempt_id: document.attempt_id,
    job_id: document.job_id,
    stream: document.stream,
    first_sequence: document.first_sequence,
    last_sequence: document.last_sequence,
    occurred_at: document.occurred_at,
    snippet: bounded_log_search_snippet(&document.redacted_text, &query.text, query.mode),
  }
}

#[test]
fn contiguous_progress_discards_positions_below_the_watermark() {
  let mut project = ProjectIndexState::default();
  advance_freshness(&mut project, LogIndexPosition::new(2).unwrap());
  assert_eq!(project.applied_positions.len(), 1);

  advance_freshness(&mut project, LogIndexPosition::new(1).unwrap());
  assert_eq!(project.indexed_through, LogIndexPosition::new(2).ok());
  assert!(project.applied_positions.is_empty());
}
