//! Native association shares ordinary Issue locks, replay and safe publication.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueId, ProjectId};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgConnection, types::Json};

#[derive(serde::Deserialize, serde::Serialize)]
struct ExplicitRefreshReplay {
  explicit_read: FactoryDigest,
  outcome: IssueSourceRefreshOutcome,
}

#[async_trait::async_trait]
impl IssueSourceStore for PostgresStore {
  async fn issue_source_attachment_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &AttachIssueSourceIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceOutcome>, StoreError> {
    let identity = MutationIdentity::new_management_replay(
      scope,
      MutationKind::AttachIssueSource,
      key.to_string(),
      octacity_server_domain::Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Issue,
      intent,
    )?;
    crate::mutation::probe(self.pool(), &identity)
      .await?
      .map(replay)
      .transpose()
  }

  async fn issue_source_observation(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    id: FactoryDigest,
  ) -> Result<Option<IssueSourceObservation>, StoreError> {
    self.issue(project, issue, None).await?;
    let value:Option<Json<IssueSourceObservation>> = sqlx::query_scalar("SELECT o.document FROM issue_source_observations o JOIN issue_source_links l USING(project_id,security_scope,native_correlation) WHERE l.project_id=$1 AND l.issue_id=$2 AND l.security_scope=$3 AND o.id=$4")
      .bind(project.as_uuid()).bind(issue.as_uuid()).bind(scope.as_str()).bind(id.as_bytes().as_slice()).fetch_optional(self.pool()).await.map_err(unavailable)?;
    Ok(value.map(|Json(observation)| observation))
  }
  async fn import_issue_source(
    &self,
    request: ManagementMutation<ImportIssueSource>,
  ) -> Result<IssueSourceOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::ImportIssueSource,
      request.idempotency_key.to_string(),
      request.imported_at,
      EntityKind::Issue,
      &request.selection,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let outcome = replay(value)?;
        if let IssueSourceOutcome::Linked { link, .. } = &outcome {
          visible(&request.visibility, link.issue_id)?;
        }
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    request.observation.validate(
      &request.selection,
      request.imported_at,
      StoreOperation::ImportIssueSource,
    )?;
    crate::issue_workflow::lock_project(&mut tx, request.selection.project_id).await?;
    if let Some(claim) = &request.read_claim {
      crate::issue_source_read::require_claim(
        &mut tx,
        audit.security_scope(),
        claim,
        &request.selection,
        None,
        request.imported_at,
      )
      .await?;
    }
    let native = request.selection.native_correlation(audit.security_scope());
    let (link, disposition) =
      if let Some(link) = native_link(&mut tx, request.selection.project_id, audit.security_scope(), native).await? {
        visible(&request.visibility, link.issue_id)?;
        (link, MutationDisposition::Replayed)
      } else {
        visible(&request.visibility, request.id)?;
        let issue = crate::issue::create_initial_issue(
          &mut tx,
          &CreateIssue {
            id: request.id,
            assignees: Default::default(),
            intent: CreateIssueIntent {
              project_id: request.selection.project_id,
              content: request.content,
              planning: Default::default(),
            },
            idempotency_key: request.idempotency_key,
            created_at: request.imported_at,
          },
          &audit,
          IssueTransitionOrigin::Source,
        )
        .await?;
        let link = IssueSourceLink {
          issue_id: issue.id,
          selection: request.selection,
          native_correlation: native,
          linked_at: request.imported_at,
        };
        insert_link(&mut tx, &link, audit.security_scope()).await?;
        (link, MutationDisposition::Applied)
      };
    insert_observation(&mut tx, &link, audit.security_scope(), &request.observation).await?;
    let outcome = IssueSourceOutcome::Linked { link, disposition };
    commit(tx, &identity, &audit, &outcome).await?;
    Ok(outcome)
  }
  async fn issue_source_link(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
  ) -> Result<Option<IssueSourceLink>, StoreError> {
    self.issue(project, issue, None).await?;
    let value: Option<Json<IssueSourceLink>> = sqlx::query_scalar(
      "SELECT document FROM issue_source_links WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3",
    )
    .bind(project.as_uuid())
    .bind(issue.as_uuid())
    .bind(scope.as_str())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    Ok(value.map(|Json(link)| link))
  }
  async fn attach_issue_source(
    &self,
    request: ManagementMutation<AttachIssueSource>,
  ) -> Result<IssueSourceOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::AttachIssueSource,
      request.idempotency_key.to_string(),
      request.attached_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return replay(value),
      MutationStart::Fresh(tx) => tx,
    };
    request.observation.validate(
      &intent.selection,
      request.attached_at,
      StoreOperation::AttachIssueSource,
    )?;
    crate::issue_workflow::lock_project(&mut tx, intent.selection.project_id).await?;
    let issue = crate::issue::lock(&mut tx, intent.selection.project_id, intent.issue_id).await?;
    if issue.version != intent.expected_version
      || crate::issue::field_policy(&mut tx, issue.project_id, issue.id).await? != intent.expected_field_policy
    {
      return Err(conflict());
    }
    if request.attached_at < issue.updated_at {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::AttachIssueSource,
        source: StoreInputError::InvalidIssue,
      });
    }
    let native = intent.selection.native_correlation(audit.security_scope());
    if let Some(link) = native_link(&mut tx, issue.project_id, audit.security_scope(), native).await? {
      return Ok(IssueSourceOutcome::Conflict { canonical: Some(link) });
    }
    let occupied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM issue_source_links WHERE issue_id=$1)")
      .bind(issue.id.as_uuid())
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
    if occupied {
      return Ok(IssueSourceOutcome::Conflict { canonical: None });
    }
    let link = IssueSourceLink {
      issue_id: issue.id,
      selection: intent.selection.clone(),
      native_correlation: native,
      linked_at: request.attached_at,
    };
    insert_link(&mut tx, &link, audit.security_scope()).await?;
    insert_observation(&mut tx, &link, audit.security_scope(), &request.observation).await?;
    let outcome = IssueSourceOutcome::Linked {
      link,
      disposition: MutationDisposition::Applied,
    };
    commit(tx, &identity, &audit, &outcome).await?;
    Ok(outcome)
  }
}
async fn insert_observation(
  connection: &mut PgConnection,
  link: &IssueSourceLink,
  scope: &ManagementSecurityScope,
  observation: &IssueSourceObservation,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO issue_source_observations(project_id,security_scope,native_correlation,id,document) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
    .bind(link.selection.project_id.as_uuid()).bind(scope.as_str()).bind(link.native_correlation.as_bytes().as_slice()).bind(observation.id(link.native_correlation)?.as_bytes().as_slice()).bind(Json(observation)).execute(&mut *connection).await.map_err(unavailable)?;
  sqlx::query("INSERT INTO issue_source_refresh(project_id,issue_id,document) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
    .bind(link.selection.project_id.as_uuid())
    .bind(link.issue_id.as_uuid())
    .bind(Json(IssueSourceRefreshState::new(observation.clone())))
    .execute(&mut *connection)
    .await
    .map_err(unavailable)?;
  Ok(())
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
// Only retained verified Source provenance can identify a native task. Legacy/manual Work
// keeps its existing contract; input names and current selector settings are never consulted.
pub(super) async fn guard_work_association(
  connection: &mut PgConnection,
  request: &AssociateIssueWork,
  scope: &ManagementSecurityScope,
) -> Result<(), StoreError> {
  let rows:Vec<(Vec<u8>,Vec<u8>)> = sqlx::query_as("SELECT p.issue,w.security_scope_digest FROM factory_source_preparations p JOIN factory_work_envelopes w ON w.project_id=p.project_id AND w.id=p.work_id WHERE p.project_id=$1 AND p.work_id=$2 AND p.run_id=$3 AND p.settled LIMIT 2 FOR KEY SHARE OF p,w")
    .bind(request.project_id.as_uuid()).bind(request.admission.work.id().as_uuid()).bind(request.admission.run.id().as_uuid()).fetch_all(&mut *connection).await.map_err(unavailable)?;
  if rows.len() > 1 {
    return Err(conflict());
  }
  if let Some((native, partition)) = rows.into_iter().next() {
    let partition = FactoryDigest::from_bytes(partition.try_into().map_err(|_| StoreError::Unavailable)?);
    if partition != source_security_partition(scope) {
      return Err(conflict());
    }
    let native = source_native_correlation(
      FactoryDigest::from_bytes(native.try_into().map_err(|_| StoreError::Unavailable)?),
      partition,
    );
    if native_link(connection, request.project_id, scope, native)
      .await?
      .is_none_or(|link| link.issue_id != request.issue_id)
    {
      return Err(conflict());
    }
  }
  Ok(())
}
fn visible(visibility: &IssueListVisibility, id: IssueId) -> Result<(), StoreError> {
  if visibility.allows(&id) {
    Ok(())
  } else {
    Err(StoreError::NotFound {
      entity: EntityKind::Issue,
    })
  }
}
fn replay(value: serde_json::Value) -> Result<IssueSourceOutcome, StoreError> {
  let mut outcome: IssueSourceOutcome = decode_outcome(value)?;
  if let IssueSourceOutcome::Linked { disposition, .. } = &mut outcome {
    *disposition = MutationDisposition::Replayed;
  }
  Ok(outcome)
}
pub(super) async fn native_link(
  connection: &mut PgConnection,
  project: ProjectId,
  scope: &ManagementSecurityScope,
  native: FactoryDigest,
) -> Result<Option<IssueSourceLink>, StoreError> {
  let value: Option<Json<IssueSourceLink>> = sqlx::query_scalar(
    "SELECT document FROM issue_source_links WHERE project_id=$1 AND security_scope=$2 AND native_correlation=$3",
  )
  .bind(project.as_uuid())
  .bind(scope.as_str())
  .bind(native.as_bytes().as_slice())
  .fetch_optional(connection)
  .await
  .map_err(unavailable)?;
  Ok(value.map(|Json(link)| link))
}
async fn insert_link(
  connection: &mut PgConnection,
  link: &IssueSourceLink,
  scope: &ManagementSecurityScope,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO issue_source_links(project_id,issue_id,security_scope,native_correlation,document) VALUES($1,$2,$3,$4,$5)")
    .bind(link.selection.project_id.as_uuid()).bind(link.issue_id.as_uuid()).bind(scope.as_str()).bind(link.native_correlation.as_bytes().as_slice()).bind(Json(link)).execute(&mut *connection).await.map_err(unavailable)?;
  Ok(())
}
async fn commit(
  tx: sqlx::Transaction<'_, sqlx::Postgres>,
  identity: &MutationIdentity,
  audit: &MutationAuditContext,
  outcome: &IssueSourceOutcome,
) -> Result<(), StoreError> {
  let IssueSourceOutcome::Linked { link, .. } = outcome else {
    return Err(StoreError::Unavailable);
  };
  let metadata = json!({"project_id":link.selection.project_id,"issue_id":link.issue_id,"native_correlation":link.native_correlation});
  crate::mutation::commit(
    tx,
    identity,
    MutationFacts::management(audit, link.issue_id.to_string(), metadata.clone(), metadata),
    encode_outcome(outcome)?,
  )
  .await
}

#[async_trait::async_trait]
impl IssueSourceRefreshStore for PostgresStore {
  async fn issue_source_refresh_replay(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    explicit_read: FactoryDigest,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueSourceRefreshOutcome>, StoreError> {
    if self.issue_source_link(project, issue, scope).await?.is_none() {
      return Err(conflict());
    }
    let value: Option<Json<serde_json::Value>> = sqlx::query_scalar(
      "SELECT outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3",
    )
    .bind(MutationKind::ObserveIssueSource.scope())
    .bind(scope.as_str())
    .bind(key.as_str())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    value
      .map(|Json(value)| {
        let replay: ExplicitRefreshReplay = decode_outcome(value)?;
        if replay.explicit_read != explicit_read || replay.outcome.issue_id != issue {
          return Err(conflict());
        }
        Ok(replay.outcome)
      })
      .transpose()
  }
  async fn issue_source_refresh_state(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
  ) -> Result<Option<IssueSourceRefreshState>, StoreError> {
    if self.issue_source_link(project, issue, scope).await?.is_none() {
      return Ok(None);
    }
    let value: Option<Json<IssueSourceRefreshState>> =
      sqlx::query_scalar("SELECT document FROM issue_source_refresh WHERE project_id=$1 AND issue_id=$2")
        .bind(project.as_uuid())
        .bind(issue.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
    Ok(value.map(|Json(state)| state))
  }
  async fn set_issue_source_pull(
    &self,
    request: ManagementMutation<SetIssueSourcePull>,
  ) -> Result<IssueSourcePullOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = (
      request.project_id,
      request.issue_id,
      request.expected_generation,
      request.enabled,
    );
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::SetIssueSourcePull,
      request.idempotency_key.to_string(),
      request.changed_at,
      EntityKind::Issue,
      &intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return decode_outcome(value),
      MutationStart::Fresh(tx) => tx,
    };
    crate::issue::lock(&mut tx, request.project_id, request.issue_id).await?;
    let visible: bool = sqlx::query_scalar(
      "SELECT EXISTS(SELECT 1 FROM issue_source_links WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3)",
    )
    .bind(request.project_id.as_uuid())
    .bind(request.issue_id.as_uuid())
    .bind(audit.security_scope().as_str())
    .fetch_one(&mut *tx)
    .await
    .map_err(unavailable)?;
    if !visible {
      return Err(conflict());
    }
    let Json(mut state): Json<IssueSourceRefreshState> =
      sqlx::query_scalar("SELECT document FROM issue_source_refresh WHERE project_id=$1 AND issue_id=$2 FOR UPDATE")
        .bind(request.project_id.as_uuid())
        .bind(request.issue_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    if state.generation != request.expected_generation {
      return Err(conflict());
    }
    state.generation = state.generation.checked_add(1).ok_or_else(conflict)?;
    state.enabled = request.enabled;
    let outcome = IssueSourcePullOutcome {
      enabled: state.enabled,
      generation: state.generation,
    };
    sqlx::query("UPDATE issue_source_refresh SET document=$3 WHERE project_id=$1 AND issue_id=$2")
      .bind(request.project_id.as_uuid())
      .bind(request.issue_id.as_uuid())
      .bind(Json(state))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let metadata = json!({"project_id":request.project_id,"issue_id":request.issue_id,"enabled":outcome.enabled,"generation":outcome.generation});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, request.issue_id.to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }

  async fn refresh_issue_source(
    &self,
    request: ManagementMutation<RefreshIssueSource>,
  ) -> Result<IssueSourceRefreshOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = MutationIdentity::new_management(
      &audit,
      if request.explicit_read.is_some() {
        MutationKind::ObserveIssueSource
      } else {
        MutationKind::RefreshIssueSource
      },
      request.idempotency_key.to_string(),
      request.refreshed_at,
      EntityKind::Issue,
      &request.replay_intent(),
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        return if request.explicit_read.is_some() {
          decode_outcome::<ExplicitRefreshReplay>(value).map(|r| r.outcome)
        } else {
          decode_outcome(value)
        };
      }
      MutationStart::Fresh(tx) => tx,
    };
    let current = crate::issue::lock(&mut tx, request.selection.project_id, request.issue_id).await?;
    if let Some(claim) = &request.read_claim {
      crate::issue_source_read::require_claim(
        &mut tx,
        audit.security_scope(),
        claim,
        &request.selection,
        Some(request.issue_id),
        request.refreshed_at,
      )
      .await?;
    }
    let native = request.selection.native_correlation(audit.security_scope());
    let link = native_link(&mut tx, request.selection.project_id, audit.security_scope(), native)
      .await?
      .filter(|link| link.issue_id == request.issue_id)
      .ok_or_else(conflict)?;
    request.observation.validate(
      &request.selection,
      request.refreshed_at,
      StoreOperation::RefreshIssueSource,
    )?;
    request.content.validate()?;
    let Json(mut state): Json<IssueSourceRefreshState> =
      sqlx::query_scalar("SELECT document FROM issue_source_refresh WHERE project_id=$1 AND issue_id=$2 FOR UPDATE")
        .bind(request.selection.project_id.as_uuid())
        .bind(request.issue_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    let mapping = crate::issue_source_mapping::read(&mut tx, current.project_id, current.id, None).await?;
    let initial = crate::issue_workflow::project_template(&mut tx, current.project_id, false).await?;
    let workflow =
      crate::issue_workflow::read(&mut tx, &initial, current.workflow_id, Some(current.workflow_version)).await?;
    let plan = state.plan(
      &current,
      &request,
      &crate::issue::field_policy(&mut tx, current.project_id, current.id).await?,
      mapping.as_ref().map(|mapping| IssueSourceMappingContext {
        mapping,
        workflow: &workflow,
        native_correlation: link.native_correlation,
      }),
      audit.actor(),
    )?;
    insert_observation(&mut tx, &link, audit.security_scope(), &request.observation).await?;
    sqlx::query("UPDATE issue_source_refresh SET document=$3 WHERE project_id=$1 AND issue_id=$2")
      .bind(current.project_id.as_uuid())
      .bind(current.id.as_uuid())
      .bind(Json(state))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let outcome = plan.outcome;
    let stored = if let Some(explicit_read) = request.explicit_read {
      serde_json::to_value(ExplicitRefreshReplay {
        explicit_read,
        outcome: outcome.clone(),
      })
    } else {
      serde_json::to_value(&outcome)
    }
    .map_err(|_| StoreError::Unavailable)?;
    if let Some(next) = plan.revision {
      crate::issue::publish(tx, &identity, &audit, &next, plan.transition.as_ref(), &stored, None).await?;
    } else {
      let metadata = json!({"project_id":current.project_id,"issue_id":current.id,"disposition":outcome.disposition});
      crate::mutation::commit(
        tx,
        &identity,
        MutationFacts::management(&audit, current.id.to_string(), metadata.clone(), metadata),
        encode_outcome(&stored)?,
      )
      .await?;
    }
    Ok(outcome)
  }
}
