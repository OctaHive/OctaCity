//! Independently versioned, read-only ticket discovery.
use crate::*;

/// Supported Source version; Reporter and Delivery negotiate independently.
pub const PROTOCOL_VERSION: u16 = 1;
/// Maximum normalized candidates per page.
pub const MAX_ITEMS: u16 = 100;
/// Maximum opaque continuation length.
pub const MAX_CURSOR_BYTES: usize = 4096;
/// One read-only operation or cancellation of its original scan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
  /// Selected Source version.
  pub protocol_version: u16,
  /// Process response correlation.
  pub request_id: String,
  /// Source operation; writes and admission are absent.
  pub command: Command,
}
/// Closed Source operation vocabulary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// Read-only installed connection/resource readiness, without execution inputs.
  Check(check::CheckIntent),
  /// Reads one bounded page using the exact installed selector.
  Scan(Box<ScanIntent>),
  /// Reads summaries and selection reasons without snapshots or worker state.
  Preview(Box<ScanIntent>),
  /// Checks current private installed read authority without listing or retaining tickets.
  Authorize(AccessIntent),
  /// Stops only this scan and performs no provider effects.
  Cancel {
    /// Original trusted scan identity.
    target_scan_id: String,
  },
}
/// One correlated Source observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
  /// Selected Source version.
  pub protocol_version: u16,
  /// Original request correlation.
  pub request_id: String,
  /// Bounded discovery facts.
  pub outcome: Outcome,
}
/// Closed Source outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// Safe read-only configuration/authentication/resource/capability observations.
  Checked(check::CheckReport),
  /// Safe classified failure, never an empty successful page.
  Failure(Failure),
  /// One complete private page; checkpointing and admission remain server-owned.
  Page(Page),
  /// Bounded read-only summaries, including candidates excluded by every selector.
  Previewed(crate::preview::Page),
  /// Current installed read authority for the original exact scope; no admission authority.
  Authorized(AuthorizedAccess),
  /// Original scan stopped without starting another operation.
  Acknowledged {
    /// Original trusted scan identity.
    scan_id: String,
  },
}
impl Request {
  /// Rejects an incompatible role version or invalid operation before I/O.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    field(&self.request_id)?;
    match &self.command {
      Command::Check(intent) => intent.validate(CredentialClass::WorkSourceRead),
      Command::Scan(intent) | Command::Preview(intent) => intent.validate(),
      Command::Authorize(intent) => intent.validate(),
      Command::Cancel { target_scan_id } => digest(target_scan_id),
    }
  }
}

/// Read-grant verification independent of selectors, discovery cursors and ticket content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccessIntent {
  /// Original trusted Source operation identity.
  pub scan_id: String,
  /// Exact operator profile, excluding credential bytes.
  pub configuration_digest: String,
  /// Exact native container and independently scoped opaque read handle.
  pub resource: ResourceAccess,
  /// Permitted provider destinations.
  pub network_hosts: Vec<String>,
}
impl AccessIntent {
  /// Validates read class and installed scope before private grant resolution.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    digest(&self.scan_id)?;
    digest(&self.configuration_digest)?;
    self.resource.validate(CredentialClass::WorkSourceRead)?;
    common::require_network_hosts(&self.network_hosts).map_err(|_| ProtocolError::Invalid)
  }
}
/// Correlated attestation of current installed read authority, never ticket or Factory data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedAccess {
  /// Original trusted Source operation identity.
  pub scan_id: String,
  /// Exact native ticket container.
  pub resource_id: String,
  /// Exact operator profile.
  pub configuration_digest: String,
}
/// Frozen inclusive update window; repeated/overlapping observations are allowed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScanWindow {
  /// Earliest update in Unix milliseconds.
  pub from_millis: u64,
  /// Last update in Unix milliseconds, fixed across a page pass.
  pub until_millis: u64,
}
/// A single page read, without admission, checkpoint or provider write authority.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScanIntent {
  /// Original trusted scan operation identity.
  pub scan_id: String,
  /// Frozen operator profile, excluding rotated credential bytes.
  pub configuration_digest: String,
  /// Exact installed/native ticket scope.
  pub resource: ResourceAccess,
  /// Exact adapter-owned selector, not a provider search expression.
  pub selection: ConfigurationReference,
  /// Additional exact member selectors evaluated against the same snapshot (at most 63).
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub additional_selections: Vec<ConfigurationReference>,
  /// Frozen exact host ceiling; the provider verifies every actual destination.
  pub network_hosts: Vec<String>,
  /// Private opaque provider continuation, not trusted network authority.
  pub cursor: Option<String>,
  /// Fixed incremental window, or explicit full reconciliation.
  pub window: Option<ScanWindow>,
  /// Maximum candidates returned in this page, at most 100.
  pub max_items: u16,
  /// Encoded private snapshot ceiling per candidate, at most 1 MiB.
  pub max_snapshot_bytes: u32,
  /// Complete encoded response ceiling, at most 4 MiB.
  pub max_page_bytes: u32,
}
impl std::fmt::Debug for ScanIntent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ScanIntent")
      .field("scan_id", &self.scan_id)
      .field("resource", &self.resource)
      .finish_non_exhaustive()
  }
}
impl ScanIntent {
  /// Checks role, scope and caller-narrowed bounds without provider I/O.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    digest(&self.scan_id)?;
    digest(&self.configuration_digest)?;
    self.resource.validate(CredentialClass::WorkSourceRead)?;
    self.selection.validate()?;
    if self.additional_selections.len() > 63 {
      return Err(ProtocolError::Invalid);
    }
    let mut selections = std::collections::BTreeSet::new();
    selections.insert((&self.selection.identity, &self.selection.sha256));
    for selection in &self.additional_selections {
      selection.validate()?;
      if !selections.insert((&selection.identity, &selection.sha256)) {
        return Err(ProtocolError::Invalid);
      }
    }
    common::require_network_hosts(&self.network_hosts).map_err(|_| ProtocolError::Invalid)?;
    if let Some(cursor) = &self.cursor {
      common::require_bounded_text(cursor, MAX_CURSOR_BYTES).map_err(|_| ProtocolError::Invalid)?;
    }
    if !(1..=MAX_ITEMS).contains(&self.max_items)
      || !(1..=MAX_SNAPSHOT_BYTES as u32).contains(&self.max_snapshot_bytes)
      || !(1..=MAX_RESPONSE_BYTES as u32).contains(&self.max_page_bytes)
      || self
        .window
        .is_some_and(|window| window.from_millis > window.until_millis || window.until_millis > i64::MAX as u64)
    {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}
impl Response {
  /// Rejects a response for another version, request or scan.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    common::require_correlation(
      self.protocol_version,
      &self.request_id,
      request.protocol_version,
      &request.request_id,
    )
    .map_err(|_| ProtocolError::Invalid)?;
    match (&self.outcome, &request.command) {
      (Outcome::Checked(report), Command::Check(intent)) => report.validate_for(intent),
      (Outcome::Failure(failure), _) => failure.validate(),
      (Outcome::Authorized(access), Command::Authorize(intent))
        if access.scan_id == intent.scan_id
          && access.resource_id == intent.resource.resource_id
          && access.configuration_digest == intent.configuration_digest =>
      {
        Ok(())
      }
      (Outcome::Previewed(page), Command::Preview(intent)) => {
        page.validate_for(intent)?;
        let encoded = serde_json::to_vec(&crate::Response::Source(self.clone())).map_err(|_| ProtocolError::Invalid)?;
        common::require_message_size(&encoded, intent.max_page_bytes as usize).map_err(|_| ProtocolError::LimitExceeded)
      }
      (Outcome::Page(page), Command::Scan(intent)) => {
        page.validate_for(intent)?;
        // Caller policy can narrow the complete family envelope as well as individual snapshots.
        let encoded = serde_json::to_vec(&crate::Response::Source(self.clone())).map_err(|_| ProtocolError::Invalid)?;
        common::require_message_size(&encoded, intent.max_page_bytes as usize).map_err(|_| ProtocolError::LimitExceeded)
      }
      (Outcome::Acknowledged { scan_id }, Command::Cancel { target_scan_id }) if scan_id == target_scan_id => Ok(()),
      _ => Err(ProtocolError::Invalid),
    }
  }
}

