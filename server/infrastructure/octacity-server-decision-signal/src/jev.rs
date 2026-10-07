use std::{collections::BTreeMap, fmt, future::Future, pin::Pin, sync::Arc, time::Duration};

use futures_util::StreamExt as _;
use octacity_server_factory::{
  BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalInputMedia, DecisionSignalProbability,
  DecisionSignalProvider, DecisionSignalProviderCapability, DecisionSignalProviderFailure,
  DecisionSignalProviderFuture, DecisionSignalProviderRequest, DecisionSignalProviderResult,
  DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain, DecisionSignalQuestionKind, DecisionSignalState,
  FactoryKey,
};
use reqwest::{
  Client, StatusCode, Url,
  header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_JEV_QUESTIONS: usize = 20;
const MAX_JEV_CHOICE_LABELS: usize = 24;
const MAX_JEV_INSTRUCTIONS_BYTES: usize = 1_000;
const MAX_JEV_CRITERION_BYTES: usize = 400;
// These values are part of the pinned JEV wire contract. Keeping them here
// makes protocol drift visible instead of scattering provider literals through
// transport and decoding code.
const JEV_DECISION_PATH: &str = "/api/v1/systemone/";
const JEV_REQUEST_STATUS_PREFIX: &str = "/api/v1/requests/";
const JEV_PROVIDER_IDENTITY: &str = "jev-ai.org";
const JEV_CHOICE_QUESTION_KIND: &str = "choice";
const JEV_TOKEN_WALLET: &str = "tokens";
const JEV_CREDIT_WALLET: &str = "credits";
const JEV_IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const MAX_JEV_QUESTION_ID_BYTES: usize = 64;
const MAX_JEV_REQUEST_ID_BYTES: usize = 128;

/// Server-side JEV API credential that zeroes its source allocation on drop.
pub struct JevApiKey(Zeroizing<String>);

impl JevApiKey {
  /// Accepts one bounded JEV server credential.
  pub fn new(value: String) -> Result<Self, JevAdapterError> {
    if !value.starts_with("sk-glm5-") || !(16..=512).contains(&value.len()) {
      return Err(JevAdapterError::InvalidConfiguration);
    }
    Ok(Self(Zeroizing::new(value)))
  }
}

/// Configuration or safe transport failure while constructing the JEV adapter.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum JevAdapterError {
  /// Endpoint, credential, timeout, or exact capability is invalid.
  #[error("JEV adapter configuration is invalid")]
  InvalidConfiguration,
  /// The HTTP client could not be constructed without exposing diagnostics.
  #[error("JEV HTTP client construction failed")]
  ClientConstruction,
}

/// Operator-owned conversion for JEV credit-wallet charges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JevBillingPolicy {
  credit_cost_micro_units: u64,
}

impl JevBillingPolicy {
  /// Defines the configured currency cost of one charged JEV credit.
  pub const fn new(credit_cost_micro_units: u64) -> Result<Self, JevAdapterError> {
    if credit_cost_micro_units == 0 {
      return Err(JevAdapterError::InvalidConfiguration);
    }
    Ok(Self {
      credit_cost_micro_units,
    })
  }
}

/// JEV HTTP adapter for one operator-pinned exact model capability.
pub struct JevDecisionSignalProvider {
  capability: DecisionSignalProviderCapability,
  endpoint: Url,
  billing: JevBillingPolicy,
  transport: Arc<dyn JevTransport>,
}

impl JevDecisionSignalProvider {
  /// Creates a production adapter for the canonical HTTPS decision endpoint.
  pub fn new(
    endpoint: &str,
    api_key: JevApiKey,
    capability: DecisionSignalProviderCapability,
    billing: JevBillingPolicy,
    timeout: Duration,
  ) -> Result<Self, JevAdapterError> {
    Self::build(endpoint, api_key, capability, billing, timeout, true)
  }

