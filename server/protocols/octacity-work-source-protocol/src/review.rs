//! Independently authorized, read-only code review observations.
use crate::*;

/// Supported review reader protocol, independent of ticket and Delivery roles.
pub const PROTOCOL_VERSION: u16 = 1;
/// One bounded read or cancellation, without publication or merge operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
  /// Selected review protocol version.
  pub protocol_version: u16,
  /// Original process correlation.
  pub request_id: String,
  /// Read-only operation.
  pub command: Command,
}
/// Closed review reader operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// Observe one repository-scoped native review identity.
  Observe(Box<Intent>),
  /// Read one bounded page of checks for the original head.
  Checks(Box<Intent>),
  /// Resolve only current installed repository read authority, without provider I/O.
  Authorize(Box<Intent>),
  /// Stop only the original read.
  Cancel {
    /// Original trusted read identity.
    target_read_id: String,
  },
}
/// Exact installed repository authority and caller-narrowed read bounds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
  /// Stable server-owned read identity.
  pub read_id: String,
  /// Exact installed profile digest.
  pub configuration_digest: String,
  /// Separately authorized code repository, never a ticket container grant.
  pub resource: ResourceAccess,
  /// Provider-native review key within the exact repository.
  pub item_id: String,
  /// Original head when reading head-scoped facts.
  pub head: Option<String>,
  /// Installed destination ceiling.
  pub network_hosts: Vec<String>,
  /// Opaque continuation, never network authority.
  pub cursor: Option<String>,
  /// Maximum records per page, at most 100.
  pub max_items: u16,
  /// Encoded response ceiling, at most 4 MiB.
  pub max_page_bytes: u32,
}
impl Intent {
  /// Validates independent authority and bounded input before private grant resolution.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    digest(&self.read_id)?;
    digest(&self.configuration_digest)?;
    self.resource.validate(CredentialClass::RepositoryRead)?;
    field(&self.item_id)?;
    if let Some(head) = &self.head {
      field(head)?;
    }
    common::require_network_hosts(&self.network_hosts).map_err(|_| ProtocolError::Invalid)?;
    if let Some(cursor) = &self.cursor {
      common::require_bounded_text(cursor, source::MAX_CURSOR_BYTES).map_err(|_| ProtocolError::Invalid)?;
    }
    if !(1..=source::MAX_ITEMS).contains(&self.max_items)
      || !(1..=MAX_RESPONSE_BYTES as u32).contains(&self.max_page_bytes)
    {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
  /// Exact response scope, excluding private handles.
  pub fn subject(&self) -> Subject {
    Subject {
      read_id: self.read_id.clone(),
      configuration_digest: self.configuration_digest.clone(),
      resource_id: self.resource.resource_id.clone(),
      item_id: self.item_id.clone(),
    }
  }
}
/// Correlation binding facts to their original installed repository and native review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Subject {
  /// Original trusted read.
  pub read_id: String,
  /// Exact installed profile.
  pub configuration_digest: String,
  /// Original native code repository.
  pub resource_id: String,
  /// Original repository-scoped review key.
  pub item_id: String,
}
/// Provider-neutral lifecycle fact, never an Issue transition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
  /// Open review.
  Open,
  /// Closed without merge.
  Closed,
  /// Provider reports a completed merge.
  Merged,
}
/// Bounded optional metadata; absent fields remain unknown.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
  /// Safe title.
  pub title: Option<String>,
  /// Display number, not repository authority.
  pub number: Option<String>,
  /// Provider navigation hint, not read authority.
  pub url: Option<String>,
  /// Safe public author identifier.
  pub author: Option<String>,
  /// Observed lifecycle, if supported.
  pub state: Option<State>,
  /// Draft status, if supported.
  pub draft: Option<bool>,
  /// Original immutable head revision.
  pub head: Option<String>,
  /// Original immutable base revision.
  pub base: Option<String>,
  /// Changed file count, if supported.
  pub changed_files: Option<u64>,
  /// Added line count, if supported.
  pub additions: Option<u64>,
  /// Deleted line count, if supported.
  pub deletions: Option<u64>,
  /// Optional bounded review summary, never an acceptance decision.
  pub review: Option<String>,
  /// Optional bounded checks summary, never an acceptance decision.
  pub checks: Option<String>,
}
impl Facts {
  fn validate(&self) -> Result<(), ProtocolError> {
    for value in [
      &self.title,
      &self.number,
      &self.url,
      &self.author,
      &self.head,
      &self.base,
      &self.review,
      &self.checks,
    ]
    .into_iter()
    .flatten()
    {
      field(value)?;
    }
    Ok(())
  }
}
/// One bounded read result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
  /// Exact original scope.
  pub subject: Subject,
  /// Supported public facts.
  pub facts: Facts,
}
/// Correlated terminal response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
  /// Selected review protocol.
  pub protocol_version: u16,
  /// Original request correlation.
  pub request_id: String,
  /// Safe normalized result.
  pub outcome: Outcome,
}
/// Closed read outcomes; no arbitrary provider DTO or diagnostic text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// One review observation.
  Observed(Observation),
  /// One original-head check page; continuation means incomplete facts.
  Checks(CheckPage),
  /// Current installed repository read grant.
  Authorized(Subject),
  /// Same-read cancellation acknowledgement.
  Acknowledged {
    /// Original trusted read identity.
    read_id: String,
  },
  /// Closed failure category, distinct from unsupported metadata.
  Failure(Failure),
}
impl Request {
  /// Checks the closed role and input scope.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    field(&self.request_id)?;
    match &self.command {
      Command::Observe(intent) | Command::Authorize(intent) => {
        intent.validate()?;
        if intent.cursor.is_some() || intent.head.is_some() {
          return Err(ProtocolError::Invalid);
        }
        Ok(())
      }
      Command::Checks(intent) => {
        intent.validate()?;
        field(intent.head.as_deref().ok_or(ProtocolError::Invalid)?)
      }
      Command::Cancel { target_read_id } => digest(target_read_id),
    }
  }
}
impl Response {
  /// Rejects responses belonging to another request, profile, repository or review.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    common::require_correlation(
      self.protocol_version,
      &self.request_id,
      request.protocol_version,
      &request.request_id,
    )
    .map_err(|_| ProtocolError::Invalid)?;
    match (&self.outcome, &request.command) {
      (Outcome::Failure(failure), _) => failure.validate(),
      (Outcome::Observed(observation), Command::Observe(intent)) if observation.subject == intent.subject() => {
        observation.facts.validate()
      }
      (Outcome::Authorized(subject), Command::Authorize(intent)) if *subject == intent.subject() => Ok(()),
      (Outcome::Acknowledged { read_id }, Command::Cancel { target_read_id }) if read_id == target_read_id => Ok(()),
      (Outcome::Checks(page), Command::Checks(intent))
        if page.subject == intent.subject() && Some(&page.head) == intent.head.as_ref() =>
      {
        if page.items.len() > intent.max_items as usize {
          return Err(ProtocolError::LimitExceeded);
        }
        let mut ids = std::collections::BTreeSet::new();
        for check in &page.items {
          field(&check.id)?;
          field(&check.name)?;
          for value in [&check.producer, &check.url].into_iter().flatten() {
            field(value)?;
          }
          if !ids.insert(&check.id) || (check.state != CheckState::Completed && check.conclusion.is_some()) {
            return Err(ProtocolError::Invalid);
          }
        }
        if let Some(cursor) = &page.next_cursor {
          common::require_bounded_text(cursor, source::MAX_CURSOR_BYTES).map_err(|_| ProtocolError::Invalid)?;
        }
        Ok(())
      }
      _ => Err(ProtocolError::Invalid),
    }
  }
}

