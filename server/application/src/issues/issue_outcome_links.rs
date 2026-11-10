//! Resolves only the resource selections retained by the accepted outcome.
use crate::*;
use octacity_artifact_store::MAX_ARTIFACT_READ_BYTES;
use octacity_server_domain::{ArtifactId, IssueId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest, WorkArtifacts, WorkEnvelopeId};
use octacity_server_store::{IssueCodeReviewStore, IssueOutcomeReference, ProjectDocumentQueryStore};
use std::sync::Arc;

/// A visible resource with an owner-supplied title and local navigation target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueOutcomeLink {
  /// Retained title when one is available.
  pub title: Option<String>,
  /// Local destination, or a visible resource with no retained content.
  pub target: IssueOutcomeLinkTarget,
}

/// Local targets are derived from the owners' immutable identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssueOutcomeLinkTarget {
  /// Named Project document.
  Document(ArtifactId),
  /// Retained Artifact.
  Artifact(ArtifactId),
  /// Private input accessible through its original Issue Work inventory.
  WorkArtifact {
    /// Exact original Work.
    work_id: WorkEnvelopeId,
    /// Exact input artifact.
    artifact_id: ArtifactId,
  },
  /// Exact review association on the selected Issue.
  Review(FactoryDigest),
  /// Verified pull request from the retained publication receipt.
  PullRequest(String),
  /// Visible metadata survives after content expiry.
  Unavailable,
}

/// Composes the existing resource owners for metadata-only Issue outcome links.
pub struct IssueOutcomeLinkReaders<S> {
  documents: Arc<dyn ProjectDocumentQueryStore>,
  artifacts: Arc<dyn IssueArtifactReader>,
  reviews: Arc<IssueCodeReviewHandlers<S>>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> IssueOutcomeLinkReaders<S> {
  /// Uses existing readers and the trusted retention clock.
  pub fn new(
    documents: Arc<dyn ProjectDocumentQueryStore>,
    artifacts: Arc<dyn IssueArtifactReader>,
    reviews: Arc<IssueCodeReviewHandlers<S>>,
    clock: Arc<dyn FactoryExecutionClock>,
  ) -> Self {
    Self {
      documents,
      artifacts,
      reviews,
      clock,
    }
  }
}

/// Exact authorized Issue Work scope for resolving retained references.
#[derive(Clone, Copy)]
pub(crate) struct IssueOutcomeLinkScope<'a> {
  pub(crate) project: ProjectId,
  pub(crate) issue: IssueId,
  pub(crate) work_id: WorkEnvelopeId,
  pub(crate) inputs: &'a WorkArtifacts,
}

impl<S: IssueCodeReviewStore> IssueOutcomeLinkReaders<S> {
  pub(crate) async fn resolve(
    &self,
    context: &ManagementRequestContext,
    policy: &dyn ManagementAuthorizationPolicy,
    scope: IssueOutcomeLinkScope<'_>,
    references: &[IssueOutcomeReference],
  ) -> Result<Vec<IssueOutcomeLink>, ApplicationError> {
    let mut links = Vec::new();
    let mut resolved_artifacts = Vec::new();
    for reference in references {
      match reference {
        IssueOutcomeReference::Document(artifact) | IssueOutcomeReference::Artifact(artifact) => {
          let id = artifact.artifact_id();
          if resolved_artifacts.contains(&artifact) || !artifact_visible(policy, context, id).await {
            continue;
          }
          let document = self.document(scope.project, artifact).await?;
          let link = match (document, reference) {
            (None, IssueOutcomeReference::Artifact(_)) => self.artifact(context, policy, scope, artifact).await?,
            (document, _) => document,
          };
          if let Some(link) = link
            && artifact_visible(policy, context, id).await
          {
            resolved_artifacts.push(artifact);
            links.push(link);
          }
        }
        IssueOutcomeReference::Review(id) => {
          if let Some(link) = self
            .reviews
            .outcome_link(context, scope.project, scope.issue, *id)
            .await?
          {
            links.push(link);
          }
        }
      }
    }
    Ok(links)
  }