  fn build(
    endpoint: &str,
    api_key: JevApiKey,
    capability: DecisionSignalProviderCapability,
    billing: JevBillingPolicy,
    timeout: Duration,
    require_https: bool,
  ) -> Result<Self, JevAdapterError> {
    let endpoint = Url::parse(endpoint).map_err(|_| JevAdapterError::InvalidConfiguration)?;
    if (require_https && endpoint.scheme() != "https")
      || endpoint.cannot_be_a_base()
      || endpoint.username() != ""
      || endpoint.password().is_some()
      || endpoint.query().is_some()
      || endpoint.fragment().is_some()
      || endpoint.path() != JEV_DECISION_PATH
      || timeout.is_zero()
      || capability.limits().max_questions() as usize > MAX_JEV_QUESTIONS
      || capability.limits().max_choices() as usize > MAX_JEV_CHOICE_LABELS
      || capability
        .supported_question_kinds()
        .iter()
        .any(|kind| *kind != DecisionSignalQuestionKind::FiniteChoice)
    {
      return Err(JevAdapterError::InvalidConfiguration);
    }

    let mut bearer = Zeroizing::new(String::with_capacity("Bearer ".len() + api_key.0.len()));
    bearer.push_str("Bearer ");
    bearer.push_str(api_key.0.as_str());
    let mut authorization =
      HeaderValue::from_str(bearer.as_str()).map_err(|_| JevAdapterError::InvalidConfiguration)?;
    authorization.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, authorization);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let client = Client::builder()
      .default_headers(headers)
      .timeout(timeout)
      .redirect(reqwest::redirect::Policy::none())
      .build()
      .map_err(|_| JevAdapterError::ClientConstruction)?;
    Ok(Self {
      capability,
      endpoint,
      billing,
      transport: Arc::new(ReqwestTransport { client }),
    })
  }

  #[cfg(test)]
  pub(crate) fn for_test(
    endpoint: &str,
    capability: DecisionSignalProviderCapability,
    transport: Arc<dyn JevTransport>,
  ) -> Result<Self, JevAdapterError> {
    let endpoint = Url::parse(endpoint).map_err(|_| JevAdapterError::InvalidConfiguration)?;
    Ok(Self {
      capability,
      endpoint,
      billing: JevBillingPolicy::new(1).expect("test billing policy is valid"),
      transport,
    })
  }

  #[expect(
    clippy::result_large_err,
    reason = "the provider-neutral contract returns the immutable typed failure by value"
  )]
  async fn dispatch(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<DecisionSignalProviderResult, DecisionSignalProviderFailure> {
    if !self.matches(request) {
      return Err(failure(request, DecisionSignalState::Invalid));
    }
    let body = match request_body(request) {
      Some(body) => body,
      None => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    let body = match serde_json::to_vec(&body) {
      Ok(body) => body,
      Err(_) => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    let idempotency_key = request.id().as_uuid().to_string();
    let response = match self.transport.post(&self.endpoint, &idempotency_key, body).await {
      Ok(response) => response,
      Err(JevTransportError::TimedOut) => return Err(failure(request, DecisionSignalState::TimedOut)),
      Err(JevTransportError::Unavailable) => return Err(failure(request, DecisionSignalState::Unavailable)),
    };
    if response.status == StatusCode::CONFLICT {
      return self.observe_conflict(request, response).await;
    }
    self.decode_response(request, response)
  }

  #[expect(
    clippy::result_large_err,
    reason = "the provider-neutral contract returns the immutable typed failure by value"
  )]
  async fn observe_conflict(
    &self,
    request: &DecisionSignalProviderRequest,
    response: JevHttpResponse,
  ) -> Result<DecisionSignalProviderResult, DecisionSignalProviderFailure> {
    let conflict = match serde_json::from_slice::<JevConflict>(&response.body) {
      Ok(conflict)
        if matches!(
          conflict.error.code.as_str(),
          "request_in_progress" | "request_already_completed"
        ) && valid_request_id(&conflict.request_id) =>
      {
        conflict
      }
      _ => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    let status = match Url::parse(&conflict.status_url) {
      Ok(status)
        if same_origin(&self.endpoint, &status)
          && status.path() == format!("{JEV_REQUEST_STATUS_PREFIX}{}/", conflict.request_id)
          && status.query().is_none()
          && status.fragment().is_none() =>
      {
        status
      }
      _ => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    match self.transport.get(&status).await {
      Ok(response) => self.decode_response(request, response),
      Err(JevTransportError::TimedOut) => Err(failure(request, DecisionSignalState::TimedOut)),
      Err(JevTransportError::Unavailable) => Err(failure(request, DecisionSignalState::Unavailable)),
    }
  }

  #[expect(
    clippy::result_large_err,
    reason = "the provider-neutral contract returns the immutable typed failure by value"
  )]
  fn decode_response(
    &self,
    request: &DecisionSignalProviderRequest,
    response: JevHttpResponse,
  ) -> Result<DecisionSignalProviderResult, DecisionSignalProviderFailure> {
    if !response.status.is_success() {
      return Err(failure(request, classify_status(response.status)));
    }
    let response_bytes = response.body.len() as u64;
    let wire = match serde_json::from_slice::<JevResponse>(&response.body) {
      Ok(wire) => wire,
      Err(_) => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    if !valid_request_id(&wire.id)
      || wire.model != request.model().identity().as_str()
      || wire.model_version != request.model().version().as_str()
      || wire.provider != JEV_PROVIDER_IDENTITY
      || wire.answers.len() != request.input().questions().len()
    {
      return Err(failure(request, DecisionSignalState::Invalid));
    }
    let answers = match decode_answers(request, &wire.answers) {
      Some(answers) => answers,
      None => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    let cost_micro_units = match wire.usage.wallet.as_str() {
      JEV_TOKEN_WALLET if wire.usage.charged_tokens == wire.usage.input_tokens && wire.usage.charged_credits == 0 => 0,
      JEV_CREDIT_WALLET if wire.usage.charged_tokens == 0 && wire.usage.charged_credits > 0 => {
        let Some(cost) = wire
          .usage
          .charged_credits
          .checked_mul(self.billing.credit_cost_micro_units)
        else {
          return Err(failure(request, DecisionSignalState::Invalid));
        };
        cost
      }
      _ => return Err(failure(request, DecisionSignalState::Invalid)),
    };
    let Some(tokens) = wire.usage.input_tokens.checked_add(wire.usage.output_tokens) else {
      return Err(failure(request, DecisionSignalState::Invalid));
    };
    let usage = BudgetUsage {
      attempts: 1,
      elapsed_millis: wire.latency_ms,
      tokens,
      cost_micro_units,
      output_bytes: response_bytes,
    };
    if usage.validate(request.request().budget()).is_err() {
      return Err(failure(request, DecisionSignalState::Invalid));
    }
    DecisionSignalProviderResult::new(
      request.id(),
      request.provider().clone(),
      request.adapter().clone(),
      request.model().clone(),
      request.probability_semantics().clone(),
      answers,
      usage,
    )
    .map_err(|_| failure(request, DecisionSignalState::Invalid))
  }

  fn matches(&self, request: &DecisionSignalProviderRequest) -> bool {
    let capability = &self.capability;
    request.provider() == capability.provider()
      && request.adapter() == capability.adapter()
      && request.model() == capability.model()
      && request.probability_semantics() == capability.probability_semantics()
      && capability.supports(request.request().purpose())
  }
}

impl fmt::Debug for JevDecisionSignalProvider {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("JevDecisionSignalProvider")
      .field("capability", &self.capability)
      .field("endpoint", &self.endpoint)
      .finish_non_exhaustive()
  }
}

impl DecisionSignalProvider for JevDecisionSignalProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }

  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    Box::pin(self.dispatch(request))
  }
}

