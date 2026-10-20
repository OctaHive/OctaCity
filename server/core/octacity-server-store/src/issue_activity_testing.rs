//! Timeline metadata shares the existing atomic Issue memory snapshot.
use super::*;
#[async_trait::async_trait]
impl IssueTimelineStore for crate::testing::InMemoryProjectStore {
  async fn issue_timeline(&self, request: ReadIssueTimeline) -> Result<IssueTimelineReadPage, StoreError> {
    request.validate()?;
    let state = self.lock()?;
    let memory = &state.issues;
    let issue = memory
      .current
      .get(&(request.project_id, request.issue_id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let maximum = memory.discussion.lifetime.get(&request.issue_id).copied().unwrap_or(0);
    let executions = memory
      .executions
      .requests
      .values()
      .filter(|(_, _, r)| r.intent.issue_id == issue.id)
      .map(|(_, s, _)| *s)
      .max()
      .unwrap_or(0);
    request.validate_boundary(issue.version, executions, maximum)?;
    let native = memory
      .sources
      .links
      .get(&issue.id)
      .filter(|(scope, _)| scope == &request.scope)
      .map(|(_, link)| link.native_correlation);
    let source_count = if native.is_some() {
      memory
        .sources
        .observations
        .keys()
        .filter(|(id, _)| *id == issue.id)
        .count() as u64
    } else {
      0
    };
    let generation = if native.is_some() {
      memory.sources.states.get(&issue.id).map(|s| s.generation).unwrap_or(0)
    } else {
      0
    };
    let mapping = if native.is_some() {
      memory
        .sources
        .mappings
        .keys()
        .filter(|(id, _)| *id == issue.id)
        .map(|(_, v)| v.get())
        .max()
        .unwrap_or(0)
    } else {
      0
    };
    let source = issue_timeline_source_watermark(native, source_count, generation, mapping)?;
    let boundary = request.boundary.unwrap_or(IssueTimelineBoundary {
      issue_version: issue.version,
      executions,
      discussion: maximum,
      source: Some(source),
    });
    let mut rows = Vec::new();
    for ((id, version), revision) in &memory.revisions {
      if *id == issue.id && *version <= boundary.issue_version {
        rows.push(IssueTimelineReference::Revision {
          at: revision.updated_at,
          version: *version,
        });
      }
    }
    for ((id, version), entry) in &memory.history {
      if *id == issue.id && *version <= boundary.issue_version {
        rows.push(IssueTimelineReference::Status {
          at: entry.occurred_at,
          version: *version,
          from: entry.from,
          to: entry.to,
          actor: entry.actor.clone(),
        });
      }
    }
    for (_, sequence, execution) in memory.executions.requests.values() {
      if execution.intent.issue_id == issue.id && *sequence <= boundary.executions {
        rows.push(IssueTimelineReference::Execution {
          at: execution.requested_at,
          request_id: execution.id,
          requirements_version: execution.intent.expected_version,
        });
      }
    }
    for ((id, sequence), (comment, _)) in &memory.discussion.comments {
      if *id == issue.id && *sequence <= boundary.discussion {
        rows.push(IssueTimelineReference::Discussion {
          comment: comment.clone(),
        });
      }
    }
    let source_stale = boundary.source.is_some_and(|original| original != source);
    if !source_stale && boundary.source.is_some() && native.is_some() {
      for ((id, receipt), observation) in &memory.sources.observations {
        if *id == issue.id {
          rows.push(IssueTimelineReference::Source {
            at: observation.observed_at,
            id: *receipt,
            expires_at: observation.expires_at,
          });
        }
      }
    }
    Ok(IssueTimelineReadPage::bounded(
      rows,
      boundary,
      issue.version,
      source_stale,
      boundary.source.is_none(),
      &request,
    ))
  }
}
