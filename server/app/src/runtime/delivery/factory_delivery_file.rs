//! Exact deployment-owned Delivery process bindings.
use crate::runtime::{
  FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft, FactoryRuntimeInstallationError, FactoryRuntimeResources,
  HostedFactoryCandidatePublisher,
};
use octacity_server_application::{
  FactoryCandidatePublicationAction, FactoryFlowActionAdapter, FactoryNodeExecutorBinding,
  MAX_FACTORY_EXECUTOR_BINDINGS, SystemFactoryExecutionClock,
};
use octacity_server_delivery::{DeliveryAdapterRegistry, InstalledDeliveryAdapter};
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  FactoryCandidatePublicationChoice, FlowDataSchema, FlowNodeKind, FlowOutcomeDefinition, ImmutableReference,
  NodeExecutionIdentity,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runtime) struct FactoryFileDeliveryProfile {
  identity: ImmutableReference,
  outcome: FlowOutcomeDefinition,
  receipt_schema: Option<FlowDataSchema>,
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
  review_links: Option<ReviewLinkProvider>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewLinkProvider {
  Github,
}

pub(super) fn install_github_review_links(
  policy: &mut FactoryDeliveryPolicy,
  adapter: &InstalledDeliveryAdapter,
) -> Result<(), FactoryRuntimeInstallationError> {
  use FactoryRuntimeInstallationError::Invalid;
  let github = octacity_github::GithubPlugin::load(adapter.working_directory().ok_or(Invalid)?).map_err(|_| Invalid)?;
  let draft = &policy.draft;
  let hosts = draft
    .network_hosts
    .iter()
    .map(|host| host.as_str().to_owned())
    .collect::<Vec<_>>();
  let links = github
    .delivery_review_links(
      &draft.configuration_digest.to_string(),
      &draft.repository_locator,
      &draft.repository_id.to_string(),
      &hosts,
    )
    .map_err(|_| Invalid)?;
  policy.review_links = Some(Box::new(move |identity| {
    links
      .resolve(identity)
      .map_err(|_| octacity_server_application::ApplicationError::invalid())
  }));
  Ok(())
}

impl FactoryFileDeliveryProfile {
  pub(in crate::runtime) fn install(
    &self,
    resources: &FactoryRuntimeResources,
  ) -> Result<
    (
      FactoryNodeExecutorBinding,
      Vec<(ProjectId, FactoryCandidatePublicationChoice)>,
    ),
    FactoryRuntimeInstallationError,
  > {
    use FactoryRuntimeInstallationError::Invalid;
    if self.policies.is_empty()
      || self.policies.len() > MAX_FACTORY_EXECUTOR_BINDINGS
      || !self.adapter_root.is_absolute()
      || !(1..=300_000).contains(&self.timeout_milliseconds)
      || !(1..=30_000).contains(&self.termination_grace_milliseconds)
      || self
        .receipt_schema
        .as_ref()
        .is_some_and(|schema| schema.reference() != self.outcome.schema())
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
    let mut choices = Vec::new();
    for selected in &self.policies {
      let mut policy = FactoryDeliveryPolicy::new(
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
      if let Some(ReviewLinkProvider::Github) = selected.review_links {
        install_github_review_links(&mut policy, adapter)?;
      }
      choices.push((
        selected.draft.project_id,
        FactoryCandidatePublicationChoice {
          plugin: self.identity.clone(),
          target: selected.reference.clone(),
          repository_id: selected.draft.repository_id,
          outcome: self.outcome.clone(),
          receipt_schema: self.receipt_schema.clone(),
          review_url: policy.review_links.is_some(),
          permissions: policy.required_permissions(Some(&self.identity)).map_err(|_| Invalid)?,
        },
      ));
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
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(self.identity.clone()),
        Arc::new(FactoryFlowActionAdapter::new(Arc::new(
          FactoryCandidatePublicationAction::new(self.identity.clone(), Arc::new(publisher), self.outcome.clone())
            .with_run_store(resources.runs.clone()),
        ))),
      ),
      choices,
    ))
  }
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runtime::{FactoryDeliveryPolicy, FactoryDeliveryPolicyDraft, FactoryDeliveryTargetPolicy};
  use octacity_delivery_protocol::ReviewMetadata;
  use octacity_server_domain::RepositoryId;
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
    let receipt = FlowDataSchema::new(
      FactoryKey::new("receipt").unwrap(),
      FactoryKey::new("v1").unwrap(),
      FlowValueSchema::String {
        min_bytes: 1,
        max_bytes: 64,
      },
    )
    .unwrap();
    let outcome = FlowOutcomeDefinition::new(
      FactoryKey::new("published").unwrap(),
      FlowOutcomeKind::Success,
      receipt.reference().clone(),
    );
    let document = serde_json::json!({
      "identity": identity, "outcome": outcome, "receipt_schema": receipt,
      "adapter_root": adapters, "timeout_milliseconds": 1000, "termination_grace_milliseconds": 50,
      "policies": [{ "reference": target.reference(), "draft": draft, "input_directory": inputs }]
    });
    let catalog = serde_json::json!({
      "version": 1,
      "projects": [
        {"project_id":draft.project_id,"references":[{"kind":"node_capability","alias":"publisher","reference":identity}],"build_configurations":[]},
        {"project_id":ProjectId::generate(),"references":[{"kind":"node_capability","alias":"publisher","reference":identity}],"build_configurations":[]}
      ],
      "codex": [], "delivery": [document.clone()],
    });
    let catalog_path = root.path().join("profiles.json");
    fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    fs::set_permissions(&catalog_path, fs::Permissions::from_mode(0o600)).unwrap();
    let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::load(&catalog_path, &[])
      .await
      .expect("the deployment file retains the exact installed Delivery profile");
    let installation =
      crate::runtime::FactoryRuntimeInstaller::install(&installer, crate::runtime::factory::factory_tests::resources())
        .await
        .unwrap();
    assert_eq!(installation.executors.len(), 1);
    assert_eq!(
      installation.executors[0].1,
      NodeExecutionIdentity::External(identity.clone())
    );
    let octacity_server_store::FactoryConfigurationAvailability::Available(choices) =
      installation.capabilities.availability(draft.project_id).await.unwrap()
    else {
      panic!("the installed target's Project must be available")
    };
    let [choice] = choices.candidate_publications() else {
      panic!("expected one Project target")
    };
    assert_eq!(choice.plugin, identity);
    assert_eq!(&choice.target, target.reference());
    assert_eq!(choice.outcome, outcome);
    assert_eq!(choice.receipt_schema.as_ref(), Some(&receipt));
    let advertised = serde_json::to_value(choice).unwrap();
    let permissions = &advertised["permissions"];
    assert!(
      permissions["plugins"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!(draft.plugin))
    );
    assert!(
      permissions["plugins"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!(identity))
    );
    assert_eq!(permissions["secret_profiles"], serde_json::json!(["delivery-only"]));
    assert_eq!(permissions["network_hosts"], serde_json::json!(["forge.example"]));
    assert!(!advertised.to_string().contains(&draft.credential_handle));
    assert!(!choice.review_url);
    let other_project = serde_json::from_value(catalog["projects"][1]["project_id"].clone()).unwrap();
    let octacity_server_store::FactoryConfigurationAvailability::Available(other) =
      installation.capabilities.availability(other_project).await.unwrap()
    else {
      panic!("the other Project must be available")
    };
    assert!(other.candidate_publications().is_empty());
    assert!(
      !marker.exists(),
      "loading the native deployment does not execute an adapter"
    );
    let profile: FactoryFileDeliveryProfile = serde_json::from_value(document.clone()).unwrap();
    let (installed, _) = profile
      .install(&crate::runtime::factory::factory_tests::resources())
      .unwrap();
    assert_eq!(installed.0, FlowNodeKind::TrustedAction);
    assert_eq!(installed.1, NodeExecutionIdentity::External(identity));
    assert!(!marker.exists(), "installing authority must not execute the adapter");
    let mut legacy = document.clone();
    legacy.as_object_mut().unwrap().remove("receipt_schema");
    let legacy: FactoryFileDeliveryProfile = serde_json::from_value(legacy).unwrap();
    let (_, installed_targets) = legacy
      .install(&crate::runtime::factory::factory_tests::resources())
      .unwrap();
    assert_eq!(
      installed_targets.len(),
      1,
      "publication-only installations still own an exact target"
    );
    assert_eq!(&installed_targets[0].1.target, target.reference());
    assert_eq!(installed_targets[0].1.outcome, outcome);
    assert!(installed_targets[0].1.receipt_schema.is_none());
    let mut same_adapter_and_action = document.clone();
    same_adapter_and_action["identity"] = serde_json::to_value(&draft.plugin).unwrap();
    let profile: FactoryFileDeliveryProfile = serde_json::from_value(same_adapter_and_action).unwrap();
    let (_, targets) = profile
      .install(&crate::runtime::factory::factory_tests::resources())
      .expect("one installed capability can serve as both action and adapter");
    assert_eq!(
      serde_json::to_value(&targets[0].1.permissions).unwrap()["plugins"],
      serde_json::json!([draft.plugin])
    );
    let mut private_catalog = catalog.clone();
    private_catalog["delivery"][0]
      .as_object_mut()
      .unwrap()
      .remove("receipt_schema");
    private_catalog["projects"][0]["references"] = serde_json::json!([]);
    let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
      &serde_json::to_vec(&private_catalog).unwrap(),
      &[],
    )
    .unwrap();
    let installation =
      crate::runtime::FactoryRuntimeInstaller::install(&installer, crate::runtime::factory::factory_tests::resources())
        .await
        .expect("publication-only profiles can install without a Console choice");
    assert_eq!(installation.executors.len(), 1);
    let octacity_server_store::FactoryConfigurationAvailability::Available(choices) =
      installation.capabilities.availability(draft.project_id).await.unwrap()
    else {
      panic!("the Project must remain available")
    };
    assert!(choices.candidate_publications().is_empty());
    let mut substituted = document;
    substituted["policies"][0]["draft"]["configuration_digest"] = serde_json::json!("06".repeat(32));
    let profile: FactoryFileDeliveryProfile = serde_json::from_value(substituted).unwrap();
    assert!(
      profile
        .install(&crate::runtime::factory::factory_tests::resources())
        .is_err()
    );
    assert!(!marker.exists());
  }
}
