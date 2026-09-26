use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use octa_plugin_lock::{PLUGIN_LOCK_VERSION, PluginLock};
use serde::{Deserialize, Serialize};

use crate::{
  HarnessError,
  bundle::{
    copy_inventory, read_bounded, regular_directory, regular_file, safe_relative_path, sha256, verify_checksums,
  },
  contract::ProtocolRange,
  invalid,
};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct OctaReleaseContract {
  format_version: u16,
  runner_capabilities: String,
  checksums: String,
  provenance: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Strict capabilities advertised by a released Octa runner.
pub struct RunnerCapabilities {
  pub(crate) octa_version: String,
  runner_protocols: Vec<u16>,
  event_schemas: Vec<u16>,
  plugin_protocols: Vec<u16>,
  octafile_versions: Vec<u8>,
  pub(crate) platform: String,
  features: Vec<String>,
  pub(crate) build_commit: Option<String>,
}

pub(crate) struct OctaBundle {
  root: PathBuf,
  inventory: BTreeMap<PathBuf, String>,
  checksum_name: String,
  pub(crate) capabilities: RunnerCapabilities,
  pub(crate) runner: PathBuf,
  pub(crate) runner_digest: String,
  pub(crate) plugin_protocol: u16,
  pub(crate) plugin_digests: BTreeMap<String, String>,
}

impl OctaBundle {
  pub(crate) fn load(root: &Path) -> Result<Self, HarnessError> {
    regular_directory(root, "Octa release root")?;
    let contract_path = root.join("octa-release-contract.json");
    let contract: OctaReleaseContract = serde_json::from_slice(&read_bounded(&contract_path, MAX_METADATA_BYTES)?)?;
    if contract
      != (OctaReleaseContract {
        format_version: 1,
        runner_capabilities: "octa-runner-capabilities.json".to_owned(),
        checksums: "SHA256SUMS".to_owned(),
        provenance: "github-build-provenance".to_owned(),
      })
    {
      return Err(invalid("unsupported Octa release contract"));
    }
    let inventory = verify_checksums(root, &contract.checksums)?;
    let capabilities: RunnerCapabilities = serde_json::from_slice(&read_bounded(
      &root.join(&contract.runner_capabilities),
      MAX_METADATA_BYTES,
    )?)?;
    capabilities.validate()?;
    let runner = executable(root, &inventory, "octa-runner")?;
    let _octa = executable(root, &inventory, "octa")?;
    let runner_digest = sha256(&runner)?;
    let plugins = verify_plugins(root, &inventory, &capabilities)?;
    Ok(Self {
      root: root.to_owned(),
      inventory,
      checksum_name: contract.checksums,
      capabilities,
      runner,
      runner_digest,
      plugin_protocol: plugins.protocol,
      plugin_digests: plugins.digests,
    })
  }

  pub(crate) fn install(&self, destination: &Path) -> Result<(), HarnessError> {
    copy_inventory(&self.root, &self.inventory, destination, &self.checksum_name)
  }

  pub(crate) fn compatible_runner_protocol(&self, supported: ProtocolRange) -> Result<u16, HarnessError> {
    compatible_version("Octa runner", &self.capabilities.runner_protocols, supported)
  }

  pub(crate) fn compatible_event_schema(&self, supported: ProtocolRange) -> Result<u16, HarnessError> {
    compatible_version("Octa event schema", &self.capabilities.event_schemas, supported)
  }
}

impl RunnerCapabilities {
  /// Octa version advertised by the released runner.
  #[must_use]
  pub fn octa_version(&self) -> &str {
    &self.octa_version
  }

  /// Runtime platform advertised by the released runner.
  #[must_use]
  pub fn platform(&self) -> &str {
    &self.platform
  }

  fn validate(&self) -> Result<(), HarnessError> {
    if self.octa_version.trim().is_empty() || self.platform.trim().is_empty() {
      return Err(invalid("Octa runner capabilities omit version or platform"));
    }
    positive_unique("runner_protocols", &self.runner_protocols)?;
    positive_unique("event_schemas", &self.event_schemas)?;
    positive_unique("plugin_protocols", &self.plugin_protocols)?;
    if self.octafile_versions.is_empty() || self.octafile_versions.contains(&0) {
      return Err(invalid("Octa runner capabilities have invalid Octafile versions"));
    }
    if duplicates(&self.octafile_versions) || duplicates(&self.features) {
      return Err(invalid("Octa runner capabilities contain duplicate values"));
    }
    if self.features.iter().any(|feature| feature.trim().is_empty()) {
      return Err(invalid("Octa runner capabilities contain an empty feature"));
    }
    if self.build_commit.as_deref().is_some_and(|revision| {
      revision.len() != 40
        || !revision
          .bytes()
          .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
      return Err(invalid("Octa runner capabilities contain an invalid build commit"));
    }
    Ok(())
  }
}

pub(crate) struct VerifiedPlugins {
  protocol: u16,
  digests: BTreeMap<String, String>,
}

fn verify_plugins(
  root: &Path,
  inventory: &BTreeMap<PathBuf, String>,
  capabilities: &RunnerCapabilities,
) -> Result<VerifiedPlugins, HarnessError> {
  let lock_path = root.join("Octa.lock");
  let lock: PluginLock = serde_yaml_ng::from_slice(&read_bounded(&lock_path, MAX_METADATA_BYTES)?)?;
  if lock.version != PLUGIN_LOCK_VERSION || lock.plugins.is_empty() {
    return Err(invalid("Octa.lock has an unsupported version or empty plugin set"));
  }
  let mut protocol = None;
  let mut digests = BTreeMap::new();
  for (name, plugin) in lock.plugins {
    if name.trim().is_empty()
      || plugin.version.trim().is_empty()
      || !capabilities.plugin_protocols.contains(&plugin.protocol)
      || !plugin.platforms.contains(&capabilities.platform)
    {
      return Err(invalid(format!("Octa plugin '{name}' is incompatible with its runner")));
    }
    let entrypoint = safe_relative_path(
      plugin
        .entrypoint
        .to_str()
        .ok_or_else(|| invalid(format!("Octa plugin '{name}' entrypoint is not UTF-8")))?,
    )?;
    let relative = Path::new("plugins").join(entrypoint);
    if !inventory.contains_key(&relative) {
      return Err(invalid(format!("Octa plugin '{name}' is absent from SHA256SUMS")));
    }
    let executable = root.join(&relative);
    regular_file(&executable, "Octa plugin executable")?;
    if sha256(&executable)? != plugin.sha256 {
      return Err(invalid(format!(
        "Octa plugin '{name}' executable digest differs from Octa.lock"
      )));
    }
    if protocol
      .replace(plugin.protocol)
      .is_some_and(|current| current != plugin.protocol)
    {
      return Err(invalid("Octa plugins do not share one runner protocol"));
    }
    digests.insert(name.clone(), plugin.sha256.clone());
    let source = Path::new("plugins").join(safe_relative_path(&plugin.source)?);
    if !inventory.contains_key(&source) {
      return Err(invalid(format!(
        "Octa plugin '{name}' manifest is absent from SHA256SUMS"
      )));
    }
    regular_file(&root.join(source), "Octa plugin manifest")?;
  }
  Ok(VerifiedPlugins {
    protocol: protocol.expect("non-empty plugin lock has a protocol"),
    digests,
  })
}

fn executable(root: &Path, inventory: &BTreeMap<PathBuf, String>, stem: &str) -> Result<PathBuf, HarnessError> {
  let candidates = [PathBuf::from(stem), PathBuf::from(format!("{stem}.exe"))];
  let present = candidates
    .into_iter()
    .filter(|path| inventory.contains_key(path))
    .collect::<Vec<_>>();
  if present.len() != 1 {
    return Err(invalid(format!(
      "Octa bundle must contain exactly one '{stem}' executable"
    )));
  }
  let executable = root.join(&present[0]);
  regular_file(&executable, "Octa executable")?;
  Ok(executable)
}

fn positive_unique(name: &str, values: &[u16]) -> Result<(), HarnessError> {
  if values.is_empty() || values.contains(&0) || duplicates(values) {
    return Err(invalid(format!("Octa runner capabilities have invalid {name}")));
  }
  Ok(())
}

fn compatible_version(name: &str, versions: &[u16], supported: ProtocolRange) -> Result<u16, HarnessError> {
  if let Some(version) = versions
    .iter()
    .copied()
    .find(|version| supported.min <= *version && *version <= supported.max)
  {
    Ok(version)
  } else {
    Err(invalid(format!(
      "{name} has no version compatible with the Agent release contract"
    )))
  }
}

fn duplicates<T: Ord + Clone>(values: &[T]) -> bool {
  values.iter().cloned().collect::<BTreeSet<_>>().len() != values.len()
}