/// Correlated page facts, including private task data and opaque continuation.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
  /// Original trusted scan identity.
  pub scan_id: String,
  /// Original native container identity.
  pub resource_id: String,
  /// Original installed profile.
  pub configuration_digest: String,
  /// Exact original installed selector.
  pub selection: ConfigurationReference,
  /// Complete normalized candidates; absence means an empty page, not a provider outage.
  pub candidates: Vec<Candidate>,
  /// Matching installed selectors per candidate. Required for union/group scans; no target authority.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub matching_selections: Vec<Vec<ConfigurationReference>>,
  /// Private opaque continuation; None means this page pass ended.
  pub next_cursor: Option<String>,
}
impl std::fmt::Debug for Page {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Page")
      .field("scan_id", &self.scan_id)
      .field("count", &self.candidates.len())
      .finish_non_exhaustive()
  }
}
impl Page {
  /// Rejects substituted scope, duplicate candidates and caller-ceiling violations.
  pub fn validate_for(&self, intent: &ScanIntent) -> Result<(), ProtocolError> {
    intent.validate()?;
    if self.scan_id != intent.scan_id
      || self.resource_id != intent.resource.resource_id
      || self.configuration_digest != intent.configuration_digest
      || self.selection != intent.selection
      || self.candidates.len() > usize::from(intent.max_items)
    {
      return Err(ProtocolError::Invalid);
    }
    if let Some(cursor) = &self.next_cursor {
      common::require_bounded_text(cursor, MAX_CURSOR_BYTES).map_err(|_| ProtocolError::Invalid)?;
    }
    if (!intent.additional_selections.is_empty() || !self.matching_selections.is_empty())
      && self.matching_selections.len() != self.candidates.len()
    {
      return Err(ProtocolError::Invalid);
    }
    for matches in &self.matching_selections {
      if matches.is_empty() || matches.len() > 1 + intent.additional_selections.len() {
        return Err(ProtocolError::Invalid);
      }
      let mut seen = std::collections::BTreeSet::new();
      for selection in matches {
        if (selection != &intent.selection && !intent.additional_selections.contains(selection))
          || !seen.insert((&selection.identity, &selection.sha256))
        {
          return Err(ProtocolError::Invalid);
        }
      }
    }
    let mut identities = std::collections::BTreeSet::new();
    for candidate in &self.candidates {
      candidate.validate(&self.resource_id, intent.max_snapshot_bytes as usize)?;
      if !identities.insert(&candidate.item_id) {
        return Err(ProtocolError::Invalid);
      }
    }
    Ok(())
  }
}
