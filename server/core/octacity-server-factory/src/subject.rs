use octacity_server_domain::{ImmutableRevision, ProjectId, RepositoryId};
use serde::{Deserialize, Serialize};

use crate::FactoryDigest;

/// Exact immutable repository subject shared across one Factory Run.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactSubject {
  project_id: ProjectId,
  repository_id: RepositoryId,
  base_revision: ImmutableRevision,
}

impl ExactSubject {
  /// Constructs an exact Project-owned repository subject.
  #[must_use]
  pub const fn new(project_id: ProjectId, repository_id: RepositoryId, base_revision: ImmutableRevision) -> Self {
    Self {
      project_id,
      repository_id,
      base_revision,
    }
  }

  /// Canonical domain-separated identity of the exact repository subject.
  #[must_use]
  pub fn digest(&self) -> FactoryDigest {
    FactoryDigest::sha256(
      "octacity.factory.subject.v1",
      &[
        self.project_id.as_uuid().as_bytes(),
        self.repository_id.as_uuid().as_bytes(),
        self.base_revision.as_str().as_bytes(),
      ],
    )
  }

  /// Returns the owning Project.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    self.project_id
  }

  /// Returns the selected Repository.
  #[must_use]
  pub const fn repository_id(&self) -> RepositoryId {
    self.repository_id
  }

  /// Returns the exact immutable base revision.
  #[must_use]
  pub fn base_revision(&self) -> &ImmutableRevision {
    &self.base_revision
  }
}

/// Exact immutable candidate identity derived from one [`ExactSubject`].
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSubject {
  exact: ExactSubject,
  candidate_revision: ImmutableRevision,
  changeset_digest: FactoryDigest,
}

impl CandidateSubject {
  /// Constructs an exact candidate identity.
  #[must_use]
  pub const fn new(
    exact: ExactSubject,
    candidate_revision: ImmutableRevision,
    changeset_digest: FactoryDigest,
  ) -> Self {
    Self {
      exact,
      candidate_revision,
      changeset_digest,
    }
  }

  /// Returns the immutable base subject.
  #[must_use]
  pub const fn exact(&self) -> &ExactSubject {
    &self.exact
  }

  /// Returns the exact immutable candidate revision.
  #[must_use]
  pub const fn candidate_revision(&self) -> &ImmutableRevision {
    &self.candidate_revision
  }

  /// Returns the trusted ChangeSet content digest.
  #[must_use]
  pub const fn changeset_digest(&self) -> FactoryDigest {
    self.changeset_digest
  }
}

/// Immutable Work correlation. Repository availability is never an access grant.
/// The untagged repository variant retains the historical canonical encoding.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WorkSubject {
  /// Historical repository-backed Work.
  Repository(ExactSubject),
  /// Versioned Project/input Work with no repository or revision.
  ProjectInput(ProjectInputSubject),
}

/// Versioned identity of immutable inputs admitted without a repository.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum ProjectInputSubject {
  /// The exact bounded Work Artifact manifest, scoped to its owning Project.
  #[serde(rename = "project_input_v1")]
  V1 {
    /// Owning Project.
    project_id: ProjectId,
    /// Canonical admitted Work Artifact manifest identity.
    input_digest: FactoryDigest,
  },
}

impl WorkSubject {
  /// Retains the authoritative input manifest without inventing VCS identities.
  pub fn project_input(project_id: ProjectId, artifacts: &crate::WorkArtifacts) -> Result<Self, crate::FactoryError> {
    let bytes = serde_json::to_vec(artifacts).map_err(|_| crate::FactoryError::InconsistentSubject)?;
    Ok(Self::ProjectInput(ProjectInputSubject::V1 {
      project_id,
      input_digest: FactoryDigest::content_sha256(&bytes),
    }))
  }

  /// Returns the owner shared by both subject forms.
  #[must_use]
  pub const fn project_id(&self) -> ProjectId {
    match self {
      Self::Repository(subject) => subject.project_id(),
      Self::ProjectInput(ProjectInputSubject::V1 { project_id, .. }) => *project_id,
    }
  }

  /// Requires an actual repository subject at repository-only capability boundaries.
  pub fn repository(&self) -> Result<&ExactSubject, crate::FactoryError> {
    match self {
      Self::Repository(subject) => Ok(subject),
      Self::ProjectInput(_) => Err(crate::FactoryError::InconsistentSubject),
    }
  }

  /// Stable subject identity; legacy repository digests remain unchanged.
  #[must_use]
  pub fn digest(&self) -> FactoryDigest {
    match self {
      Self::Repository(subject) => subject.digest(),
      Self::ProjectInput(ProjectInputSubject::V1 {
        project_id,
        input_digest,
      }) => FactoryDigest::sha256(
        "octacity.factory.project-input-subject.v1",
        &[project_id.as_uuid().as_bytes(), &input_digest.as_bytes()],
      ),
    }
  }
}

impl From<ExactSubject> for WorkSubject {
  fn from(value: ExactSubject) -> Self {
    Self::Repository(value)
  }
}

impl PartialEq<ExactSubject> for WorkSubject {
  fn eq(&self, other: &ExactSubject) -> bool {
    matches!(self, Self::Repository(subject) if subject == other)
  }
}
impl PartialEq<WorkSubject> for ExactSubject {
  fn eq(&self, other: &WorkSubject) -> bool {
    other == self
  }
}
