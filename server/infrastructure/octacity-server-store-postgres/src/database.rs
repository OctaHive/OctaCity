use octacity_server_domain::EntityKind;
use octacity_server_store::{StoreError, StoreInputError, StoreOperation};

pub(crate) fn number(value: u64, operation: StoreOperation) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| StoreError::InvalidInput {
    operation,
    source: StoreInputError::NumericOutOfRange,
  })
}

pub(crate) fn classify(error: sqlx::Error, entity: EntityKind) -> StoreError {
  let sql_state = error.as_database_error().and_then(|error| error.code());
  match sql_state.as_deref() {
    Some("23505") => StoreError::Duplicate { entity },
    Some("23503") => StoreError::NotFound { entity },
    Some("23514" | "23P01") => StoreError::Conflict { entity },
    _ => StoreError::Unavailable,
  }
}

pub(crate) fn unavailable(_: sqlx::Error) -> StoreError {
  StoreError::Unavailable
}