/// Normalized check lifecycle; unknown facts never imply success.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
  /// Waiting to run.
  Pending,
  /// Running.
  Running,
  /// Provider reports completion; conclusion can remain unavailable.
  Completed,
  /// Unsupported provider state.
  Unknown,
}
/// One supported public check fact, with no logs, outputs or private annotations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
  /// Stable native check identity.
  pub id: String,
  /// Safe display name.
  pub name: String,
  /// Visible producer, if supported.
  pub producer: Option<String>,
  /// Normalized lifecycle.
  pub state: CheckState,
  /// Supported terminal conclusion; absence remains unknown.
  pub conclusion: Option<Conclusion>,
  /// Safe navigation hint, never read authority.
  pub url: Option<String>,
}
/// Closed supported conclusions; none is an OctaCity acceptance decision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Conclusion {
  /// Provider reports success.
  Success,
  /// Provider reports failure.
  Failure,
  /// Provider reports cancellation.
  Cancelled,
  /// Provider reports skipped work.
  Skipped,
  /// Provider reports a neutral outcome.
  Neutral,
  /// Provider reports timeout.
  TimedOut,
  /// Provider requires action.
  ActionRequired,
  /// Provider reports stale completion.
  Stale,
}
/// Bounded checks for exactly one original review head.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckPage {
  /// Original scope.
  pub subject: Subject,
  /// Immutable original head, never silently replaced with a newer head.
  pub head: String,
  /// One bounded page, potentially incomplete.
  pub items: Vec<Check>,
  /// Opaque continuation; absence means this pass ended, not acceptance.
  pub next_cursor: Option<String>,
}
impl Command {
  /// Read intent, absent for same-read cancellation.
  pub fn intent(&self) -> Option<&Intent> {
    match self {
      Self::Observe(intent) | Self::Checks(intent) | Self::Authorize(intent) => Some(intent),
      Self::Cancel { .. } => None,
    }
  }
}
