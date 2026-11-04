//! Original normalized intake intent retained before mutable VCS resolution.
use crate::IdempotencyKey;
pub use octacity_server_domain::RepositorySourceSelection;
use octacity_server_domain::{ProjectId, RepositoryId, RepositoryVersion, Timestamp};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, WorkArtifacts,
  WorkClassification,
};
use serde::{Deserialize, Serialize};

/// Work admission source, distinct from ordinary repository/VCS selection.
/// Repository forms keep their original transport and replay representation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FactoryAdmissionSource {
  /// Existing repository reference or revision selection.
  Repository(RepositorySourceSelection),
  /// Explicit versioned Project/input admission without VCS.
  ProjectInput(ProjectInputSource),
}

/// Version discriminator for repository-free Work admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ProjectInputSource {
  /// Version one has no repository selection fields.
  #[serde(rename = "project_input_v1")]
  V1,
}

impl From<RepositorySourceSelection> for FactoryAdmissionSource {
  fn from(source: RepositorySourceSelection) -> Self {
    Self::Repository(source)
  }
}
impl FactoryAdmissionSource {
  /// Requires a real VCS selection at repository-only preparation boundaries.
  pub fn repository(&self) -> Result<&RepositorySourceSelection, crate::StoreError> {
    match self {
      Self::Repository(source) => Ok(source),
      Self::ProjectInput(_) => Err(crate::StoreError::invalid(
        crate::StoreOperation::AdmitFactoryWork,
        crate::StoreInputError::InvalidFactoryAdmission,
      )),
    }
  }
}

/// Provider-neutral intent to admit one immutable Work Envelope.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FactoryWorkAdmissionIntent {
  /// Existing Project that must own every selected definition.
  pub project_id: ProjectId,
  /// Exact immutable Factory Configuration selected for the run.
  pub configuration_id: FactoryConfigurationId,
  /// Exact immutable Factory Configuration version selected for the run.
  pub configuration_version: FactoryConfigurationVersion,
  /// Repository identity for repository selection; absent only for Project/input mode.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub repository_id: Option<RepositoryId>,
  /// Exact Repository definition version; paired with `repository_id`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub repository_version: Option<RepositoryVersion>,
  /// Stable Work Source identity within the accepted management security scope.
  pub external_identity: ExternalWorkIdentity,
  /// Versioned Project/input mode or an allowed repository source selection.
  pub source: FactoryAdmissionSource,
  /// Immutable task, acceptance, and optional specification Artifact references.
  pub artifacts: WorkArtifacts,
  /// Bounded priority, risk, and source metadata.
  pub classification: WorkClassification,
  /// Explicit exact stage choices; omission preserves existing admission bytes and replay identity.
  #[serde(
    default,
    skip_serializing_if = "octacity_server_factory::FlowProfileAdmissionOverrides::is_empty"
  )]
  pub profile_overrides: octacity_server_factory::FlowProfileAdmissionOverrides,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
}

impl FactoryWorkAdmissionIntent {
  /// Validates the explicit source form before authorization, store or VCS lookup.
  /// Optional wire fields retain the historical repository encoding unchanged.
  pub fn validate_source(&self) -> Result<(), crate::StoreError> {
    let valid = match self.source {
      FactoryAdmissionSource::ProjectInput(_) => self.repository_id.is_none() && self.repository_version.is_none(),
      _ => self.repository_id.is_some() && self.repository_version.is_some(),
    };
    valid.then_some(()).ok_or_else(|| {
      crate::StoreError::invalid(
        crate::StoreOperation::AdmitFactoryWork,
        crate::StoreInputError::InvalidFactoryAdmission,
      )
    })
  }
}

