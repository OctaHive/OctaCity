use std::time::Duration;

use serde::Deserialize;

const DEFAULT_WINDOW_MILLISECONDS: u64 = 1_000;
const DEFAULT_TRACKED_IDENTITIES: u32 = 4_096;
const DEFAULT_MANAGEMENT_REQUESTS: u32 = 200;
const DEFAULT_EXPENSIVE_MANAGEMENT_REQUESTS: u32 = 20;
const DEFAULT_AGENT_REQUESTS: u32 = 200;
const DEFAULT_AGENT_HEARTBEATS: u32 = 20;
const DEFAULT_WEBHOOK_REQUESTS: u32 = 50;

const MAX_WINDOW_MILLISECONDS: u64 = 60_000;
const MAX_TRACKED_IDENTITIES: u32 = 65_536;
const MAX_REQUESTS_PER_WINDOW: u32 = 100_000;

/// Process-local overload policy. Its counters are deliberately disposable
/// and never participate in authoritative coordination state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AdmissionConfig {
  window_milliseconds: u64,
  tracked_identities: u32,
  management_requests: u32,
  expensive_management_requests: u32,
  agent_requests: u32,
  agent_heartbeats: u32,
  webhook_requests: u32,
}

impl Default for AdmissionConfig {
  fn default() -> Self {
    Self {
      window_milliseconds: DEFAULT_WINDOW_MILLISECONDS,
      tracked_identities: DEFAULT_TRACKED_IDENTITIES,
      management_requests: DEFAULT_MANAGEMENT_REQUESTS,
      expensive_management_requests: DEFAULT_EXPENSIVE_MANAGEMENT_REQUESTS,
      agent_requests: DEFAULT_AGENT_REQUESTS,
      agent_heartbeats: DEFAULT_AGENT_HEARTBEATS,
      webhook_requests: DEFAULT_WEBHOOK_REQUESTS,
    }
  }
}

impl AdmissionConfig {
  pub(crate) const fn window(self) -> Duration {
    Duration::from_millis(self.window_milliseconds)
  }

  pub(crate) const fn tracked_identities(self) -> usize {
    self.tracked_identities as usize
  }

  pub(crate) const fn management_requests(self) -> u32 {
    self.management_requests
  }

  pub(crate) const fn expensive_management_requests(self) -> u32 {
    self.expensive_management_requests
  }

  pub(crate) const fn agent_requests(self) -> u32 {
    self.agent_requests
  }

  pub(crate) const fn agent_heartbeats(self) -> u32 {
    self.agent_heartbeats
  }

  pub(crate) const fn webhook_requests(self) -> u32 {
    self.webhook_requests
  }

  pub(crate) const fn is_bounded(self) -> bool {
    self.window_milliseconds > 0
      && self.window_milliseconds <= MAX_WINDOW_MILLISECONDS
      && self.tracked_identities > 0
      && self.tracked_identities <= MAX_TRACKED_IDENTITIES
      && self.management_requests > 0
      && self.management_requests <= MAX_REQUESTS_PER_WINDOW
      && self.expensive_management_requests > 0
      && self.expensive_management_requests <= self.management_requests
      && self.agent_requests > 0
      && self.agent_requests <= MAX_REQUESTS_PER_WINDOW
      && self.agent_heartbeats > 0
      && self.agent_heartbeats <= MAX_REQUESTS_PER_WINDOW
      && self.webhook_requests > 0
      && self.webhook_requests <= MAX_REQUESTS_PER_WINDOW
  }
}
