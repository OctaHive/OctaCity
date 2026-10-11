use std::{collections::BTreeMap, str::FromStr};

use octacity_protocol::{FactoryEnforcementCapabilityV3, JobBinding, VerifiedJobSpec, verify_compatible_job_spec};
use octacity_server_domain::{AttemptNumber, JobId, PipelineNodeId, Timestamp};
use octacity_server_factory::{LocalPermissionCeiling, PermissionCategory};
use octacity_server_job::{JobSpecSigner, sign_ready_job_spec};

use crate::{
  FactoryBuildPolicyLayers, FactoryManagedJobSpecInput, FactoryManagedJobSpecTemplateError,
  derive_factory_managed_job_spec_template,
};

use crate::factory_job_test_support::{build, canonical_v3_spec, permissions, policy};

#[test]
fn signing_validity_does_not_shorten_the_exact_managed_execution_deadline() {
  let canonical = canonical_v3_spec();
  let grants = permissions(false);
  let template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: build(&canonical),
    pipeline_node_id: PipelineNodeId::new("configured-command").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: grants.clone(),
      configuration: grants.clone(),
      task: grants.clone(),
      local: LocalPermissionCeiling::fully_enforced(grants),
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    policy: policy(&canonical),
  })
  .unwrap();
  let signer = JobSpecSigner::new("factory-key", [9; 32]).unwrap();
  let job = JobId::generate();
  for (deadline, expires_at) in [(600_999, 200), (100_999, 101)] {
    let template = template
      .clone()
      .with_execution_deadline(Timestamp::from_unix_millis(deadline).unwrap())
      .unwrap();
    assert!(
      sign_ready_job_spec(
        &template,
        AttemptNumber::FIRST,
        job,
        Timestamp::from_unix_millis(deadline).unwrap(),
        &signer
      )
      .is_err()
    );
    let signed = sign_ready_job_spec(
      &template,
      AttemptNumber::FIRST,
      job,
      Timestamp::from_unix_millis(100_000).unwrap(),
      &signer,
    )
    .unwrap();
    let verified = verify_compatible_job_spec(
      signed.envelope(),
      &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
      JobBinding {
        job_id: &job.to_string(),
        attempt: 1,
        now: 100,
      },
    )
    .unwrap();
    let VerifiedJobSpec::V3(spec) = verified else {
      panic!("expected v3")
    };
    assert_eq!(
      spec.expires_at, expires_at,
      "signature validity remains the selected policy window"
    );
    assert_eq!(
      serde_json::to_value(spec).unwrap()["execution_deadline_unix_ms"],
      deadline,
      "execution retains its own exact millisecond ceiling, independent of signing validity"
    );
  }
}

#[test]
fn ready_and_retry_signing_cannot_extend_a_frozen_node_deadline() {
  let canonical = canonical_v3_spec();
  let permissions = permissions(false);
  let template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: build(&canonical),
    pipeline_node_id: PipelineNodeId::new("configured-command").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: permissions.clone(),
      configuration: permissions.clone(),
      task: permissions.clone(),
      local: LocalPermissionCeiling::fully_enforced(permissions),
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    policy: policy(&canonical),
  })
  .unwrap()
  .with_execution_deadline(Timestamp::from_unix_millis(120_000).unwrap())
  .unwrap();
  let signer = JobSpecSigner::new("factory-key", [9; 32]).unwrap();
  let job = JobId::generate();
  for (attempt, issued_at) in [(1, 100_000), (2, 115_000)] {
    let signed = sign_ready_job_spec(
      &template,
      AttemptNumber::new(attempt).unwrap(),
      job,
      Timestamp::from_unix_millis(issued_at).unwrap(),
      &signer,
    )
    .unwrap();
    let verified = verify_compatible_job_spec(
      signed.envelope(),
      &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
      JobBinding {
        job_id: &job.to_string(),
        attempt: attempt as u32,
        now: issued_at as u64 / 1_000,
      },
    )
    .unwrap();
    let VerifiedJobSpec::V3(spec) = verified else {
      panic!("expected managed execution")
    };
    assert_eq!(
      spec.expires_at, 120,
      "a later ordinary Attempt retains the original deadline"
    );
  }
  assert!(
    sign_ready_job_spec(
      &template,
      AttemptNumber::FIRST,
      job,
      Timestamp::from_unix_millis(120_000).unwrap(),
      &signer
    )
    .is_err()
  );
}

#[test]
fn factory_inputs_can_only_narrow_the_project_policy_before_v3_signing() {
  let canonical = canonical_v3_spec();
  let narrow = permissions(false);
  let broad = permissions(true);
  let template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: build(&canonical),
    pipeline_node_id: PipelineNodeId::new("factory-implementation").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: narrow.clone(),
      configuration: broad.clone(),
      task: broad.clone(),
      local: LocalPermissionCeiling::fully_enforced(broad),
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    policy: policy(&canonical),
  })
  .unwrap();
  let signer = JobSpecSigner::new("factory-key", [9; 32]).unwrap();
  let job_id = JobId::from_str("00000000-0000-0000-0000-000000000099").unwrap();
  let signed = sign_ready_job_spec(
    &template,
    AttemptNumber::FIRST,
    job_id,
    Timestamp::from_unix_millis(100_000).unwrap(),
    &signer,
  )
  .unwrap();
  let verified = verify_compatible_job_spec(
    signed.envelope(),
    &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
    JobBinding {
      job_id: &job_id.to_string(),
      attempt: 1,
      now: 100,
    },
  )
  .unwrap();
  let VerifiedJobSpec::V3(spec) = verified else {
    panic!("Factory managed intent must sign as JobSpec v3");
  };
  assert_eq!(spec.permissions.plugins.len(), 1);
  assert_eq!(spec.permissions.plugins[0].identity, "codex");
  assert_eq!(spec.permissions.network_hosts, ["api.openai.com"]);
  assert_eq!(spec.permissions.outputs.kinds, ["codex-result"]);
  let payload = String::from_utf8(
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &signed.envelope().payload).unwrap(),
  )
  .unwrap();
  for forbidden in ["evil.example", "shell", "transfer_url", "download_url", "bearer"] {
    assert!(!payload.contains(forbidden), "signed payload retained {forbidden}");
  }
}

#[test]
fn incomplete_local_enforcement_fails_before_a_v3_template_exists() {
  let canonical = canonical_v3_spec();
  let narrow = permissions(false);
  let error = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: build(&canonical),
    pipeline_node_id: PipelineNodeId::new("factory-implementation").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: narrow.clone(),
      configuration: narrow.clone(),
      task: narrow.clone(),
      local: LocalPermissionCeiling::try_new(
        narrow,
        PermissionCategory::ALL
          .into_iter()
          .filter(|category| *category != PermissionCategory::NetworkHosts)
          .collect(),
      )
      .unwrap(),
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    policy: policy(&canonical),
  })
  .unwrap_err();

  assert!(matches!(error, FactoryManagedJobSpecTemplateError::Permissions));
}