#[derive(Serialize)]
struct JevRequest {
  model: String,
  state: serde_json::Value,
  questions: BTreeMap<String, JevQuestion>,
}

#[derive(Serialize)]
struct JevQuestion {
  #[serde(rename = "type")]
  kind: &'static str,
  instructions: String,
  criteria: serde_json::Value,
}

fn request_body(request: &DecisionSignalProviderRequest) -> Option<JevRequest> {
  if request.input().questions().len() > MAX_JEV_QUESTIONS {
    return None;
  }
  let state = match request.input().media() {
    DecisionSignalInputMedia::CanonicalJson => serde_json::from_str(request.state_document().as_str()).ok()?,
    DecisionSignalInputMedia::Utf8Text => serde_json::Value::String(request.state_document().as_str().to_owned()),
  };
  let questions = request
    .input()
    .questions()
    .iter()
    .map(|question| {
      let identity = question.reference().identity().as_str().to_owned();
      if identity.len() > MAX_JEV_QUESTION_ID_BYTES
        || question.instructions().as_str().len() > MAX_JEV_INSTRUCTIONS_BYTES
        || !identity
          .bytes()
          .enumerate()
          .all(|(index, byte)| byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'_' | b'-')))
      {
        return None;
      }
      let (kind, criteria) = match question.domain() {
        DecisionSignalQuestionDomain::FiniteChoice(choices) if choices.as_slice().len() <= MAX_JEV_CHOICE_LABELS => {
          let DecisionSignalQuestionCriteria::FiniteChoice(descriptions) = question.criteria() else {
            return None;
          };
          if descriptions
            .iter()
            .any(|criterion| criterion.description().as_str().len() > MAX_JEV_CRITERION_BYTES)
          {
            return None;
          }
          (
            JEV_CHOICE_QUESTION_KIND,
            serde_json::to_value(
              descriptions
                .iter()
                .map(|criterion| (criterion.choice().as_str(), criterion.description().as_str()))
                .collect::<BTreeMap<_, _>>(),
            )
            .ok()?,
          )
        }
        DecisionSignalQuestionDomain::BoundedScore(_) => {
          // JEV returns a decimal expected position for score questions while
          // the current provider-neutral contract represents integer scores.
          // Fail closed until that contract can preserve the exact value.
          return None;
        }
        DecisionSignalQuestionDomain::FiniteChoice(_) => return None,
      };
      Some((
        identity.clone(),
        JevQuestion {
          kind,
          instructions: question.instructions().as_str().to_owned(),
          criteria,
        },
      ))
    })
    .collect::<Option<BTreeMap<_, _>>>()?;
  Some(JevRequest {
    model: request.model().identity().as_str().to_owned(),
    state,
    questions,
  })
}