/// Hashes the original normalized intent independently of mutable VCS resolution.
pub fn factory_admission_intent_digest(command: &FactoryWorkAdmissionIntent, domain: &str) -> FactoryDigest {
  let mut fields = vec![
    command.project_id.to_string().into_bytes(),
    command.configuration_id.to_string().into_bytes(),
    command.configuration_version.get().to_string().into_bytes(),
  ];
  if let Some(repository) = command.repository_id {
    fields.push(repository.to_string().into_bytes());
  }
  if let Some(version) = command.repository_version {
    fields.push(version.get().to_string().into_bytes());
  }
  fields.push(command.external_identity.as_str().as_bytes().to_vec());
  match &command.source {
    FactoryAdmissionSource::ProjectInput(_) => fields.push(b"project_input.v1".to_vec()),
    FactoryAdmissionSource::Repository(source) => match source {
      RepositorySourceSelection::DefaultReference => fields.push(b"default_reference".to_vec()),
      RepositorySourceSelection::Reference(reference) => {
        fields.push(b"reference".to_vec());
        fields.push(reference.as_str().as_bytes().to_vec());
      }
      RepositorySourceSelection::ExactRevision(revision) => {
        fields.push(b"exact_revision".to_vec());
        fields.push(revision.as_str().as_bytes().to_vec());
      }
    },
  }
  append_artifact_identity(&mut fields, command.artifacts.task());
  append_artifact_identity(&mut fields, command.artifacts.acceptance());
  for specification in command.artifacts.specifications() {
    append_artifact_identity(&mut fields, specification);
  }
  fields.push(command.classification.priority().get().to_string().into_bytes());
  fields.push(command.classification.risk().as_str().as_bytes().to_vec());
  for (key, value) in command.classification.metadata().iter() {
    fields.push(key.as_bytes().to_vec());
    fields.push(value.as_bytes().to_vec());
  }
  if !command.profile_overrides.is_empty() {
    fields.push(b"profile_overrides.v1".to_vec());
    for entry in command.profile_overrides.entries() {
      fields.push(entry.stage.root.id().to_string().into_bytes());
      fields.push(entry.stage.root.version().get().to_string().into_bytes());
      fields.push(entry.stage.root.digest().as_bytes().to_vec());
      fields.push(entry.stage.calls.len().to_string().into_bytes());
      fields.extend(entry.stage.calls.iter().map(|call| call.as_str().as_bytes().to_vec()));
      fields.push(entry.stage.node.as_str().as_bytes().to_vec());
      fields.push(entry.profile.project_id.to_string().into_bytes());
      fields.push(entry.profile.id.to_string().into_bytes());
      fields.push(entry.profile.version.get().to_string().into_bytes());
      fields.push(entry.profile.digest.as_bytes().to_vec());
    }
  }
  let fields = fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
  FactoryDigest::sha256(domain, &fields)
}

fn append_artifact_identity(fields: &mut Vec<Vec<u8>>, artifact: &octacity_server_factory::FactoryArtifactReference) {
  fields.push(artifact.artifact_id().to_string().into_bytes());
  fields.push(artifact.content_digest().as_bytes().to_vec());
  fields.push(artifact.encoded_size().to_be_bytes().to_vec());
}

/// Canonical source probe shared by direct and prepared ingress; credentials and observation edits are absent.
pub fn factory_source_admission_probe(
  identity: crate::FactoryAdmissionIdentity,
  intent: &FactoryWorkAdmissionIntent,
) -> Result<crate::FactoryAdmissionProbe, crate::StoreError> {
  // Transport keys share a wider partition than item identities, so scope both explicitly.
  let transport = FactoryDigest::sha256(
    "octacity.factory.source-transport.v1",
    &[
      identity.source_scope.source.as_str().as_bytes(),
      identity.external_identity.as_str().as_bytes(),
      intent.idempotency_key.as_str().as_bytes(),
    ],
  );
  Ok(crate::FactoryAdmissionProbe {
    source_scope: identity.source_scope,
    external_identity: identity.external_identity,
    intent_digest: factory_admission_intent_digest(intent, "octacity.factory.source-admission.v1"),
    idempotency_key: IdempotencyKey::new(format!("source.{transport}")).map_err(|_| crate::source_conflict())?,
  })
}
