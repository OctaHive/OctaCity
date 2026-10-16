//! Original normalized intake intent retained before mutable VCS resolution.
use crate::IdempotencyKey;
use octacity_server_domain::{
  ImmutableRevision, ProjectId, RepositoryId, RepositoryVersion, SourceReference, Timestamp,
};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, WorkArtifacts,
  WorkClassification,
};
use serde::{Deserialize, Serialize};
/// Provider-neutral source expression used by manual and installed-source admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepositorySourceSelection {
  /// Resolve the immutable Repository version's configured default reference.
  DefaultReference,
  /// Resolve one explicitly allowed mutable branch or tag.
  Reference(SourceReference),
  /// Verify and use an explicitly supplied immutable revision.
  ExactRevision(ImmutableRevision),
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
  /// Exact Repository identity selected as the Work subject.
  pub repository_id: RepositoryId,
  /// Exact immutable Repository definition version used for source resolution.
  pub repository_version: RepositoryVersion,
  /// Stable Work Source identity within the accepted management security scope.
  pub external_identity: ExternalWorkIdentity,
  /// Allowed mutable reference, configured default, or permitted exact revision.
  pub source: RepositorySourceSelection,
  /// Immutable task, acceptance, and optional specification Artifact references.
  pub artifacts: WorkArtifacts,
  /// Bounded priority, risk, and source metadata.
  pub classification: WorkClassification,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative admission time.
  pub admitted_at: Timestamp,
}

/// Hashes the original normalized intent independently of mutable VCS resolution.
pub fn factory_admission_intent_digest(command: &FactoryWorkAdmissionIntent, domain: &str) -> FactoryDigest {
  let mut fields = vec![
    command.project_id.to_string().into_bytes(),
    command.configuration_id.to_string().into_bytes(),
    command.configuration_version.get().to_string().into_bytes(),
    command.repository_id.to_string().into_bytes(),
    command.repository_version.get().to_string().into_bytes(),
    command.external_identity.as_str().as_bytes().to_vec(),
  ];
  match &command.source {
    RepositorySourceSelection::DefaultReference => fields.push(b"default_reference".to_vec()),
    RepositorySourceSelection::Reference(reference) => {
      fields.push(b"reference".to_vec());
      fields.push(reference.as_str().as_bytes().to_vec());
    }
    RepositorySourceSelection::ExactRevision(revision) => {
      fields.push(b"exact_revision".to_vec());
      fields.push(revision.as_str().as_bytes().to_vec());
    }
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