#[derive(Deserialize)]
struct JevResponse {
  id: String,
  model: String,
  model_version: String,
  provider: String,
  answers: BTreeMap<String, JevAnswer>,
  usage: JevUsage,
  latency_ms: u64,
}

#[derive(Deserialize)]
struct JevUsage {
  input_tokens: u64,
  output_tokens: u64,
  charged_tokens: u64,
  charged_credits: u64,
  wallet: String,
}

#[derive(Deserialize)]
struct JevAnswer {
  #[serde(rename = "type")]
  kind: String,
  choice: Option<String>,
  probabilities: Option<BTreeMap<String, f64>>,
}

#[derive(Deserialize)]
struct JevConflict {
  error: JevConflictError,
  request_id: String,
  status_url: String,
}

#[derive(Deserialize)]
struct JevConflictError {
  code: String,
}

fn decode_answers(
  request: &DecisionSignalProviderRequest,
  wire_answers: &BTreeMap<String, JevAnswer>,
) -> Option<Vec<DecisionSignalAnswer>> {
  request
    .input()
    .questions()
    .iter()
    .map(|question| {
      let wire = wire_answers.get(question.reference().identity().as_str())?;
      let probabilities = wire.probabilities.as_ref()?;
      let value = match question.domain() {
        DecisionSignalQuestionDomain::FiniteChoice(choices) if wire.kind == JEV_CHOICE_QUESTION_KIND => {
          let selected = FactoryKey::new(wire.choice.as_deref()?).ok()?;
          let declared = choices
            .as_slice()
            .iter()
            .map(FactoryKey::as_str)
            .collect::<std::collections::BTreeSet<_>>();
          let returned = probabilities
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
          if !choices.contains(&selected) || returned != declared {
            return None;
          }
          let selected_probability = probability(*probabilities.get(selected.as_str())?)?;
          let runner_up_probability = probabilities
            .iter()
            .filter(|(choice, _)| choice.as_str() != selected.as_str())
            .map(|(_, value)| probability(*value))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .max()?;
          DecisionSignalAnswerValue::FiniteChoice {
            selected,
            selected_probability,
            runner_up_probability,
          }
        }
        DecisionSignalQuestionDomain::BoundedScore(_) => return None,
        _ => return None,
      };
      Some(DecisionSignalAnswer::new(question.reference().clone(), value))
    })
    .collect()
}

