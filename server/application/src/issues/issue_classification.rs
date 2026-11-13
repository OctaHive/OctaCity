//! Authorized derived classification reads; this boundary publishes no revision or Work.
use crate::*;
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::FactoryTaskSubject;
pub use octacity_server_store::{
  IssueClassificationReview, MAX_ISSUE_CLASSIFICATION_FIELDS, MAX_ISSUE_CLASSIFICATION_REFERENCES,
};

#[path = "issue_classification_evidence.rs"]
mod evidence;
pub use evidence::*;

impl<S> IssueHandlers<S> {
  /// Installs independent authorized evidence reads; omission leaves selected claims unverified.
  pub fn with_classification_evidence_reader(
    mut self,
    reader: std::sync::Arc<dyn IssueClassificationEvidenceReader>,
  ) -> Self {
    self.classification_evidence = Some(reader);
    self
  }
}

/// Reviews an exact visible Issue without requiring a producer execution.
#[derive(Clone, Debug)]
pub struct ReviewIssueClassificationQuery {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible Issue.
  pub id: IssueId,
  /// Exact revision, or current when absent.
  pub version: Option<IssueVersion>,
  /// Independently resolved code subject, when the caller is reviewing code-dependent data.
  pub subject: Option<FactoryTaskSubject>,
  /// Trusted server observation time, not a management body field.
  pub checked_at: Timestamp,
}
impl Query for ReviewIssueClassificationQuery {
  type Outcome = IssueClassificationReview;
}
impl ManagementAuthorizationTarget for ReviewIssueClassificationQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetIssueQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Issue, self.id)
  }
}
#[async_trait::async_trait]
impl<S: IssueStore + octacity_server_store::IssuePropertyStore> ManagementQueryUseCase<ReviewIssueClassificationQuery>
  for IssueHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ReviewIssueClassificationQuery,
  ) -> Result<IssueClassificationReview, ApplicationError> {
    super::visible_issue(grant, query.id)?;
    let issue = self.store.issue(query.project_id, query.id, query.version).await?;
    if query.checked_at < issue.updated_at
      || query
        .subject
        .as_ref()
        .is_some_and(|s| s.exact().is_err() || s.exact().is_ok_and(|s| s.project_id() != query.project_id))
    {
      return Err(ApplicationError::invalid());
    }
    let catalog = self.store.issue_properties(query.project_id, None).await?;
    let current_version = catalog.version;
    let mut catalogs = std::collections::BTreeMap::from([(current_version, catalog)]);
    for assessment in &issue.classification.fields {
      if let octacity_server_store::IssueClassificationFieldReference::Property { definition_version, .. } =
        &assessment.field
        && let std::collections::btree_map::Entry::Vacant(entry) = catalogs.entry(*definition_version)
      {
        entry.insert(
          self
            .store
            .issue_properties(query.project_id, Some(*definition_version))
            .await?,
        );
      }
    }
    let mut review = issue.review_classification(
      &catalogs[&current_version],
      &catalogs,
      query.subject.as_ref(),
      query.checked_at,
    );
    if let Some(reader) = &self.classification_evidence {
      let mut proofs = std::collections::BTreeMap::new();
      for field in &mut review.fields {
        let Some(assessment) = issue
          .classification
          .fields
          .iter()
          .find(|assessment| assessment.field.identity() == field.field.identity())
        else {
          continue;
        };
        let claim = issue
          .classification_reproduction_claim(&assessment.field, &catalogs)
          .unwrap_or("unknown");
        for record in &assessment.evidence {
          let Some(reference) = issue
            .classification
            .evidence
            .iter()
            .find(|reference| reference.record == *record)
          else {
            continue;
          };
          if let std::collections::btree_map::Entry::Vacant(entry) = proofs.entry(*record) {
            entry.insert(reader.read(context, reference, query.checked_at).await.unwrap_or(None));
          }
          if let Some(Some(proof)) = proofs.get(record) {
            let diagnostics = proof.review_claim(reference, claim);
            if field.requires_verified_evidence {
              field.problems.retain(|problem| !matches!(problem, octacity_server_store::IssueClassificationProblem::EvidenceUnavailable {record:missing} if missing == record));
              field.problems.extend(diagnostics.clone());
            }
            if let Some(evidence) = field.evidence.iter_mut().find(|e| e.record == *record) {
              evidence.problems.retain(|problem| {
                !matches!(
                  problem,
                  octacity_server_store::IssueClassificationProblem::EvidenceUnavailable { .. }
                )
              });
              evidence.problems.extend(diagnostics);
            }
          }
        }
      }
    }
    Ok(review)
  }
}
