use async_trait::async_trait;

use crate::Command;

/// One authoritative transaction boundary for a typed command.
///
/// Implementations may validate and load prerequisite state before the
/// transaction starts, but a successful result MUST mean that the domain
/// state, idempotency outcome, audit fact, and required outbox entries have
/// committed together. An error MUST leave none of those writes visible.
#[async_trait]
pub trait CommandTransaction<C>: Send + Sync
where
  C: Command,
{
  /// Typed failure returned without publishing command success.
  type Error: Send;

  /// Executes the command and returns only after its authoritative commit.
  async fn commit_command(&self, command: C) -> Result<C::Outcome, Self::Error>;
}
