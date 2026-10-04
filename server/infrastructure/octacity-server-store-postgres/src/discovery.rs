use octacity_server_domain::{EntityKind, ProjectId, Timestamp};
use octacity_server_store::StoreError;
use sqlx::PgPool;

use crate::database::unavailable;

pub(crate) async fn require_project(pool: &PgPool, project_id: ProjectId) -> Result<(), StoreError> {
  let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1)")
    .bind(project_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(unavailable)?;
  exists.then_some(()).ok_or(StoreError::NotFound {
    entity: EntityKind::Project,
  })
}

pub(crate) fn requested_row_limit(page_limit: u16) -> i64 {
  i64::from(page_limit) + 1
}

pub(crate) fn decode_page<R, T, I>(
  rows: Vec<R>,
  limit: u16,
  decode: impl FnMut(R) -> Result<T, StoreError>,
  position: impl Fn(&T) -> I,
) -> Result<(Vec<T>, Option<I>), StoreError> {
  let mut items = rows.into_iter().map(decode).collect::<Result<Vec<_>, _>>()?;
  let limit = usize::from(limit);
  let has_more = items.len() > limit;
  items.truncate(limit);
  let next_cursor = has_more.then(|| position(items.last().expect("a non-zero full page has a last item")));
  Ok((items, next_cursor))
}

pub(crate) fn positive(value: i64) -> Result<u64, StoreError> {
  u64::try_from(value)
    .ok()
    .filter(|value| *value > 0)
    .ok_or(StoreError::Unavailable)
}

pub(crate) fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}