  async fn document(
    &self,
    project: ProjectId,
    reference: &FactoryArtifactReference,
  ) -> Result<Option<IssueOutcomeLink>, ApplicationError> {
    let artifact = reference.artifact_id();
    let Some(document) = self.documents.project_document_metadata(project, artifact).await? else {
      return Ok(None);
    };
    if document.project_id != project || document.artifact != *reference {
      return Ok(None);
    }
    let now = self.clock.now()?;
    if document.published_at > now {
      return Ok(None);
    }
    Ok(Some(named_link(
      document.name.to_string(),
      IssueOutcomeLinkTarget::Document(artifact),
      now >= document.expires_at,
    )))
  }

  async fn artifact(
    &self,
    context: &ManagementRequestContext,
    policy: &dyn ManagementAuthorizationPolicy,
    scope: IssueOutcomeLinkScope<'_>,
    reference: &FactoryArtifactReference,
  ) -> Result<Option<IssueOutcomeLink>, ApplicationError> {
    let artifact = reference.artifact_id();
    let now = self.clock.now()?;
    let Ok(metadata) = self
      .artifacts
      .metadata(ReadFactoryArtifact {
        project_id: scope.project,
        artifact: reference.clone(),
        maximum_bytes: MAX_ARTIFACT_READ_BYTES,
        observed_at: now,
        required_until: Timestamp::from_unix_millis(now.unix_millis().saturating_add(1))
          .map_err(|_| ApplicationError::invalid())?,
      })
      .await
    else {
      return Ok(None);
    };
    if metadata.availability == IssueArtifactAvailability::Unavailable {
      return Ok(None);
    }
    let target = match metadata.origin {
      IssueArtifactOrigin::BuildOutput => {
        let Some((build, job)) = metadata.build_owner else {
          return Ok(None);
        };
        if !crate::issues::issue_execution::build_job_visible(policy, context, build, job).await {
          return Ok(None);
        }
        IssueOutcomeLinkTarget::Artifact(artifact)
      }
      IssueArtifactOrigin::ProjectInput | IssueArtifactOrigin::SourceInput => {
        if !scope.inputs.iter().any(|input| input == reference) {
          return Ok(None);
        }
        IssueOutcomeLinkTarget::WorkArtifact {
          work_id: scope.work_id,
          artifact_id: artifact,
        }
      }
    };
    let now = self.clock.now()?;
    if !metadata.published_at.is_some_and(|published_at| published_at <= now) {
      return Ok(None);
    }
    let expired =
      metadata.availability == IssueArtifactAvailability::Expired || metadata.expires_at.is_some_and(|end| now >= end);
    Ok(Some(named_link(metadata.name, target, expired)))
  }
}

/// The publication link is visible only with access to its exact repository.
pub(crate) async fn confirmed_publication(
  context: &ManagementRequestContext,
  policy: &dyn ManagementAuthorizationPolicy,
  project: ProjectId,
  facts: Option<&octacity_server_factory::FlowDiscoveryFacts>,
) -> Option<IssueOutcomeLink> {
  use crate::factory::{
    artifacts::factory_candidate_publication::FactoryPublicationFacts,
    authoring::factory_configuration_choices::require_visible_reference,
  };
  let (subject, url) = facts?.confirmed_candidate_review()?;
  let parsed = url::Url::parse(url).ok()?;
  if subject.project_id() != project || !FactoryPublicationFacts::is_safe_review_url(&parsed) {
    return None;
  }
  require_visible_reference(
    policy,
    context,
    ManagementResourceKind::Repository,
    subject.repository_id(),
  )
  .await
  .ok()?;
  Some(IssueOutcomeLink {
    title: None,
    target: IssueOutcomeLinkTarget::PullRequest(parsed.into()),
  })
}

async fn artifact_visible(
  policy: &dyn ManagementAuthorizationPolicy,
  context: &ManagementRequestContext,
  artifact: ArtifactId,
) -> bool {
  use crate::factory::authoring::factory_configuration_choices::{
    require_authorized_reference, require_visible_reference,
  };
  require_visible_reference(policy, context, ManagementResourceKind::Artifact, artifact)
    .await
    .is_ok()
    && require_authorized_reference(
      policy,
      context,
      ManagementAction::Download,
      ManagementResourceKind::Artifact,
      artifact,
    )
    .await
    .is_ok()
}

fn named_link(title: String, target: IssueOutcomeLinkTarget, expired: bool) -> IssueOutcomeLink {
  IssueOutcomeLink {
    title: Some(title),
    target: if expired {
      IssueOutcomeLinkTarget::Unavailable
    } else {
      target
    },
  }
}
