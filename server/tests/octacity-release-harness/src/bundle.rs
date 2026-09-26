use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::Read as _,
  path::{Component as PathComponent, Path, PathBuf},
  process::Command,
};

use octacity_source_plugin::{SOURCE_PLUGIN_MANIFEST_VERSION, SourcePluginManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
  HarnessError,
  contract::{ProtocolRange, ReleaseContract},
  invalid,
};

const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 4 * 1024 * 1024;
const MAX_BUNDLE_FILES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// Strict identity and compatibility document shipped with one product bundle.
pub struct ProductManifest {
  format_version: u16,
  pub(crate) product: String,
  pub(crate) version: String,
  pub(crate) platform: String,
  pub(crate) build_inputs: BuildInputs,
  pub(crate) protocols: BTreeMap<String, ProtocolRange>,
  components: BTreeMap<String, ManifestComponent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuildInputs {
  pub(crate) octacity_revision: String,
  #[serde(default)]
  pub(crate) octa_revision: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ManifestComponent {
  path: String,
  sha256: String,
}

pub(crate) struct Bundle {
  root: PathBuf,
  inventory: BTreeMap<PathBuf, String>,
  pub(crate) manifest: ProductManifest,
  source_plugin: Option<SourcePluginManifest>,
}

impl Bundle {
  pub(crate) fn load(
    root: &Path,
    expected_product: &str,
    canonical_contract: &ReleaseContract,
  ) -> Result<Self, HarnessError> {
    regular_directory(root, "release root")?;
    let contract_path = root.join("release-contract.json");
    let bundled_contract = ReleaseContract::parse(&read_bounded(&contract_path, MAX_METADATA_BYTES)?)?;
    if &bundled_contract != canonical_contract {
      return Err(invalid(
        "bundled OctaCity release contract differs from the harness contract",
      ));
    }
    let inventory = verify_checksums(root, &canonical_contract.checksums)?;
    let manifest_path = root.join(&canonical_contract.manifest);
    let manifest: ProductManifest = serde_json::from_slice(&read_bounded(&manifest_path, MAX_METADATA_BYTES)?)?;
    manifest.validate(root, expected_product, canonical_contract)?;
    let mut bundle = Self {
      root: root.to_owned(),
      inventory,
      manifest,
      source_plugin: None,
    };
    for (name, component) in &bundle.manifest.components {
      let path = bundle.checked_path(&component.path)?;
      if sha256(&path)? != component.sha256 {
        return Err(invalid(format!(
          "component '{name}' digest differs from release-manifest.json"
        )));
      }
    }
    if expected_product == "octacity-agent" {
      bundle.source_plugin = Some(bundle.verify_source_plugin()?);
    }
    Ok(bundle)
  }

  pub(crate) fn install(&self, destination: &Path) -> Result<(), HarnessError> {
    copy_inventory(&self.root, &self.inventory, destination, "SHA256SUMS")
  }

  pub(crate) fn component(&self, name: &str) -> Result<PathBuf, HarnessError> {
    let component = self
      .manifest
      .components
      .get(name)
      .ok_or_else(|| invalid(format!("release manifest has no '{name}' component")))?;
    self.checked_path(&component.path)
  }

  pub(crate) fn verify_binary_version(&self, component: &str) -> Result<(), HarnessError> {
    let executable = self.component(component)?;
    let output = Command::new(&executable)
      .arg("--version")
      .output()
      .map_err(|source| HarnessError::Io {
        path: executable.clone(),
        source,
      })?;
    let expected = format!("{} {}", self.manifest.product, self.manifest.version);
    let actual = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || actual.trim() != expected {
      return Err(invalid(format!(
        "{} --version did not report '{expected}'",
        executable.display()
      )));
    }
    Ok(())
  }

  pub(crate) fn source_plugin(&self) -> Result<&SourcePluginManifest, HarnessError> {
    self
      .source_plugin
      .as_ref()
      .ok_or_else(|| invalid("release bundle has no verified source plugin"))
  }

  fn checked_path(&self, relative: &str) -> Result<PathBuf, HarnessError> {
    let relative = safe_relative_path(relative)?;
    if !self.inventory.contains_key(&relative) {
      return Err(invalid(format!(
        "manifest component '{}' is absent from SHA256SUMS",
        relative.display()
      )));
    }
    let path = self.root.join(&relative);
    regular_file(&path, "manifest component")?;
    Ok(path)
  }

  fn verify_source_plugin(&self) -> Result<SourcePluginManifest, HarnessError> {
    let manifest_path = self.root.join("source-plugins/git/plugin.toml");
    let source = read_bounded(&manifest_path, MAX_METADATA_BYTES)?;
    let source = std::str::from_utf8(&source).map_err(|_| invalid("source-plugin manifest is not UTF-8"))?;
    let manifest = SourcePluginManifest::from_toml(source)?;
    if manifest.manifest_version != SOURCE_PLUGIN_MANIFEST_VERSION || manifest.name != "git" {
      return Err(invalid("unsupported Git source-plugin manifest"));
    }
    if manifest.version != self.manifest.version {
      return Err(invalid(
        "Git source-plugin version differs from the Agent release version",
      ));
    }
    let expected_platform = runtime_platform(&self.manifest.platform)?;
    if manifest.platforms.as_slice() != [expected_platform] {
      return Err(invalid(
        "Git source-plugin platform differs from the Agent release platform",
      ));
    }
    let supported = self.manifest.protocol("source_plugin")?;
    let installed = ProtocolRange {
      min: manifest.protocol_min,
      max: manifest.protocol_max,
    };
    if !supported.overlaps(installed) {
      return Err(invalid(
        "Git source-plugin protocol range is incompatible with the Agent",
      ));
    }
    let executable_relative = Path::new("source-plugins/git").join(safe_relative_path(&manifest.executable)?);
    let component_relative = safe_relative_path(
      &self
        .manifest
        .components
        .get("source_git")
        .ok_or_else(|| invalid("Agent release manifest has no 'source_git' component"))?
        .path,
    )?;
    if executable_relative != component_relative {
      return Err(invalid(
        "Git source-plugin manifest and release component identify different executables",
      ));
    }
    let executable = self.root.join(&executable_relative);
    regular_file(&executable, "source-plugin executable")?;
    if sha256(&executable)? != manifest.sha256 {
      return Err(invalid(
        "Git source-plugin executable digest does not match plugin.toml",
      ));
    }
    Ok(manifest)
  }
}

impl ProductManifest {
  /// Product name recorded by the release manifest.
  #[must_use]
  pub fn product(&self) -> &str {
    &self.product
  }

  /// Product version recorded by the release manifest.
  #[must_use]
  pub fn version(&self) -> &str {
    &self.version
  }

  /// Distribution platform recorded by the release manifest.
  #[must_use]
  pub fn platform(&self) -> &str {
    &self.platform
  }

  /// Exact OctaCity source revision used to build the product.
  #[must_use]
  pub fn octacity_revision(&self) -> &str {
    &self.build_inputs.octacity_revision
  }

  /// Exact Octa source revision required by this product, when applicable.
  #[must_use]
  pub fn octa_revision(&self) -> Option<&str> {
    self.build_inputs.octa_revision.as_deref()
  }

  pub(crate) fn protocol(&self, name: &str) -> Result<ProtocolRange, HarnessError> {
    self
      .protocols
      .get(name)
      .copied()
      .ok_or_else(|| invalid(format!("{} manifest has no '{name}' protocol", self.product)))
  }

  fn validate(&self, root: &Path, expected_product: &str, contract: &ReleaseContract) -> Result<(), HarnessError> {
    if self.format_version != 1 || self.product != expected_product {
      return Err(invalid(format!("unexpected product manifest in {}", root.display())));
    }
    if !release_token(&self.version) || !release_token(&self.platform) {
      return Err(invalid(format!(
        "{} manifest has an invalid version or platform",
        self.product
      )));
    }
    if !revision(&self.build_inputs.octacity_revision)
      || self
        .build_inputs
        .octa_revision
        .as_deref()
        .is_some_and(|value| !revision(value))
    {
      return Err(invalid(format!(
        "{} manifest has an invalid source revision",
        self.product
      )));
    }
    let product = contract.product(expected_product)?;
    if self.protocols != product.protocols {
      return Err(invalid(format!(
        "{} manifest protocol ranges differ from release-contract.json",
        self.product
      )));
    }
    let components = self.components.keys().cloned().collect::<BTreeSet<_>>();
    if components != product.required_components {
      return Err(invalid(format!(
        "{} manifest component set differs from release-contract.json",
        self.product
      )));
    }
    for (name, component) in &self.components {
      safe_relative_path(&component.path)?;
      if !digest(&component.sha256) {
        return Err(invalid(format!("component '{name}' has an invalid SHA-256")));
      }
    }
    Ok(())
  }
}

pub(crate) fn verify_checksums(root: &Path, checksum_name: &str) -> Result<BTreeMap<PathBuf, String>, HarnessError> {
  let checksum_path = root.join(checksum_name);
  let contents = read_bounded(&checksum_path, MAX_CHECKSUM_BYTES)?;
  let contents = std::str::from_utf8(&contents).map_err(|_| invalid("SHA256SUMS is not UTF-8"))?;
  let mut expected = BTreeMap::new();
  for line in contents.lines() {
    let (hash, relative) = line
      .split_once("  ")
      .ok_or_else(|| invalid("SHA256SUMS line must contain two spaces"))?;
    if !digest(hash) {
      return Err(invalid("SHA256SUMS contains an invalid digest"));
    }
    let relative = safe_relative_path(relative)?;
    if relative == Path::new(checksum_name) || expected.insert(relative, hash.to_owned()).is_some() {
      return Err(invalid("SHA256SUMS contains a duplicate or self entry"));
    }
    if expected.len() > MAX_BUNDLE_FILES {
      return Err(invalid(format!("bundle exceeds the {MAX_BUNDLE_FILES}-file limit")));
    }
  }
  if expected.is_empty() {
    return Err(invalid("SHA256SUMS is empty"));
  }
  let actual = collect_files(root, checksum_name)?;
  if expected.keys().collect::<BTreeSet<_>>() != actual.iter().collect::<BTreeSet<_>>() {
    return Err(invalid("SHA256SUMS does not cover the exact bundle file set"));
  }
  for (relative, hash) in &expected {
    let path = root.join(relative);
    if sha256(&path)? != *hash {
      return Err(invalid(format!("checksum mismatch for '{}'", relative.display())));
    }
  }
  Ok(expected)
}

pub(crate) fn copy_inventory(
  root: &Path,
  inventory: &BTreeMap<PathBuf, String>,
  destination: &Path,
  checksum_name: &str,
) -> Result<(), HarnessError> {
  if destination.exists() {
    return Err(invalid(format!(
      "bundle destination already exists: {}",
      destination.display()
    )));
  }
  create_dir(destination)?;
  for relative in inventory
    .keys()
    .map(PathBuf::as_path)
    .chain(std::iter::once(Path::new(checksum_name)))
  {
    let source_path = root.join(relative);
    let target = destination.join(relative);
    if let Some(parent) = target.parent() {
      fs::create_dir_all(parent).map_err(|source| HarnessError::Io {
        path: parent.to_owned(),
        source,
      })?;
    }
    fs::copy(&source_path, &target).map_err(|source| HarnessError::Io {
      path: target.clone(),
      source,
    })?;
    let permissions = fs::metadata(&source_path)
      .map_err(|source| HarnessError::Io {
        path: source_path.clone(),
        source,
      })?
      .permissions();
    fs::set_permissions(&target, permissions).map_err(|source| HarnessError::Io { path: target, source })?;
  }
  Ok(())
}

fn collect_files(root: &Path, checksum_name: &str) -> Result<Vec<PathBuf>, HarnessError> {
  fn visit(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), HarnessError> {
    for entry in fs::read_dir(directory).map_err(|source| HarnessError::Io {
      path: directory.to_owned(),
      source,
    })? {
      let entry = entry.map_err(|source| HarnessError::Io {
        path: directory.to_owned(),
        source,
      })?;
      let path = entry.path();
      let kind = entry.file_type().map_err(|source| HarnessError::Io {
        path: path.clone(),
        source,
      })?;
      if kind.is_symlink() {
        return Err(invalid(format!("bundle contains a symbolic link: {}", path.display())));
      }
      if kind.is_dir() {
        visit(root, &path, files)?;
      } else if kind.is_file() {
        files.push(
          path
            .strip_prefix(root)
            .expect("visited paths stay below root")
            .to_owned(),
        );
      } else {
        return Err(invalid(format!(
          "bundle contains a non-regular entry: {}",
          path.display()
        )));
      }
      if files.len() > MAX_BUNDLE_FILES + 1 {
        return Err(invalid(format!("bundle exceeds the {MAX_BUNDLE_FILES}-file limit")));
      }
    }
    Ok(())
  }

  let mut files = Vec::new();
  visit(root, root, &mut files)?;
  files.retain(|path| path != Path::new(checksum_name));
  files.sort();
  Ok(files)
}

pub(crate) fn sha256(path: &Path) -> Result<String, HarnessError> {
  let mut source = fs::File::open(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })?;
  let mut digest = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let count = source.read(&mut buffer).map_err(|source| HarnessError::Io {
      path: path.to_owned(),
      source,
    })?;
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, HarnessError> {
  regular_file(path, "release metadata")?;
  let metadata = fs::metadata(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })?;
  if metadata.len() > limit {
    return Err(invalid(format!("'{}' exceeds the {limit}-byte limit", path.display())));
  }
  fs::read(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })
}

