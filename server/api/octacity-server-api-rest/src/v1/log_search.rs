use serde::{Deserialize, Serialize};

use super::Cursor;

/// Stable query modes supported by the v1 Build-log search contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildLogSearchMode {
  /// Match all whitespace-separated terms without language stemming.
  FullText,
  /// Match one exact UTF-8 fragment.
  Literal,
}

/// Logical process output stream exposed by Build-log search.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildLogStream {
  /// Standard output.
  Stdout,
  /// Standard error.
  Stderr,
}

/// One redacted logical log-chunk match.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildLogSearchHit {
  /// Immutable logical chunk identity.
  pub chunk_id: String,
  /// Owning Build identity.
  pub build_id: String,
  /// Owning Attempt identity.
  pub attempt_id: String,
  /// Owning Job identity.
  pub job_id: String,
  /// Logical output stream.
  pub stream: BuildLogStream,
  /// First event sequence represented by the chunk.
  pub first_sequence: u64,
  /// Last event sequence represented by the chunk.
  pub last_sequence: u64,
  /// Source-observed Unix time in milliseconds.
  pub occurred_at_unix_ms: i64,
  /// Bounded redacted context surrounding the match.
  pub snippet: String,
}

/// Search-projection progress relative to authoritative committed logs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildLogSearchFreshness {
  /// Greatest contiguous Project position applied by the search index.
  pub indexed_through: Option<u64>,
  /// Latest authoritative committed Project position.
  pub committed_through: Option<u64>,
  /// Whether the projection includes every committed position.
  pub caught_up: bool,
}

/// Deterministic bounded page of redacted Build-log matches.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildLogSearchPage {
  /// Matching logical chunks in deterministic newest-first order.
  pub items: Vec<BuildLogSearchHit>,
  /// Opaque exclusive cursor for the following page.
  pub next_cursor: Option<Cursor>,
  /// Projection progress observed by this query.
  pub freshness: BuildLogSearchFreshness,
}
