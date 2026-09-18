use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Whether an idempotent command changed state or replayed a prior outcome.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationDisposition {
  /// The command committed new state.
  Applied,
  /// An identical prior command supplied the returned outcome.
  Replayed,
}

impl From<octacity_server_store::MutationDisposition> for MutationDisposition {
  fn from(value: octacity_server_store::MutationDisposition) -> Self {
    match value {
      octacity_server_store::MutationDisposition::Applied => Self::Applied,
      octacity_server_store::MutationDisposition::Replayed => Self::Replayed,
    }
  }
}

/// A transport-independent request that intends to change authoritative state.
pub trait Command: Send {
  /// Typed result returned after the command has completed successfully.
  type Outcome: Send;
}

/// A transport-independent request that reads application state.
pub trait Query: Send {
  /// Typed application projection returned by the query.
  type Outcome: Send;
}

/// Compile-time dispatch contract for one command type.
#[async_trait]
pub trait CommandHandler<C>: Send + Sync
where
  C: Command,
{
  /// Typed failure exposed at the application boundary.
  type Error: Send;

  /// Handles one command without transport-specific context.
  async fn handle_command(&self, command: C) -> Result<C::Outcome, Self::Error>;
}

/// Compile-time dispatch contract for one query type.
#[async_trait]
pub trait QueryHandler<Q>: Send + Sync
where
  Q: Query,
{
  /// Typed failure exposed at the application boundary.
  type Error: Send;

  /// Handles one query and returns an application projection.
  async fn handle_query(&self, query: Q) -> Result<Q::Outcome, Self::Error>;
}
