//! Exact deployment-owned Delivery process bindings.
use super::{
  FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft, FactoryRuntimeInstallationError, FactoryRuntimeResources,
  HostedFactoryCandidatePublisher,
};
use octacity_server_application::{
  FactoryCandidatePublicationAction, FactoryFlowActionAdapter, FactoryNodeExecutorBinding,
  MAX_FACTORY_EXECUTOR_BINDINGS, SystemFactoryExecutionClock,
};
use octacity_server_delivery::DeliveryAdapterRegistry;
use octacity_server_factory::{FlowNodeKind, FlowOutcomeDefinition, ImmutableReference, NodeExecutionIdentity};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryFileDeliveryProfile {
  identity: ImmutableReference,
  outcome: FlowOutcomeDefinition,
  adapter_root: PathBuf,
  policies: Vec<FactoryFileDeliveryPolicy>,
  timeout_milliseconds: u64,
  termination_grace_milliseconds: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FactoryFileDeliveryPolicy {
  reference: ImmutableReference,
  draft: FactoryDeliveryPolicyDraft,
  input_directory: PathBuf,
}

impl FactoryFileDeliveryProfile {
  pub(super) fn install(
    &self,
    resources: &FactoryRuntimeResources,
  ) -> Result<FactoryNodeExecutorBinding, FactoryRuntimeInstallationError> {
    use FactoryRuntimeInstallationError::Invalid;
    if self.policies.is_empty()
      || self.policies.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || !self.adapter_root.is_absolute()
      || !(1..=300_000).contains(&self.timeout_milliseconds)
      || !(1..=30_000).contains(&self.termination_grace_milliseconds)
    {
      return Err(Invalid);
    }
    // Registry diagnostics contain installation paths. Keep this private file
    // boundary on the same stable, secret-safe installation error contract.
    let registry = tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
      DeliveryAdapterRegistry::discover(&self.adapter_root)
    })
    .map_err(|_| Invalid)?;
    let mut policies = Vec::with_capacity(self.policies.len());
    let mut directories = BTreeMap::new();
    for selected in &self.policies {
      let policy = FactoryDeliveryPolicy::new(
        selected.reference.identity().clone(),
        selected.reference.version().clone(),
        selected.draft.clone(),
      )
      .map_err(|_| Invalid)?;
      let adapter = registry
        .resolve(
          selected.draft.plugin.identity().as_str(),
          &selected.draft.plugin.digest().to_string(),
        )
        .map_err(|_| Invalid)?;
      if policy.reference() != &selected.reference
        || adapter.manifest.capabilities.configuration_digest != selected.draft.configuration_digest.to_string()
        || !adapter.manifest.capabilities.observe
        || !adapter.manifest.capabilities.publish
        || !adapter.manifest.capabilities.cancel
        || directories
          .insert(selected.reference.clone(), selected.input_directory.clone())
          .is_some()
      {
        return Err(Invalid);
      }
      policies.push(policy);
    }
    let publisher = HostedFactoryCandidatePublisher::new(
      Arc::new(registry),
      policies,
      resources.delivery_materializer(directories)?,
      Arc::new(SystemFactoryExecutionClock),
      Duration::from_millis(self.timeout_milliseconds),
      Duration::from_millis(self.termination_grace_milliseconds),
      tokio_util::sync::CancellationToken::new(),
    )
    .map_err(|_| Invalid)?;
    Ok((
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(self.identity.clone()),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(
        FactoryCandidatePublicationAction::new(self.identity.clone(), Arc::new(publisher), self.outcome.clone()),
      ))),
    ))
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runtime::{FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft, FactoryDeliveryTargetPolicy};
  use octacity_delivery_protocol::ReviewMetadata;
  use octacity_server_domain::{ProjectId, RepositoryId};
  use octacity_server_factory::*;
  use sha2::{Digest as _, Sha256};
  use std::{fs, os::unix::fs::PermissionsExt as _};

  #[tokio::test]
  async fn file_delivery_installs_only_the_exact_verified_adapter_and_target_without_execution() {
    let root = tempfile::tempdir().unwrap();
    let adapters = root.path().join("adapters");
    let entry = adapters.join("isolated-forge");
    fs::create_dir_all(&entry).unwrap();
    let marker = root.path().join("must-not-execute");
    let script = format!("#!/bin/sh\ntouch '{}'\nexit 91\n", marker.display());
    let digest = hex::encode(Sha256::digest(script.as_bytes()));
    fs::write(entry.join("plugin"), script).unwrap();
    fs::set_permissions(entry.join("plugin"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(entry.join("adapter.toml"), format!(
      "manifest_version=1\nadapter_id=\"isolated-forge\"\nexecutable=\"plugin\"\nexecutable_sha256={digest:?}\n[protocol]\nmin=1\nmax=1\n[capabilities]\nobserve=true\npublish=true\ncancel=true\nidempotency=\"observe_before_retry\"\ncredential_class=\"delivery_write\"\nconfiguration_digest={:?}\n", "05".repeat(32)
    )).unwrap();
    let reference = |name: &str, sha: FactoryDigest| {
      ImmutableReference::new(FactoryKey::new(name).unwrap(), FactoryKey::new("v1").unwrap(), sha)
    };
    let draft = FactoryDeliveryPolicyDraft {
      plugin: reference("isolated-forge", digest.parse().unwrap()),
      configuration_digest: FactoryDigest::from_bytes([5; 32]),
      project_id: ProjectId::generate(),
      repository_id: RepositoryId::generate(),
      repository_locator: "repository:configured".to_owned(),
      credential_profile: FactoryKey::new("delivery-only").unwrap(),
      credential_handle: "operator-private-handle".to_owned(),
      network_hosts: vec![NetworkHost::new("forge.example").unwrap()],
      target: FactoryDeliveryTargetPolicy {
        base_reference: "refs/heads/base".to_owned(),
        candidate_reference_prefix: "refs/heads/review/".to_owned(),
      },
      review: ReviewMetadata {
        title: "Accepted candidate".to_owned(),
        summary: "Verified".to_owned(),
        draft: true,
      },
    };
    let target = FactoryDeliveryPolicy::new(
      FactoryKey::new("target-policy").unwrap(),
      FactoryKey::new("v1").unwrap(),
      draft.clone(),
    )
    .unwrap();
    let inputs = root.path().join("private-inputs");
    fs::create_dir(&inputs).unwrap();
    fs::set_permissions(&inputs, fs::Permissions::from_mode(0o700)).unwrap();
    let identity = reference("configured-publication", FactoryDigest::from_bytes([9; 32]));
    let outcome = FlowOutcomeDefinition::new(
      FactoryKey::new("published").unwrap(),
      FlowOutcomeKind::Success,
      reference("receipt", FactoryDigest::from_bytes([8; 32])),
    );
    let document = serde_json::json!({
      "identity": identity, "outcome": outcome,
      "adapter_root": adapters, "timeout_milliseconds": 1000, "termination_grace_milliseconds": 50,
      "policies": [{ "reference": target.reference(), "draft": draft, "input_directory": inputs }]
    });
    let catalog = serde_json::json!({
      "version": 1,
      "projects": [{"project_id":document["policies"][0]["draft"]["project_id"],"references":[],"build_configurations":[]}],
      "codex": [], "delivery": [document.clone()],
    });
    let catalog_path = root.path().join("profiles.json");
    fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    fs::set_permissions(&catalog_path, fs::Permissions::from_mode(0o600)).unwrap();
    let installer = crate::runtime::factory_profile_file::FileProfileInstaller::load(&catalog_path, &[])
      .await
      .expect("the deployment file retains the exact installed Delivery profile");
    let installation =
      crate::runtime::FactoryRuntimeInstaller::install(&installer, crate::runtime::factory_tests::resources())
        .await
        .unwrap();
    assert_eq!(installation.executors.len(), 1);
    assert_eq!(
      installation.executors[0].1,
      NodeExecutionIdentity::External(identity.clone())
    );
    assert!(
      !marker.exists(),
      "loading the native deployment does not execute an adapter"
    );
    let profile: FactoryFileDeliveryProfile = serde_json::from_value(document.clone()).unwrap();
    let installed = profile.install(&crate::runtime::factory_tests::resources()).unwrap();
    assert_eq!(installed.0, FlowNodeKind::TrustedAction);
    assert_eq!(installed.1, NodeExecutionIdentity::External(identity));
    assert!(!marker.exists(), "installing authority must not execute the adapter");
    let mut substituted = document;
    substituted["policies"][0]["draft"]["configuration_digest"] = serde_json::json!("06".repeat(32));
    let profile: FactoryFileDeliveryProfile = serde_json::from_value(substituted).unwrap();
    assert!(profile.install(&crate::runtime::factory_tests::resources()).is_err());
    assert!(!marker.exists());
  }
}
