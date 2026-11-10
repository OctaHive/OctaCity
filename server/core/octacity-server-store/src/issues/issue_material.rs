//! Explicit collaboration references; selection never copies bodies or grants execution authority.
use crate::*;
use octacity_server_domain::{IssueCommentId, IssueId, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Shared ceiling across comments and both document owner kinds.
pub const MAX_ISSUE_MATERIALS: usize = 32;

/// One exact source with its original owner and immutable content pin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueMaterialReference {
  /// Append-only comment belonging to this Issue.
  Comment {
    /// Original Project.
    project_id: ProjectId,
    /// Original Issue.
    issue_id: IssueId,
    /// Exact immutable comment.
    comment_id: IssueCommentId,
    /// Digest of the original body.
    body_digest: FactoryDigest,
  },
  /// Exact publication owned by this Issue; a newer publication never retargets it.
  IssueDocument {
    /// Original Project.
    project_id: ProjectId,
    /// Original Issue owner.
    issue_id: IssueId,
    /// Exact publication identity, byte digest and size.
    artifact: octacity_server_factory::FactoryArtifactReference,
  },
  /// Explicit link to an existing shared Project publication; ownership stays with Project.
  ProjectDocument {
    /// Original Project owner.
    project_id: ProjectId,
    /// Exact publication identity, byte digest and size.
    artifact: octacity_server_factory::FactoryArtifactReference,
  },
}
/// Safe selection attribution, independent of the source author's identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueMaterialSelection {
  /// Exact source; no embedded editable body.
  pub reference: IssueMaterialReference,
  /// Server-accepted actor who first selected this exact reference.
  pub selected_by: AuditActor,
  /// Original selection time, retained for unchanged entries.
  pub selected_at: Timestamp,
}
/// Optional selections on an immutable Issue revision; legacy revisions are empty.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct IssueMaterials(Vec<IssueMaterialSelection>);
impl<'de> Deserialize<'de> for IssueMaterials {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    Self::try_from(Vec::<IssueMaterialSelection>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
  }
}
impl TryFrom<Vec<IssueMaterialSelection>> for IssueMaterials {
  type Error = StoreInputError;
  fn try_from(entries: Vec<IssueMaterialSelection>) -> Result<Self, Self::Error> {
    validate_references(entries.iter().map(|entry| &entry.reference))?;
    for entry in &entries {
      crate::management_mutation::validate_management_actor(&entry.selected_by)?;
    }
    Ok(Self(entries))
  }
}
pub(crate) fn validate_references<'a>(
  references: impl Iterator<Item = &'a IssueMaterialReference>,
) -> Result<(), StoreInputError> {
  let mut identities = BTreeSet::new();
  for reference in references {
    // Document identity is shared across owner kinds: changing a claimed owner
    // or digest cannot disguise a duplicate publication as a distinct material.
    let identity = match reference {
      IssueMaterialReference::Comment { comment_id, .. } => (0, comment_id.as_uuid()),
      IssueMaterialReference::IssueDocument { artifact, .. }
      | IssueMaterialReference::ProjectDocument { artifact, .. } => (1, artifact.artifact_id().as_uuid()),
    };
    if !identities.insert(identity) || identities.len() > MAX_ISSUE_MATERIALS {
      return Err(StoreInputError::InvalidIssue);
    }
  }
  Ok(())
}
impl IssueMaterials {
  /// Safe source metadata in the reviewed order.
  pub fn entries(&self) -> &[IssueMaterialSelection] {
    &self.0
  }
  /// Ordinary tasks need no material selection.
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}
/// Complete reviewed replacement, guarded by the exact current Issue version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditIssueMaterialsIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Issue being edited.
  pub id: IssueId,
  /// Reviewed immutable revision.
  pub expected_version: IssueVersion,
  /// Exact desired references; absence removes a selection only.
  pub references: Vec<IssueMaterialReference>,
}
impl EditIssueMaterialsIntent {
  /// Shared intrinsic bounds/identity/owner checks; source access and digests
  /// still require the independently authorized source readers.
  pub fn validate(&self) -> Result<(), StoreError> {
    let invalid = || crate::issues::issue::invalid_issue(StoreOperation::EditIssueMaterials);
    validate_references(self.references.iter()).map_err(|_| invalid())?;
    if self.references.iter().any(|reference| match reference {
      IssueMaterialReference::Comment {
        project_id, issue_id, ..
      }
      | IssueMaterialReference::IssueDocument {
        project_id, issue_id, ..
      } => *project_id != self.project_id || *issue_id != self.id,
      IssueMaterialReference::ProjectDocument { project_id, .. } => *project_id != self.project_id,
    }) {
      return Err(invalid());
    }
    Ok(())
  }
}
/// Trusted mutation envelope; actor attribution comes from management context.
#[derive(Clone, Debug)]
pub struct EditIssueMaterials {
  /// Reviewed selection.
  pub intent: EditIssueMaterialsIntent,
  /// Scope-partitioned original receipt identity.
  pub idempotency_key: IdempotencyKey,
  /// Server time, never supplied by a selection request.
  pub edited_at: Timestamp,
}
/// Original review and outcome, retained independently of later edits and source availability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueMaterialEditReceipt {
  /// Original frozen intent.
  pub intent: EditIssueMaterialsIntent,
  /// Original immutable revision.
  pub outcome: IssueMutationOutcome,
}
/// Selection persistence shares Issue revisions and their atomic audit/replay transaction.
#[async_trait::async_trait]
pub trait IssueMaterialStore: IssueStore {
  /// Reads a receipt in the current accepted security scope, without resolving source bodies.
  async fn issue_material_edit(
    &self,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueMaterialEditReceipt>, StoreError>;
  /// Publishes one source-validated selection atomically with its revision and original receipt.
  async fn edit_issue_materials(
    &self,
    request: ManagementMutation<EditIssueMaterials>,
  ) -> Result<IssueMutationOutcome, StoreError>;
}
impl IssueRevision {
  /// Changes collaboration selection without invalidating requirements or moving business status.
  pub fn with_materials(&self, request: &EditIssueMaterials, actor: &AuditActor) -> Result<Self, StoreError> {
    let intent = &request.intent;
    intent.validate()?;
    if self.project_id != intent.project_id || self.id != intent.id || self.version != intent.expected_version {
      return Err(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Issue,
      });
    }
    let invalid = || crate::issues::issue::invalid_issue(StoreOperation::EditIssueMaterials);
    if request.edited_at < self.updated_at {
      return Err(invalid());
    }
    let materials = intent
      .references
      .iter()
      .map(|reference| {
        self
          .materials
          .0
          .iter()
          .find(|entry| &entry.reference == reference)
          .cloned()
          .unwrap_or_else(|| IssueMaterialSelection {
            reference: reference.clone(),
            selected_by: actor.clone(),
            selected_at: request.edited_at,
          })
      })
      .collect();
    Ok(Self {
      materials: IssueMaterials(materials),
      version: self.version.next().map_err(|_| invalid())?,
      updated_at: request.edited_at,
      ..self.clone()
    })
  }
}
