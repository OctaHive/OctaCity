//! Immutable source lineage for text projected into an Issue snapshot.
use crate::{IssueCommentMetadata, IssueDocumentResource, IssueMaterialSelection, ProjectDocumentResource};
use serde::{Deserialize, Serialize};

/// An approved exact source and its original safe attribution, without a second body copy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSnapshotMaterial {
  /// Owner/content reference and the actor/time that approved this reference on the Issue.
  pub selection: IssueMaterialSelection,
  /// Original source attribution; selecting text never turns it into evidence or approval.
  pub source: IssueSnapshotMaterialSource,
}
/// Source-owned immutable metadata, independently read with its original body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueSnapshotMaterialSource {
  /// Same-Issue discussion, including author/time, parent and original body identity.
  Comment {
    /// Original source metadata rather than the selection actor.
    comment: IssueCommentMetadata,
  },
  /// Exact Issue-owned publication, preserving its parent and original author.
  IssueDocument {
    /// Source metadata, never a Project catalog promotion.
    document: IssueDocumentResource,
  },
  /// Exact separately linked Project publication, preserving its original owner.
  ProjectDocument {
    /// Existing Project publication metadata; selection attribution stays separate.
    document: ProjectDocumentResource,
  },
}
