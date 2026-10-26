//! Append-only import metadata uses the existing management mutation protocol and Source authority checks.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IntegrationVersion, IssueSourceImportId, ProjectId, Timestamp};
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::{PgConnection, types::Json};
const KIND: MutationKind = MutationKind::PublishIssueSourceImport;
fn replay(value: Value) -> Result<IssueSourceImportOutcome, StoreError> {
  let mut outcome: IssueSourceImportOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
pub(super) async fn read(
  connection: &mut PgConnection,
  project: ProjectId,
  scope: &ManagementSecurityScope,
  id: IssueSourceImportId,
  version: Option<IntegrationVersion>,
) -> Result<PublishedIssueSourceImport, StoreError> {
  let document: Option<(Json<PublishedIssueSourceImport>,)> = sqlx::query_as(
    "SELECT document FROM issue_source_imports WHERE project_id=$1 AND security_scope=$2 AND id=$3 AND ($4::bigint IS NULL OR version=$4) ORDER BY version DESC LIMIT 1"
  ).bind(project.as_uuid()).bind(scope.as_str()).bind(id.as_uuid()).bind(version.map(|v|v.get() as i64))
    .fetch_optional(connection).await.map_err(unavailable)?;
  document.map(|(Json(v),)| v).ok_or(StoreError::NotFound {
    entity: EntityKind::Integration,
  })
}
#[async_trait::async_trait]
impl IssueSourceImportStore for PostgresStore {
  async fn replay_issue_source_import(
    &self,
    request: ManagementMutation<ReplayIssueSourceImport>,
  ) -> Result<Option<IssueSourceImportOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = MutationIdentity::new_management(
      &audit,
      KIND,
      request.idempotency_key.to_string(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Project,
      &request.intent,
    )?;
    let row: Option<(Vec<u8>,Json<Value>)> = sqlx::query_as(
      "SELECT request_digest,outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3"
    ).bind(KIND.scope()).bind(identity.persisted_security_scope()).bind(&identity.key)
      .fetch_optional(self.pool()).await.map_err(unavailable)?;
    let Some((digest, Json(value))) = row else {
      return Ok(None);
    };
    if digest != identity.request_digest.as_slice() {
      return Err(issue_source_import_conflict());
    }
    replay(value).map(Some)
  }
  async fn publish_issue_source_import(
    &self,
    request: ManagementMutation<PublishIssueSourceImport>,
  ) -> Result<IssueSourceImportOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let configuration = request.resolve()?;
    let identity = MutationIdentity::new_management(
      &audit,
      KIND,
      request.idempotency_key.to_string(),
      request.published_at,
      EntityKind::Project,
      &request.intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return replay(value),
      MutationStart::Fresh(tx) => tx,
    };
    let selected = &configuration.definition.connection;
    // Connection -> Project ordering matches grant/metadata publication, including disablement.
    let current: Option<i64> =
      sqlx::query_scalar("SELECT current_version FROM integration_connections WHERE id=$1 FOR SHARE")
        .bind(selected.id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?;
    if current.is_none() || (configuration.definition.enabled && current != Some(selected.version.get() as i64)) {
      return Err(issue_source_import_conflict());
    }
    crate::issue_workflow::lock_project(&mut tx, configuration.project_id).await?;
    let previous: Option<i64> = sqlx::query_scalar(
      "SELECT max(version) FROM issue_source_imports WHERE project_id=$1 AND security_scope=$2 AND id=$3",
    )
    .bind(configuration.project_id.as_uuid())
    .bind(audit.security_scope().as_str())
    .bind(configuration.id.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .map_err(unavailable)?;
    if previous.map(|v| v as u64) != request.intent.expected_current_version.map(|v| v.get()) {
      return Err(issue_source_import_conflict());
    }
    // Disabled revisions may keep an old parent, but it must be retained metadata.
    let metadata = crate::integration_connection::read(&mut *tx, selected.id, Some(selected.version)).await?;
    if configuration.definition.enabled {
      let access = crate::integration_project_access::read(&mut *tx, selected.id, configuration.project_id)
        .await?
        .ok_or_else(issue_source_import_conflict)?;
      let administration = crate::integration_administration::read_state(&mut tx, selected.id).await?;
      ManagedWorkSourceContext {
        project_id: configuration.project_id,
        connection: selected,
        metadata: &metadata,
        access: &access,
        administration: &administration,
      }
      .validate(&configuration.definition.source, IntegrationPurpose::TaskRead)?;
    }
    sqlx::query("INSERT INTO issue_source_imports(project_id,security_scope,id,version,connection_id,connection_version,document) VALUES($1,$2,$3,$4,$5,$6,$7)")
      .bind(configuration.project_id.as_uuid()).bind(audit.security_scope().as_str()).bind(configuration.id.as_uuid())
      .bind(configuration.version.get() as i64).bind(selected.id.as_uuid()).bind(selected.version.get() as i64)
      .bind(Json(&configuration)).execute(&mut *tx).await.map_err(unavailable)?;
    let facts = MutationFacts::management(
      &audit,
      configuration.project_id.to_string(),
      json!({"import_id":configuration.id,"version":configuration.version,"enabled":configuration.definition.enabled}),
      json!({"project_id":configuration.project_id,"import_id":configuration.id,"version":configuration.version}),
    );
    let outcome = IssueSourceImportOutcome {
      disposition: MutationDisposition::Applied,
      configuration,
    };
    crate::mutation::commit(tx, &identity, facts, encode_outcome(&outcome)?).await?;
    Ok(outcome)
  }
  async fn issue_source_import(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    id: IssueSourceImportId,
    version: Option<IntegrationVersion>,
  ) -> Result<PublishedIssueSourceImport, StoreError> {
    let mut connection = self.pool().acquire().await.map_err(unavailable)?;
    read(&mut connection, project, scope, id, version).await
  }
  async fn list_issue_source_imports(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    visibility: IntegrationConnectionListVisibility,
    after: Option<IssueSourceImportId>,
    limit: u16,
  ) -> Result<IssueSourceImportPage, StoreError> {
    if !(1..=MAX_INTEGRATION_DISCOVERY_PAGE_SIZE).contains(&limit) {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::PublishIssueSourceImport,
        source: StoreInputError::InvalidIssueSourceImport,
      });
    }
    crate::discovery::require_project(self.pool(), project).await?;
    let visibility = crate::read_visibility::sql_read_visibility(visibility.view(), |id| id.as_uuid());
    let rows:Vec<(Json<PublishedIssueSourceImport>,)>=sqlx::query_as(
      "SELECT document FROM (SELECT DISTINCT ON(id) id,connection_id,document FROM issue_source_imports WHERE project_id=$1 AND security_scope=$2 ORDER BY id,version DESC) heads WHERE ($5 OR connection_id=ANY($6::uuid[])) AND ($3::uuid IS NULL OR id>$3) ORDER BY id LIMIT $4"
    ).bind(project.as_uuid()).bind(scope.as_str()).bind(after.map(|v|v.as_uuid())).bind(i64::from(limit)+1).bind(visibility.all).bind(visibility.identities)
      .fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items: Vec<_> = rows.into_iter().map(|(Json(v),)| v).collect();
    let more = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    let next_cursor = more.then(|| items.last().map(|v| v.id)).flatten();
    Ok(IssueSourceImportPage { items, next_cursor })
  }
}
