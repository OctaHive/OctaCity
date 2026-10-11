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
