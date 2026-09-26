use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{HarnessError, invalid};

const CANONICAL_CONTRACT: &str = include_str!("../../../../packaging/release-contract.json");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReleaseContract {
  format_version: u16,
  pub(crate) manifest: String,
  pub(crate) checksums: String,
  pub(crate) products: BTreeMap<String, ProductContract>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProductContract {
  pub(crate) required_components: BTreeSet<String>,
  pub(crate) protocols: BTreeMap<String, ProtocolRange>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProtocolRange {
  pub(crate) min: u16,
  pub(crate) max: u16,
}

impl ProtocolRange {
  pub(crate) const fn overlaps(self, other: Self) -> bool {
    self.min <= self.max && other.min <= other.max && self.min <= other.max && other.min <= self.max
  }

  fn validate(self, name: &str) -> Result<(), HarnessError> {
    if self.min == 0 || self.min > self.max {
      return Err(invalid(format!(
        "release contract has an invalid '{name}' protocol range"
      )));
    }
    Ok(())
  }
}

impl ReleaseContract {
  pub(crate) fn canonical() -> Result<Self, HarnessError> {
    let contract: Self = serde_json::from_str(CANONICAL_CONTRACT)?;
    contract.validate()?;
    Ok(contract)
  }

  pub(crate) fn parse(contents: &[u8]) -> Result<Self, HarnessError> {
    let contract: Self = serde_json::from_slice(contents)?;
    contract.validate()?;
    Ok(contract)
  }

  pub(crate) fn product(&self, name: &str) -> Result<&ProductContract, HarnessError> {
    self
      .products
      .get(name)
      .ok_or_else(|| invalid(format!("release contract does not define product '{name}'")))
  }

  fn validate(&self) -> Result<(), HarnessError> {
    if self.format_version != 1 || self.manifest != "release-manifest.json" || self.checksums != "SHA256SUMS" {
      return Err(invalid("unsupported OctaCity release contract"));
    }
    for (product, contract) in &self.products {
      if contract.required_components.is_empty() || contract.protocols.is_empty() {
        return Err(invalid(format!("release contract for '{product}' is empty")));
      }
      for (name, range) in &contract.protocols {
        range.validate(name)?;
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::{ProductContract, ProtocolRange, ReleaseContract};

  #[test]
  fn canonical_contract_tracks_compiled_protocol_versions() {
    let contract = ReleaseContract::canonical().unwrap();
    let agent = contract.product("octacity-agent").unwrap();
    let server = contract.product("octacity-server").unwrap();
    assert_exact(agent, "agent", octacity_protocol::AGENT_PROTOCOL_VERSION);
    assert_exact(agent, "artifact", octacity_protocol::ARTIFACT_PROTOCOL_VERSION);
    assert_exact(agent, "coordinator", octacity_protocol::COORDINATOR_PROTOCOL_VERSION);
    assert_exact(agent, "octa_event_schema", octacity_runner::RUNNER_EVENT_SCHEMA_VERSION);
    assert_exact(agent, "octa_runner", octacity_runner::RUNNER_PROTOCOL_VERSION);
    assert_exact(
      agent,
      "source_plugin",
      octacity_source_plugin::SOURCE_PLUGIN_PROTOCOL_VERSION,
    );
    assert_exact(server, "agent", octacity_protocol::AGENT_PROTOCOL_VERSION);
    assert_exact(server, "artifact", octacity_protocol::ARTIFACT_PROTOCOL_VERSION);
    assert_exact(server, "coordinator", octacity_protocol::COORDINATOR_PROTOCOL_VERSION);
    assert_exact(server, "remote_cache", octa_cache_protocol::REMOTE_CACHE_PROTOCOL_V1);
    assert_exact(server, "vcs_provider", octacity_vcs_protocol::VCS_PROTOCOL_VERSION);
    assert_exact(
      server,
      "webhook_provider",
      octacity_webhook_provider_protocol::WEBHOOK_PROTOCOL_VERSION,
    );
  }

  fn assert_exact(product: &ProductContract, name: &str, expected: u16) {
    assert_eq!(
      product.protocols[name],
      ProtocolRange {
        min: expected,
        max: expected,
      }
    );
  }
}
