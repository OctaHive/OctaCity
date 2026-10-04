use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::{ContractValueError, validate_url_safe_cursor};

/// Searchable management resource kind exposed by the v1 API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceSearchKind {
  /// Hierarchical Project.
  Project,
  /// Immutable Build request and current state.
  Build,
  /// Registered Agent.
  Agent,
  /// Static Agent Pool.
  AgentPool,
}

impl FromStr for ResourceSearchKind {
  type Err = ContractValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value {
      "project" => Ok(Self::Project),
      "build" => Ok(Self::Build),
      "agent" => Ok(Self::Agent),
      "agent_pool" => Ok(Self::AgentPool),
      _ => Err(ContractValueError::InvalidResourceSearchKind),
    }
  }
}

/// Typed bounded result returned by global resource search.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSearchResult {
  /// Searchable resource kind.
  pub kind: ResourceSearchKind,
  /// Stable resource identity.
  pub id: String,
  /// Safe operator-facing resource label.
  pub label: String,
  /// Optional bounded non-secret context used to distinguish equal labels.
  pub context: Option<String>,
}

/// Bounded opaque cursor owned by the global resource-search contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceSearchCursor(String);

impl ResourceSearchCursor {
  /// Validates one non-empty URL-safe cursor within the search-specific bound.
  pub fn new(value: impl Into<String>) -> Result<Self, ContractValueError> {
    validate_url_safe_cursor(
      value.into(),
      octacity_server_application::MAX_RESOURCE_SEARCH_CURSOR_BYTES,
    )
    .map(Self)
  }

  /// Borrows the opaque encoded cursor.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl Serialize for ResourceSearchCursor {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for ResourceSearchCursor {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// One deterministic bounded page of global resource-search results.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSearchPage {
  /// Visible results in deterministic rank order.
  pub items: Vec<ResourceSearchResult>,
  /// Exclusive cursor for the following page, or `None` at the end.
  pub next_cursor: Option<ResourceSearchCursor>,
}
