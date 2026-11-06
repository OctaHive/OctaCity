//! Read-only Issue cards. The transport never serializes accepted payloads or hidden references.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementQueryHandler, GetIssueOutcomesQuery, IssueOutcomeCard,
  IssueOutcomeClassification, IssueOutcomeContent, IssueOutcomeLink, IssueOutcomeLinkTarget,
};

impl ManagementApplication {
  /// Installs the authorized read of current pipeline outcomes for an Issue.
  pub fn with_issue_outcomes(
    mut self,
    handler: Arc<dyn AuthorizedManagementQueryHandler<GetIssueOutcomesQuery, Error = ApplicationError>>,
  ) -> Self {
    self.issue_outcomes = Some(handler);
    self
  }
}

pub(super) async fn get(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let query = GetIssueOutcomesQuery {
    project_id: project.parse().map_err(|_| invalid())?,
    issue_id: issue.parse().map_err(|_| invalid())?,
  };
  let cards = app
    .issue_outcomes
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &id))?;
  let items = cards.into_iter().map(Card::from).collect();
  Ok(([("cache-control", "no-store")], Json(Outcomes { items })))
}

#[derive(Serialize)]
struct Outcomes {
  items: Vec<Card>,
}

#[derive(Serialize)]
struct Card {
  slot: String,
  conclusion: String,
  run_id: String,
  presentation: Presentation,
  links: Vec<Link>,
}
impl From<IssueOutcomeCard> for Card {
  fn from(card: IssueOutcomeCard) -> Self {
    Self {
      slot: card.summary.slot.to_string(),
      conclusion: card.summary.conclusion,
      run_id: card.summary.run_id.to_string(),
      presentation: card.summary.content.into(),
      links: card.links.into_iter().map(Link::from).collect(),
    }
  }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Presentation {
  Triage {
    decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    duplicate_coverage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recommended_priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recommended_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<String>,
    classification: IssueOutcomeClassification,
  },
  Specifications,
  Publication,
  Review {
    verdict: String,
  },
  Custom {
    title: String,
  },
}
impl From<IssueOutcomeContent> for Presentation {
  fn from(content: IssueOutcomeContent) -> Self {
    match content {
      IssueOutcomeContent::Triage {
        decision,
        duplicate_coverage,
        priority,
        size,
        component,
        classification,
      } => Self::Triage {
        decision: decision.to_string(),
        duplicate_coverage: duplicate_coverage.map(|key| key.to_string()),
        recommended_priority: priority,
        recommended_size: size.map(|key| key.to_string()),
        component: component.map(|key| key.to_string()),
        classification,
      },
      IssueOutcomeContent::Specifications => Self::Specifications,
      IssueOutcomeContent::Publication => Self::Publication,
      IssueOutcomeContent::Review { verdict } => Self::Review {
        verdict: verdict.to_string(),
      },
      IssueOutcomeContent::Custom { title } => Self::Custom {
        title: title.to_string(),
      },
    }
  }
}

#[derive(Serialize)]
struct Link {
  #[serde(skip_serializing_if = "Option::is_none")]
  title: Option<String>,
  #[serde(flatten)]
  target: LinkTarget,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LinkTarget {
  Document { artifact_id: String },
  Artifact { artifact_id: String },
  WorkArtifact { work_id: String, artifact_id: String },
  Review { review_id: String },
  PullRequest { url: String },
  Unavailable,
}
impl From<IssueOutcomeLink> for Link {
  fn from(link: IssueOutcomeLink) -> Self {
    let target = match link.target {
      IssueOutcomeLinkTarget::Document(artifact_id) => LinkTarget::Document {
        artifact_id: artifact_id.to_string(),
      },
      IssueOutcomeLinkTarget::Artifact(artifact_id) => LinkTarget::Artifact {
        artifact_id: artifact_id.to_string(),
      },
      IssueOutcomeLinkTarget::WorkArtifact { work_id, artifact_id } => LinkTarget::WorkArtifact {
        work_id: work_id.to_string(),
        artifact_id: artifact_id.to_string(),
      },
      IssueOutcomeLinkTarget::Review(review_id) => LinkTarget::Review {
        review_id: review_id.to_string(),
      },
      IssueOutcomeLinkTarget::PullRequest(url) => LinkTarget::PullRequest { url },
      IssueOutcomeLinkTarget::Unavailable => LinkTarget::Unavailable,
    };
    Self {
      title: link.title,
      target,
    }
  }
}
