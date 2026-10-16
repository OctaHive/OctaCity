use std::{
  fmt,
  time::{Duration, Instant},
};

use futures_util::StreamExt as _;

use octacity_server_factory::{
  BudgetUsage, DecisionSignalAnswer, DecisionSignalAnswerValue, DecisionSignalInputMedia, DecisionSignalMode,
  DecisionSignalProbability, DecisionSignalProvider, DecisionSignalProviderCapability, DecisionSignalProviderFailure,
  DecisionSignalProviderFuture, DecisionSignalProviderLimits, DecisionSignalProviderRequest,
  DecisionSignalProviderResult, DecisionSignalQuestionCriteria, DecisionSignalQuestionDomain,
  DecisionSignalQuestionKind, DecisionSignalState, FactoryKey, NetworkHost,
};
use reqwest::{
  Client, Url,
  header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde::Deserialize;
use thiserror::Error;
use zeroize::Zeroizing;

/// Secret-safe OpenAI connector installation failure.
#[derive(Debug, Error)]
#[error("OpenAI Decisions connector configuration is invalid")]
pub struct OpenAiDecisionSignalError;

/// OpenAI Decisions transport restricted to shadow evaluation of finite choices.
/// The beta model alias is not represented as an immutable model snapshot.
pub struct OpenAiDecisionSignalProvider {
  capability: DecisionSignalProviderCapability,
  destinations: [NetworkHost; 1],
  endpoint: Url,
  client: Client,
  input_cost_micro_units_per_million_tokens: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
  version: u16,
  endpoint: String,
  capability: DecisionSignalProviderCapability,
  timeout_milliseconds: u64,
  input_cost_micro_units_per_million_tokens: u64,
  #[serde(default)]
  root_certificates_pem: Vec<String>,
}

impl OpenAiDecisionSignalProvider {
  /// Installs a connector without contacting OpenAI; credentials stay separate
  /// from the bounded, operator-owned deployment document.
  pub fn from_deployment_json(bytes: &[u8], credential: String) -> Result<Self, OpenAiDecisionSignalError> {
    let credential = Zeroizing::new(credential);
    if bytes.len() > 4 * 1024 * 1024 {
      return Err(OpenAiDecisionSignalError);
    }
    let config: Configuration = serde_json::from_slice(bytes).map_err(|_| OpenAiDecisionSignalError)?;
    let endpoint = Url::parse(&config.endpoint).map_err(|_| OpenAiDecisionSignalError)?;
    let capability = &config.capability;
    let limits = capability.limits();
    if config.version != 1
      || !(1..=300_000).contains(&config.timeout_milliseconds)
      || config.input_cost_micro_units_per_million_tokens == 0
      || capability.provider().identity().as_str() != "openai"
      || capability.adapter().identity().as_str() != "openai-decisions-http"
      || capability.model().identity().as_str() != "gpt-6-luna"
      || capability.model().version().as_str() != "unversioned-shadow-v1"
      || capability.probability_semantics().identity().as_str() != "openai-choice-probabilities"
      || capability.probability_semantics().version().as_str() != "v1"
      || capability.supported_input_media() != [DecisionSignalInputMedia::CanonicalJson]
      || capability.supported_question_kinds() != [DecisionSignalQuestionKind::FiniteChoice]
      || DecisionSignalProviderLimits::new(limits.max_input_bytes(), limits.max_questions(), limits.max_choices())
        .is_err()
      || endpoint.scheme() != "https"
      || endpoint.host_str().is_none()
      || !endpoint.username().is_empty()
      || endpoint.password().is_some()
      || endpoint.query().is_some()
      || endpoint.fragment().is_some()
      || endpoint.path() != "/v1/decisions"
      || !credential.starts_with("sk-")
      || !(16..=512).contains(&credential.len())
      || !credential.bytes().all(|c| c.is_ascii_graphic())
    {
      return Err(OpenAiDecisionSignalError);
    }
    let destination =
      NetworkHost::new(endpoint.host_str().ok_or(OpenAiDecisionSignalError)?).map_err(|_| OpenAiDecisionSignalError)?;
    let mut bearer = Zeroizing::new(String::from("Bearer "));
    bearer.push_str(&credential);
    let mut authorization = HeaderValue::from_str(&bearer).map_err(|_| OpenAiDecisionSignalError)?;
    authorization.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, authorization);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if config.root_certificates_pem.len() > 8 {
      return Err(OpenAiDecisionSignalError);
    }
    let mut roots = Vec::new();
    for pem in config.root_certificates_pem {
      if pem.len() > 16 * 1024 {
        return Err(OpenAiDecisionSignalError);
      }
      let certificates =
        reqwest::Certificate::from_pem_bundle(pem.as_bytes()).map_err(|_| OpenAiDecisionSignalError)?;
      if certificates.len() != 1 {
        return Err(OpenAiDecisionSignalError);
      }
      roots.extend(certificates);
    }
    let client = Client::builder()
      .default_headers(headers)
      .timeout(Duration::from_millis(config.timeout_milliseconds))
      .redirect(reqwest::redirect::Policy::none())
      // Decisions documents no idempotent replay/status lookup contract.
      // Preserve unknown usage rather than retrying a possibly accepted POST.
      .retry(reqwest::retry::never())
      .tls_certs_merge(roots)
      .build()
      .map_err(|_| OpenAiDecisionSignalError)?;
    Ok(Self {
      capability: config.capability,
      destinations: [destination],
      endpoint,
      client,
      input_cost_micro_units_per_million_tokens: config.input_cost_micro_units_per_million_tokens,
    })
  }

  #[expect(
    clippy::result_large_err,
    reason = "the common provider port returns the immutable failure by value"
  )]
  async fn dispatch(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<DecisionSignalProviderResult, DecisionSignalProviderFailure> {
    if request.mode() != DecisionSignalMode::Shadow
      || request.provider() != self.capability.provider()
      || request.adapter() != self.capability.adapter()
      || request.model() != self.capability.model()
      || request.probability_semantics() != self.capability.probability_semantics()
      || request.data_handling() != self.capability.data_handling()
      || !self.capability.supports(request.request().purpose())
      || !request.request().budget().fits_within(self.capability.budget_ceiling())
    {
      return Err(rejected(request));
    }
    let questions = request
      .input()
      .questions()
      .iter()
      .map(|q| {
        let DecisionSignalQuestionCriteria::FiniteChoice(criteria) = q.criteria() else {
          return None;
        };
        Some(
          serde_json::json!({"name":q.reference().identity().as_str(),"type":"choice",
        "instructions":q.instructions().as_str(),"choices":criteria.iter().map(|c| serde_json::json!({
          "value":c.choice().as_str(),"description":c.description().as_str()})).collect::<Vec<_>>()}),
        )
      })
      .collect::<Option<Vec<_>>>()
      .ok_or_else(|| rejected(request))?;
    let body = serde_json::json!({"model":request.model().identity().as_str(),
      "input":request.input().state_document().as_str(),"questions":questions});
    let start = Instant::now();
    let response = self
      .client
      .post(self.endpoint.clone())
      .json(&body)
      .send()
      .await
      .map_err(|e| {
        unknown(
          request,
          if e.is_timeout() {
            DecisionSignalState::TimedOut
          } else {
            DecisionSignalState::Unavailable
          },
        )
      })?;
    if !response.status().is_success() {
      return Err(unknown(request, DecisionSignalState::Unavailable));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.map_err(|e| {
        unknown(
          request,
          if e.is_timeout() {
            DecisionSignalState::TimedOut
          } else {
            DecisionSignalState::Unavailable
          },
        )
      })?;
      if bytes.len().saturating_add(chunk.len()) > 4 * 1024 * 1024 {
        return Err(unknown(request, DecisionSignalState::Invalid));
      }
      bytes.extend_from_slice(&chunk);
    }
    let wire: Response = serde_json::from_slice(&bytes).map_err(|_| unknown(request, DecisionSignalState::Invalid))?;
    if wire.model != request.model().identity().as_str() || wire.answers.len() != request.input().questions().len() {
      return Err(unknown(request, DecisionSignalState::Invalid));
    }
    let answers = request
      .input()
      .questions()
      .iter()
      .zip(&wire.answers)
      .map(|(q, a)| {
        let DecisionSignalQuestionDomain::FiniteChoice(choices) = q.domain() else {
          return None;
        };
        let selected = FactoryKey::new(&a.choice).ok()?;
        if a.kind != "choice" || a.name != q.reference().identity().as_str() || !choices.contains(&selected) {
          return None;
        }
        let returned = a
          .probabilities
          .iter()
          .map(|p| p.value.as_str())
          .collect::<std::collections::BTreeSet<_>>();
        let declared = choices
          .as_slice()
          .iter()
          .map(FactoryKey::as_str)
          .collect::<std::collections::BTreeSet<_>>();
        if returned != declared
          || returned.len() != a.probabilities.len()
          || probability(a.confidence).is_none()
          || a.probabilities.iter().any(|p| probability(p.probability).is_none())
          || (a.probabilities.iter().map(|p| p.probability).sum::<f64>() - 1.0).abs() > 0.000001
        {
          return None;
        }
        let selected_probability = probability(a.probabilities.iter().find(|p| p.value == a.choice)?.probability)?;
        let runner_up_probability = a
          .probabilities
          .iter()
          .filter(|p| p.value != a.choice)
          .map(|p| probability(p.probability))
          .collect::<Option<Vec<_>>>()?
          .into_iter()
          .max()?;
        Some(DecisionSignalAnswer::new(
          q.reference().clone(),
          DecisionSignalAnswerValue::FiniteChoice {
            selected,
            selected_probability,
            runner_up_probability,
          },
        ))
      })
      .collect::<Option<Vec<_>>>()
      .ok_or_else(|| unknown(request, DecisionSignalState::Invalid))?;
    let tokens = wire
      .usage
      .input_tokens
      .checked_add(wire.usage.output_tokens)
      .ok_or_else(|| unknown(request, DecisionSignalState::Invalid))?;
    if tokens != wire.usage.total_tokens {
      return Err(unknown(request, DecisionSignalState::Invalid));
    }
    let cost = u128::from(wire.usage.input_tokens) * u128::from(self.input_cost_micro_units_per_million_tokens);
    let usage = BudgetUsage {
      attempts: 1,
      elapsed_millis: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
      tokens,
      cost_micro_units: u64::try_from(cost.div_ceil(1_000_000))
        .map_err(|_| unknown(request, DecisionSignalState::Invalid))?,
      output_bytes: bytes.len() as u64,
    };
    if usage.validate(request.request().budget()).is_err() {
      return Err(unknown(request, DecisionSignalState::Invalid));
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
    .map_err(|_| unknown(request, DecisionSignalState::Invalid))
  }
}

#[derive(Deserialize)]
struct Response {
  model: String,
  answers: Vec<Answer>,
  usage: Usage,
}
#[derive(Deserialize)]
struct Answer {
  name: String,
  #[serde(rename = "type")]
  kind: String,
  choice: String,
  probabilities: Vec<Probability>,
  confidence: f64,
}
#[derive(Deserialize)]
struct Probability {
  value: String,
  probability: f64,
}
#[derive(Deserialize)]
struct Usage {
  input_tokens: u64,
  output_tokens: u64,
  total_tokens: u64,
}

fn probability(value: f64) -> Option<DecisionSignalProbability> {
  if !value.is_finite() || !(0.0..=1.0).contains(&value) {
    return None;
  }
  #[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite bounded probability is rounded to fixed point"
  )]
  let ppm = (value * 1_000_000.0).round() as u32;
  DecisionSignalProbability::new(ppm).ok()
}
fn rejected(request: &DecisionSignalProviderRequest) -> DecisionSignalProviderFailure {
  DecisionSignalProviderFailure::new(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    DecisionSignalState::Invalid,
    BudgetUsage::default(),
  )
  .expect("invalid is a failure")
}
fn unknown(request: &DecisionSignalProviderRequest, state: DecisionSignalState) -> DecisionSignalProviderFailure {
  DecisionSignalProviderFailure::unobserved(
    request.id(),
    request.provider().clone(),
    request.adapter().clone(),
    request.model().clone(),
    state,
  )
  .expect("non-completed failure state")
}

impl fmt::Debug for OpenAiDecisionSignalProvider {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("OpenAiDecisionSignalProvider").finish_non_exhaustive()
  }
}

impl DecisionSignalProvider for OpenAiDecisionSignalProvider {
  fn capability(&self) -> &DecisionSignalProviderCapability {
    &self.capability
  }
  fn network_destinations(&self) -> Option<&[NetworkHost]> {
    Some(&self.destinations)
  }
  fn evaluate<'a>(&'a self, request: &'a DecisionSignalProviderRequest) -> DecisionSignalProviderFuture<'a> {
    Box::pin(self.dispatch(request))
  }
}
