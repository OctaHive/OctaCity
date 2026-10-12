//! Shared GitHub HTTP translation; provider representations stay in this plugin.
use crate::config::{Config, Credential, Repository};
use http_body_util::BodyExt as _;
use octacity_delivery_protocol::{DeliveryIntent, DeliveryReceipt, FailureClass, ReviewReference, ReviewState};
use reqwest::{
  Client, Method, StatusCode,
  header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde_json::Value;
use zeroize::Zeroizing;

const MAX_API_BODY_BYTES: usize = 1024 * 1024;
pub(crate) struct GithubApi {
  client: octocrab::Octocrab,
  base: url::Url,
  owner: String,
  repository: String,
}
pub(crate) struct Observation {
  pub(crate) receipt: DeliveryReceipt,
  pub(crate) metadata_matches: bool,
}
impl GithubApi {
  pub(crate) async fn repair_review(&self, intent: &DeliveryIntent, identity: &str) -> Result<(), FailureClass> {
    let body = serde_json::json!({"title":intent.review.title,"body":review_body(intent)});
    self
      .request(Method::PATCH, self.url(&["pulls", identity])?, Some(body))
      .await?
      .ok_or(FailureClass::ProtocolFault)?;
    Ok(())
  }
  pub(crate) async fn create_review(&self, intent: &DeliveryIntent) -> Result<(), FailureClass> {
    let body = serde_json::json!({"title":intent.review.title,
      "body":review_body(intent),
      "head":branch(&intent.target.candidate_reference)?,"base":branch(&intent.target.base_reference)?,"draft":intent.review.draft});
    self
      .request(Method::POST, self.url(&["pulls"])?, Some(body))
      .await?
      .ok_or(FailureClass::ProtocolFault)?;
    Ok(())
  }
  pub(crate) fn new(config: &Config, repo: &Repository, credential: &Credential) -> Result<Self, FailureClass> {
    let mut bearer = Zeroizing::new(String::from("Bearer "));
    bearer.push_str(&credential.token);
    let mut authorization = HeaderValue::from_str(&bearer).map_err(|_| FailureClass::Permanent)?;
    authorization.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, authorization);
    headers.insert("accept", HeaderValue::from_static("application/vnd.github+json"));
    headers.insert(
      "x-github-api-version",
      HeaderValue::from_str(&config.api_version).map_err(|_| FailureClass::Permanent)?,
    );
    let client = Client::builder()
      .default_headers(headers)
      .user_agent("octacity-github")
      .timeout(std::time::Duration::from_secs(30))
      .redirect(reqwest::redirect::Policy::none())
      .retry(reqwest::retry::never())
      .build()
      .map_err(|_| FailureClass::Permanent)?;
    // No SDK retry, redirect or tracing features: the Delivery contract owns
    // observe-before-retry and prevents token/URL exposure in diagnostics.
    let client = octocrab::Octocrab::builder()
      .base_uri(config.api_base.as_str())
      .map_err(|_| FailureClass::Permanent)?
      .add_header(
        "x-github-api-version".parse().map_err(|_| FailureClass::Permanent)?,
        config.api_version.clone(),
      )
      .build_with_reqwest(client)
      .map_err(|_| FailureClass::Permanent)?;
    Ok(Self {
      client,
      base: config.api_base.clone(),
      owner: repo.owner.clone(),
      repository: repo.name.clone(),
    })
  }
  pub(crate) async fn observe(&self, intent: &DeliveryIntent) -> Result<Option<DeliveryReceipt>, FailureClass> {
    self
      .inspect(intent)
      .await
      .map(|observation| observation.map(|observation| observation.receipt))
  }
  pub(crate) async fn inspect(&self, intent: &DeliveryIntent) -> Result<Option<Observation>, FailureClass> {
    let base = self
      .reference(&intent.target.base_reference)
      .await?
      .ok_or(FailureClass::Conflict)?;
    let Some(head) = self.reference(&intent.target.candidate_reference).await? else {
      return if base == intent.base_revision {
        Ok(None)
      } else {
        Err(FailureClass::Conflict)
      };
    };
    if head != intent.candidate_revision {
      return Err(FailureClass::Conflict);
    }
    let mut url = self.url(&["pulls"])?;
    url
      .query_pairs_mut()
      .append_pair("state", "all")
      .append_pair(
        "head",
        &format!("{}:{}", self.owner, branch(&intent.target.candidate_reference)?),
      )
      .append_pair("base", branch(&intent.target.base_reference)?)
      .append_pair("per_page", "100");
    let reviews = self
      .request(Method::GET, url, None)
      .await?
      .ok_or(FailureClass::ProtocolFault)?;
    let reviews = reviews.as_array().ok_or(FailureClass::ProtocolFault)?;
    if reviews.len() > 100 {
      return Err(FailureClass::ProtocolFault);
    }
    let mut selected = None;
    let mut metadata_matches = false;
    for review in reviews {
      let expected_repo = format!("{}/{}", self.owner, self.repository);
      if text(review, "/head/sha")? != intent.candidate_revision
        || text(review, "/head/ref")? != branch(&intent.target.candidate_reference)?
        || text(review, "/base/ref")? != branch(&intent.target.base_reference)?
        || !text(review, "/head/repo/full_name")?.eq_ignore_ascii_case(&expected_repo)
        || !text(review, "/base/repo/full_name")?.eq_ignore_ascii_case(&expected_repo)
        || !text(review, "/body")?.lines().any(|line| line == marker(intent))
      {
        return Err(FailureClass::Conflict);
      }
      if selected.is_some() {
        return Err(FailureClass::Conflict);
      }
      let number = review
        .get("number")
        .and_then(Value::as_u64)
        .filter(|number| *number > 0)
        .ok_or(FailureClass::ProtocolFault)?;
      let state = if review.get("merged_at").is_some_and(|at| !at.is_null()) {
        ReviewState::MergedExternally
      } else {
        match text(review, "/state")? {
          "open" => ReviewState::Open,
          "closed" => ReviewState::Closed,
          _ => return Err(FailureClass::ProtocolFault),
        }
      };
      let review_base = text(review, "/base/sha")?;
      if !object_id(review_base)
        || (state != ReviewState::MergedExternally
          && (base != intent.base_revision || review_base != intent.base_revision))
      {
        return Err(FailureClass::Conflict);
      }
      selected = Some(ReviewReference {
        identity: number.to_string(),
        state,
        head: head.clone(),
        base: review_base.to_owned(),
      });
      metadata_matches = review.get("title").and_then(Value::as_str) == Some(intent.review.title.as_str())
        && text(review, "/body")? == review_body(intent);
    }
    if base != intent.base_revision
      && selected
        .as_ref()
        .is_none_or(|review| review.state != ReviewState::MergedExternally)
    {
      return Err(FailureClass::Conflict);
    }
    let receipt = DeliveryReceipt {
      delivery_id: intent.delivery_id.clone(),
      repository_id: intent.repository.repository_id.clone(),
      base_revision: intent.base_revision.clone(),
      candidate_revision: intent.candidate_revision.clone(),
      changeset_digest: intent.changeset_digest.clone(),
      decision_digest: intent.decision_digest.clone(),
      configuration_digest: intent.configuration_digest.clone(),
      target: intent.target.clone(),
      branch_reference: intent.target.candidate_reference.clone(),
      remote_head: head,
      review: selected,
    };
    receipt.validate_for(intent).map_err(|_| FailureClass::ProtocolFault)?;
    Ok(Some(Observation {
      receipt,
      metadata_matches,
    }))
  }
  pub(crate) async fn reference(&self, reference: &str) -> Result<Option<String>, FailureClass> {
    branch(reference)?;
    let mut parts = vec!["git", "ref"];
    parts.extend(
      reference
        .strip_prefix("refs/")
        .ok_or(FailureClass::InvalidRequest)?
        .split('/'),
    );
    let Some(value) = self.request(Method::GET, self.url(&parts)?, None).await? else {
      return Ok(None);
    };
    if text(&value, "/ref")? != reference || text(&value, "/object/type")? != "commit" {
      return Err(FailureClass::ProtocolFault);
    }
    let sha = text(&value, "/object/sha")?;
    if !object_id(sha) {
      return Err(FailureClass::ProtocolFault);
    }
    Ok(Some(sha.to_owned()))
  }
  pub(crate) fn url(&self, suffix: &[&str]) -> Result<url::Url, FailureClass> {
    let mut url = self.base.clone();
    {
      let mut segments = url.path_segments_mut().map_err(|_| FailureClass::Permanent)?;
      segments
        .pop_if_empty()
        .push("repos")
        .push(&self.owner)
        .push(&self.repository);
      for part in suffix {
        segments.push(part);
      }
    }
    Ok(url)
  }
  pub(crate) async fn request(
    &self,
    method: Method,
    url: url::Url,
    body: Option<Value>,
  ) -> Result<Option<Value>, FailureClass> {
    // Raw SDK responses retain status/pagination and let us bound the body
    // before deserializing. The SDK's convenience JSON methods buffer freely.
    let response = match method {
      Method::GET => self.client._get(url.as_str()).await,
      Method::POST => self.client._post(url.as_str(), body.as_ref()).await,
      Method::PATCH => self.client._patch(url.as_str(), body.as_ref()).await,
      _ => return Err(FailureClass::InvalidRequest),
    }
    .map_err(|_| FailureClass::Transient)?;
    match response.status() {
      StatusCode::NOT_FOUND => return Ok(None),
      StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(FailureClass::Permanent),
      StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY => return Err(FailureClass::Conflict),
      StatusCode::TOO_MANY_REQUESTS => return Err(FailureClass::Transient),
      status if status.is_server_error() => return Err(FailureClass::Transient),
      status if !status.is_success() => return Err(FailureClass::ProtocolFault),
      _ => {}
    }
    // Never qualify a partial list while another page could contain a conflicting review.
    if response
      .headers()
      .get("link")
      .and_then(|header| header.to_str().ok())
      .is_some_and(|links| links.contains("rel=\"next\""))
      || response
        .headers()
        .get("content-length")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|bytes| bytes > MAX_API_BODY_BYTES as u64)
    {
      return Err(FailureClass::ProtocolFault);
    }
    let mut bytes = Vec::new();
    let mut body = response.into_body();
    while let Some(frame) = body.frame().await {
      let frame = frame.map_err(|_| FailureClass::Transient)?;
      let Ok(chunk) = frame.into_data() else { continue };
      if bytes.len().saturating_add(chunk.len()) > MAX_API_BODY_BYTES {
        return Err(FailureClass::ProtocolFault);
      }
      bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
      .map(Some)
      .map_err(|_| FailureClass::ProtocolFault)
  }
}
pub(crate) fn marker(intent: &DeliveryIntent) -> String {
  format!(
    "<!-- octacity-delivery:v1 {} {} {} -->",
    intent.delivery_id, intent.decision_digest, intent.changeset_digest
  )
}
fn review_body(intent: &DeliveryIntent) -> String {
  format!("{}\n\n{}", intent.review.summary, marker(intent))
}
fn text<'a>(value: &'a Value, path: &str) -> Result<&'a str, FailureClass> {
  value
    .pointer(path)
    .and_then(Value::as_str)
    .ok_or(FailureClass::ProtocolFault)
}
pub(crate) fn object_id(value: &str) -> bool {
  value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub(crate) fn branch(reference: &str) -> Result<&str, FailureClass> {
  let name = reference
    .strip_prefix("refs/heads/")
    .ok_or(FailureClass::InvalidRequest)?;
  if name.is_empty()
    || name.starts_with('-')
    || name.contains("..")
    || name.contains("@{")
    || name
      .split('/')
      .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with('.') || part.ends_with(".lock"))
    || name
      .bytes()
      .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || b"~^:?*[\\".contains(&b))
  {
    return Err(FailureClass::InvalidRequest);
  }
  Ok(name)
}