pub(crate) fn safe_relative_path(value: &str) -> Result<PathBuf, HarnessError> {
  let path = Path::new(value);
  if value.is_empty() || path.is_absolute() {
    return Err(invalid(format!("unsafe release path '{value}'")));
  }
  let mut normalized = PathBuf::new();
  for part in path.components() {
    match part {
      PathComponent::CurDir => {}
      PathComponent::Normal(part) => normalized.push(part),
      _ => return Err(invalid(format!("unsafe release path '{value}'"))),
    }
  }
  if normalized.as_os_str().is_empty() {
    return Err(invalid(format!("unsafe release path '{value}'")));
  }
  Ok(normalized)
}

pub(crate) fn regular_file(path: &Path, label: &str) -> Result<(), HarnessError> {
  let metadata = fs::symlink_metadata(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })?;
  if metadata.file_type().is_symlink() || !metadata.is_file() {
    return Err(invalid(format!(
      "{label} is not a regular non-symlink file: {}",
      path.display()
    )));
  }
  Ok(())
}

pub(crate) fn regular_directory(path: &Path, label: &str) -> Result<(), HarnessError> {
  let metadata = fs::symlink_metadata(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })?;
  if metadata.file_type().is_symlink() || !metadata.is_dir() {
    return Err(invalid(format!(
      "{label} is not a regular directory: {}",
      path.display()
    )));
  }
  Ok(())
}

fn create_dir(path: &Path) -> Result<(), HarnessError> {
  fs::create_dir(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })
}

fn digest(value: &str) -> bool {
  value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn revision(value: &str) -> bool {
  value.len() == 40
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn release_token(value: &str) -> bool {
  !value.is_empty()
    && value
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'_' | b'-'))
}

pub(crate) fn runtime_platform(release_platform: &str) -> Result<&'static str, HarnessError> {
  match release_platform {
    "linux-amd64" => Ok("linux-x86_64"),
    "linux-arm64" => Ok("linux-aarch64"),
    "windows-amd64" => Ok("windows-x86_64"),
    "macos-arm64" => Ok("macos-aarch64"),
    value => Err(invalid(format!("unsupported release platform '{value}'"))),
  }
}

pub(crate) fn octa_runtime_platform(release_platform: &str) -> Result<&'static str, HarnessError> {
  match release_platform {
    "macos-arm64" => Ok("linux-aarch64"),
    value => runtime_platform(value),
  }
}