fn probability(value: f64) -> Option<DecisionSignalProbability> {
  if !value.is_finite() || !(0.0..=1.0).contains(&value) {
    return None;
  }
  #[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded finite probability is rounded to the fixed-point contract"
  )]
  DecisionSignalProbability::new((value * 1_000_000.0).round() as u32).ok()
}

pub(super) type JevTransportFuture<'a> =
  Pin<Box<dyn Future<Output = Result<JevHttpResponse, JevTransportError>> + Send + 'a>>;

pub(super) trait JevTransport: Send + Sync {
  fn post<'a>(&'a self, endpoint: &'a Url, idempotency_key: &'a str, body: Vec<u8>) -> JevTransportFuture<'a>;

  fn get<'a>(&'a self, endpoint: &'a Url) -> JevTransportFuture<'a>;
}

pub(super) struct JevHttpResponse {
  pub(super) status: StatusCode,
  pub(super) body: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(super) enum JevTransportError {
  TimedOut,
  Unavailable,
}

struct ReqwestTransport {
  client: Client,
}

impl JevTransport for ReqwestTransport {
  fn post<'a>(&'a self, endpoint: &'a Url, idempotency_key: &'a str, body: Vec<u8>) -> JevTransportFuture<'a> {
    Box::pin(async move {
      let response = self
        .client
        .post(endpoint.clone())
        .header(JEV_IDEMPOTENCY_HEADER, idempotency_key)
        .body(body)
        .send()
        .await
        .map_err(classify_transport_error)?;
      bounded_response(response).await
    })
  }

  fn get<'a>(&'a self, endpoint: &'a Url) -> JevTransportFuture<'a> {
    Box::pin(async move {
      let response = self
        .client
        .get(endpoint.clone())
        .send()
        .await
        .map_err(classify_transport_error)?;
      bounded_response(response).await
    })
  }
}

async fn bounded_response(response: reqwest::Response) -> Result<JevHttpResponse, JevTransportError> {
  if response
    .content_length()
    .is_some_and(|length| length > MAX_RESPONSE_BYTES)
  {
    return Ok(JevHttpResponse {
      status: StatusCode::PAYLOAD_TOO_LARGE,
      body: Vec::new(),
    });
  }
  let status = response.status();
  let mut stream = response.bytes_stream();
  let mut body = Vec::new();
  while let Some(chunk) = stream.next().await {
    let chunk = chunk.map_err(classify_transport_error)?;
    if u64::try_from(body.len())
      .ok()
      .and_then(|length| length.checked_add(u64::try_from(chunk.len()).ok()?))
      .is_none_or(|length| length > MAX_RESPONSE_BYTES)
    {
      return Ok(JevHttpResponse {
        status: StatusCode::PAYLOAD_TOO_LARGE,
        body: Vec::new(),
      });
    }
    body.extend_from_slice(&chunk);
  }
  Ok(JevHttpResponse { status, body })
}

fn classify_transport_error(error: reqwest::Error) -> JevTransportError {
  if error.is_timeout() {
    JevTransportError::TimedOut
  } else {
    JevTransportError::Unavailable
  }
}

fn failure(request: &DecisionSignalProviderRequest, state: DecisionSignalState) -> DecisionSignalProviderFailure {
  DecisionSignalProviderFailure::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    state,
    BudgetUsage::default(),
  )
  .expect("adapter failures never use the completed state")
}

fn classify_status(status: StatusCode) -> DecisionSignalState {
  match status.as_u16() {
    400 | 401 | 402 | 404 | 413 | 422 => DecisionSignalState::Invalid,
    499 => DecisionSignalState::Cancelled,
    504 => DecisionSignalState::TimedOut,
    _ => DecisionSignalState::Unavailable,
  }
}

fn valid_request_id(value: &str) -> bool {
  value.strip_prefix("dec_").is_some_and(|suffix| {
    !suffix.is_empty()
      && suffix.len() <= MAX_JEV_REQUEST_ID_BYTES
      && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
  })
}

fn same_origin(left: &Url, right: &Url) -> bool {
  left.scheme() == right.scheme()
    && left.host_str() == right.host_str()
    && left.port_or_known_default() == right.port_or_known_default()
}
