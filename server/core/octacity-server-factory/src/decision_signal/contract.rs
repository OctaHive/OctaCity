use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{
  BudgetUsage, DecisionSignalFallback, DecisionSignalMode, DecisionSignalPurpose, DecisionSignalRequest,
  DecisionSignalRequestId, DecisionSignalState, FactoryDigest, FactoryError, FactoryKey, FactoryText,
  ImmutableReference,
};

use super::question::{
  DecisionSignalChoices, DecisionSignalInputMedia, DecisionSignalProbability, DecisionSignalProviderInput,
  DecisionSignalProviderLimits, DecisionSignalQuestion, DecisionSignalQuestionDomain, DecisionSignalQuestionKind,
  DecisionSignalRouteSet, MAX_DECISION_SIGNAL_QUESTIONS,
};

/// Exact capability advertised by one provider adapter and model pair.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalProviderCapability {
  provider: ImmutableReference,
  adapter: ImmutableReference,
  model: ImmutableReference,
  probability_semantics: ImmutableReference,
  data_handling: ImmutableReference,
  supported_purposes: Vec<DecisionSignalPurpose>,
  supported_input_media: Vec<DecisionSignalInputMedia>,
  supported_question_kinds: Vec<DecisionSignalQuestionKind>,
  limits: DecisionSignalProviderLimits,
  budget_ceiling: crate::BudgetLimit,
}

impl DecisionSignalProviderCapability {
  /// Constructs a bounded exact capability advertisement.
  #[expect(
    clippy::too_many_arguments,
    reason = "capability binds all exact provider limits and semantics"
  )]
  pub fn new(
    provider: ImmutableReference,
    adapter: ImmutableReference,
    model: ImmutableReference,
    probability_semantics: ImmutableReference,
    data_handling: ImmutableReference,
    mut supported_purposes: Vec<DecisionSignalPurpose>,
    mut supported_input_media: Vec<DecisionSignalInputMedia>,
    mut supported_question_kinds: Vec<DecisionSignalQuestionKind>,
    limits: DecisionSignalProviderLimits,
    budget_ceiling: crate::BudgetLimit,
  ) -> Result<Self, FactoryError> {
    let purpose_count = supported_purposes.len();
    supported_purposes.sort_unstable();
    supported_purposes.dedup();
    if supported_purposes.is_empty() || supported_purposes.len() > 2 || supported_purposes.len() != purpose_count {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "supported purposes",
      });
    }
    let media_count = supported_input_media.len();
    supported_input_media.sort_unstable();
    supported_input_media.dedup();
    let question_kind_count = supported_question_kinds.len();
    supported_question_kinds.sort_unstable();
    supported_question_kinds.dedup();
    if supported_input_media.is_empty()
      || supported_question_kinds.is_empty()
      || supported_input_media.len() != media_count
      || supported_question_kinds.len() != question_kind_count
    {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "provider capability shape",
      });
    }
    Ok(Self {
      provider,
      adapter,
      model,
      probability_semantics,
      data_handling,
      supported_purposes,
      supported_input_media,
      supported_question_kinds,
      limits,
      budget_ceiling,
    })
  }

  /// Returns the exact decision service identity.
  #[must_use]
  pub const fn provider(&self) -> &ImmutableReference {
    &self.provider
  }

  /// Returns the exact provider adapter identity.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    &self.adapter
  }

  /// Returns the exact model identity.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns the exact provider/model-specific probability semantics.
  #[must_use]
  pub const fn probability_semantics(&self) -> &ImmutableReference {
    &self.probability_semantics
  }

  /// Returns the exact provider data-handling policy.
  #[must_use]
  pub const fn data_handling(&self) -> &ImmutableReference {
    &self.data_handling
  }

  /// Reports whether the capability supports a Decision Signal purpose.
  #[must_use]
  pub fn supports(&self, purpose: DecisionSignalPurpose) -> bool {
    self.supported_purposes.contains(&purpose)
  }

  /// Returns the supported purposes in canonical order.
  #[must_use]
  pub fn supported_purposes(&self) -> &[DecisionSignalPurpose] {
    &self.supported_purposes
  }

  /// Returns supported state-document media in canonical order.
  #[must_use]
  pub fn supported_input_media(&self) -> &[DecisionSignalInputMedia] {
    &self.supported_input_media
  }

  /// Returns supported question kinds in canonical order.
  #[must_use]
  pub fn supported_question_kinds(&self) -> &[DecisionSignalQuestionKind] {
    &self.supported_question_kinds
  }

  /// Returns the maximum finite answer count accepted by the provider.
  #[must_use]
  pub const fn limits(&self) -> DecisionSignalProviderLimits {
    self.limits
  }

  /// Returns the provider-enforced request budget ceiling.
  #[must_use]
  pub const fn budget_ceiling(&self) -> crate::BudgetLimit {
    self.budget_ceiling
  }

  fn supports_input(&self, media: DecisionSignalInputMedia) -> bool {
    self.supported_input_media.contains(&media)
  }

  fn supports_question(&self, kind: DecisionSignalQuestionKind) -> bool {
    self.supported_question_kinds.contains(&kind)
  }
}

/// Purpose-specific calibrated threshold interpreted under exact probability semantics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalThreshold {
  purpose: DecisionSignalPurpose,
  probability_semantics: ImmutableReference,
  calibration: ImmutableReference,
  minimum_probability: DecisionSignalProbability,
  minimum_margin: DecisionSignalProbability,
}

impl DecisionSignalThreshold {
  /// Constructs a calibrated threshold for exactly one Decision Signal purpose.
  pub fn new(
    purpose: DecisionSignalPurpose,
    probability_semantics: ImmutableReference,
    calibration: ImmutableReference,
    minimum_probability: DecisionSignalProbability,
    minimum_margin: DecisionSignalProbability,
  ) -> Result<Self, FactoryError> {
    if minimum_probability.parts_per_million() == 0 {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "minimum probability",
      });
    }
    Ok(Self {
      purpose,
      probability_semantics,
      calibration,
      minimum_probability,
      minimum_margin,
    })
  }

  /// Returns the purpose whose calibration corpus produced this threshold.
  #[must_use]
  pub const fn purpose(&self) -> DecisionSignalPurpose {
    self.purpose
  }

  /// Returns the exact provider-specific probability semantics.
  #[must_use]
  pub const fn probability_semantics(&self) -> &ImmutableReference {
    &self.probability_semantics
  }

  /// Returns the exact immutable calibration policy and corpus identity.
  #[must_use]
  pub const fn calibration(&self) -> &ImmutableReference {
    &self.calibration
  }

  /// Returns the minimum accepted selected-answer probability.
  #[must_use]
  pub const fn minimum_probability(&self) -> DecisionSignalProbability {
    self.minimum_probability
  }

  /// Returns the minimum accepted lead over the runner-up answer.
  #[must_use]
  pub const fn minimum_margin(&self) -> DecisionSignalProbability {
    self.minimum_margin
  }

  pub(super) fn accepts(&self, selected: DecisionSignalProbability, runner_up: DecisionSignalProbability) -> bool {
    selected >= self.minimum_probability
      && selected
        .parts_per_million()
        .saturating_sub(runner_up.parts_per_million())
        >= self.minimum_margin.parts_per_million()
  }
}

/// Canonical provider-neutral request sent through the Decision Signal seam.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalProviderRequest {
  request: DecisionSignalRequest,
  provider: ImmutableReference,
  adapter: ImmutableReference,
  model: ImmutableReference,
  question_set: ImmutableReference,
  policy: ImmutableReference,
  probability_semantics: ImmutableReference,
  data_handling: ImmutableReference,
  input: DecisionSignalProviderInput,
  threshold: DecisionSignalThreshold,
  mode: DecisionSignalMode,
  fallback: DecisionSignalFallback,
  deadline: Timestamp,
  digest: FactoryDigest,
}

impl DecisionSignalProviderRequest {
  /// Constructs a request only when the frozen profile and advertised capability agree exactly.
  pub fn new(
    request: DecisionSignalRequest,
    profile: &crate::DecisionSignalProfile,
    capability: &DecisionSignalProviderCapability,
    input: DecisionSignalProviderInput,
    threshold: DecisionSignalThreshold,
    deadline: Timestamp,
  ) -> Result<Self, FactoryError> {
    if request.purpose() != profile.purpose()
      || !capability.supports(request.purpose())
      || profile.provider() != capability.provider()
      || profile.adapter() != capability.adapter()
      || profile.model() != capability.model()
      || request.input_digest() != input.digest()
      || request.policy_digest() != profile.policy().digest()
      || !routing_input_matches_profile(request.purpose(), profile.routes(), &input)
      || threshold.purpose() != request.purpose()
      || threshold.probability_semantics() != capability.probability_semantics()
      || !capability.supports_input(input.media())
      || input
        .questions()
        .iter()
        .any(|question| !capability.supports_question(question.domain().kind()))
      || u32::try_from(input.state_document().as_str().len()).unwrap_or(u32::MAX)
        > capability.limits().max_input_bytes()
      || u16::try_from(input.questions().len()).unwrap_or(u16::MAX) > capability.limits().max_questions()
      || input.questions().iter().any(|question| match question.domain() {
        DecisionSignalQuestionDomain::FiniteChoice(choices) => {
          u16::try_from(choices.as_slice().len()).unwrap_or(u16::MAX) > capability.limits().max_choices()
        }
        DecisionSignalQuestionDomain::BoundedScore(_) => false,
      })
      || !request.budget().fits_within(profile.budget())
      || !profile.budget().fits_within(capability.budget_ceiling())
      || !request.budget().fits_within(capability.budget_ceiling())
    {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "provider request capability",
      });
    }
    let mut provider_request = Self {
      request,
      provider: capability.provider().clone(),
      adapter: capability.adapter().clone(),
      model: capability.model().clone(),
      question_set: profile.question_set().clone(),
      policy: profile.policy().clone(),
      probability_semantics: capability.probability_semantics().clone(),
      data_handling: capability.data_handling().clone(),
      input,
      threshold,
      mode: profile.mode(),
      fallback: profile.fallback(),
      deadline,
      digest: FactoryDigest::from_bytes([0; 32]),
    };
    provider_request.digest = digest_provider_request(&provider_request);
    Ok(provider_request)
  }

  /// Returns the durable logical request identity.
  #[must_use]
  pub const fn id(&self) -> DecisionSignalRequestId {
    self.request.id()
  }

  /// Returns the exact provider-neutral Factory request.
  #[must_use]
  pub const fn request(&self) -> &DecisionSignalRequest {
    &self.request
  }

  /// Returns the exact decision service identity.
  #[must_use]
  pub const fn provider(&self) -> &ImmutableReference {
    &self.provider
  }

  /// Returns the exact provider adapter identity.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    &self.adapter
  }

  /// Returns the exact model identity.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns the exact versioned question set.
  #[must_use]
  pub const fn question_set(&self) -> &ImmutableReference {
    &self.question_set
  }

  /// Returns the exact deterministic consumption policy.
  #[must_use]
  pub const fn policy(&self) -> &ImmutableReference {
    &self.policy
  }

  /// Returns the provider/model-specific probability semantics.
  #[must_use]
  pub const fn probability_semantics(&self) -> &ImmutableReference {
    &self.probability_semantics
  }

  /// Returns the exact provider data-handling policy.
  #[must_use]
  pub const fn data_handling(&self) -> &ImmutableReference {
    &self.data_handling
  }

  /// Returns the bounded redacted canonical state document.
  #[must_use]
  pub const fn state_document(&self) -> &FactoryText {
    self.input.state_document()
  }

  /// Returns the finite declared answer domain.
  #[must_use]
  pub fn choices(&self) -> &DecisionSignalChoices {
    self.input.choices()
  }

  /// Returns the provider-neutral bounded request input.
  #[must_use]
  pub const fn input(&self) -> &DecisionSignalProviderInput {
    &self.input
  }

  /// Returns the calibrated purpose-specific threshold.
  #[must_use]
  pub const fn threshold(&self) -> &DecisionSignalThreshold {
    &self.threshold
  }

  /// Returns the configured rollout mode.
  #[must_use]
  pub const fn mode(&self) -> DecisionSignalMode {
    self.mode
  }

  /// Returns the configured fail-closed fallback.
  #[must_use]
  pub const fn fallback(&self) -> DecisionSignalFallback {
    self.fallback
  }

  /// Returns the immutable request deadline.
  #[must_use]
  pub const fn deadline(&self) -> Timestamp {
    self.deadline
  }

  /// Returns the digest computed from every immutable request field.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }

  /// Revalidates canonical request, input, and policy digests after restoration.
  pub fn validate_integrity(&self) -> Result<(), FactoryError> {
    if self.request.input_digest() != self.input.digest()
      || self.request.policy_digest() != self.policy.digest()
      || self.digest != digest_provider_request(self)
    {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "provider request integrity",
      });
    }
    Ok(())
  }
}

fn routing_input_matches_profile(
  purpose: DecisionSignalPurpose,
  routes: Option<&DecisionSignalRouteSet>,
  input: &DecisionSignalProviderInput,
) -> bool {
  match (purpose, routes, input.routing_state()) {
    (DecisionSignalPurpose::Routing, Some(routes), Some(state)) => {
      routes.state() == state && routes.choices() == input.choices()
    }
    (DecisionSignalPurpose::ToolRisk, None, None) => true,
    _ => false,
  }
}

fn digest_provider_request(request: &DecisionSignalProviderRequest) -> FactoryDigest {
  let request_id = request.id().as_uuid();
  let stage_id = request.request().stage_attempt_id().as_uuid();
  let run_id = request.request().run_id().as_uuid();
  let input_digest = request.input().digest().as_bytes();
  let policy_digest = request.request().policy_digest().as_bytes();
  let provider = encode_reference(request.provider());
  let adapter = encode_reference(request.adapter());
  let model = encode_reference(request.model());
  let question_set = encode_reference(request.question_set());
  let policy = encode_reference(request.policy());
  let semantics = encode_reference(request.probability_semantics());
  let data_handling = encode_reference(request.data_handling());
  let calibration = encode_reference(request.threshold().calibration());
  let budget = encode_budget_limit(request.request().budget());
  let threshold = format!(
    "{}\0{}",
    request.threshold().minimum_probability().parts_per_million(),
    request.threshold().minimum_margin().parts_per_million()
  );
  let deadline = request.deadline().unix_millis().to_be_bytes();
  FactoryDigest::sha256(
    "octacity.decision-signal.request.v1",
    &[
      request_id.as_bytes(),
      stage_id.as_bytes(),
      run_id.as_bytes(),
      request.request().purpose().as_str().as_bytes(),
      input_digest.as_slice(),
      policy_digest.as_slice(),
      provider.as_slice(),
      adapter.as_slice(),
      model.as_slice(),
      question_set.as_slice(),
      policy.as_slice(),
      semantics.as_slice(),
      data_handling.as_slice(),
      calibration.as_slice(),
      budget.as_slice(),
      threshold.as_bytes(),
      request.mode().as_str().as_bytes(),
      request.fallback().as_str().as_bytes(),
      deadline.as_slice(),
    ],
  )
}

fn encode_reference(reference: &ImmutableReference) -> Vec<u8> {
  format!(
    "{}\0{}\0{}",
    reference.identity(),
    reference.version(),
    reference.digest()
  )
  .into_bytes()
}

fn encode_budget_limit(limit: crate::BudgetLimit) -> Vec<u8> {
  format!(
    "{}\0{}\0{}\0{}\0{}",
    limit.max_attempts(),
    limit.max_elapsed_millis(),
    limit.max_tokens(),
    limit.max_cost_micro_units(),
    limit.max_output_bytes()
  )
  .into_bytes()
}

/// Typed value returned for one exact versioned question.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DecisionSignalAnswerValue {
  /// A finite symbolic answer with calibrated probability context.
  FiniteChoice {
    /// Selected declared answer.
    selected: FactoryKey,
    /// Probability assigned to the selected answer.
    selected_probability: DecisionSignalProbability,
    /// Probability assigned to the next-highest answer.
    runner_up_probability: DecisionSignalProbability,
  },
  /// An integer score with provider-reported confidence.
  BoundedScore {
    /// Score inside the question's inclusive domain.
    score: i32,
    /// Confidence interpreted under the recorded semantics.
    confidence: DecisionSignalProbability,
  },
}

/// One provider answer bound to the exact immutable question definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalAnswer {
  question: ImmutableReference,
  value: DecisionSignalAnswerValue,
}

impl DecisionSignalAnswer {
  /// Constructs an answer for one exact question definition.
  #[must_use]
  pub const fn new(question: ImmutableReference, value: DecisionSignalAnswerValue) -> Self {
    Self { question, value }
  }

  /// Returns the exact answered question.
  #[must_use]
  pub const fn question(&self) -> &ImmutableReference {
    &self.question
  }

  /// Returns the typed answer value.
  #[must_use]
  pub const fn value(&self) -> &DecisionSignalAnswerValue {
    &self.value
  }
}

/// Schema-valid typed result returned by a Decision Signal provider adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalProviderResult {
  request_id: DecisionSignalRequestId,
  provider: ImmutableReference,
  adapter: ImmutableReference,
  model: ImmutableReference,
  probability_semantics: ImmutableReference,
  answers: Vec<DecisionSignalAnswer>,
  usage: BudgetUsage,
  result_digest: FactoryDigest,
}

impl DecisionSignalProviderResult {
  /// Constructs a structurally valid result; request binding is checked during consumption.
  pub fn new(
    request_id: DecisionSignalRequestId,
    provider: ImmutableReference,
    adapter: ImmutableReference,
    model: ImmutableReference,
    probability_semantics: ImmutableReference,
    answers: Vec<DecisionSignalAnswer>,
    usage: BudgetUsage,
  ) -> Result<Self, FactoryError> {
    if answers.is_empty() || answers.len() > MAX_DECISION_SIGNAL_QUESTIONS {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "provider answers",
      });
    }
    let mut identities = answers
      .iter()
      .map(|answer| answer.question().identity())
      .collect::<Vec<_>>();
    identities.sort_unstable();
    identities.dedup();
    if identities.len() != answers.len() {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "provider answers",
      });
    }
    let result_digest = digest_provider_result(
      request_id,
      &provider,
      &adapter,
      &model,
      &probability_semantics,
      &answers,
      usage,
    );
    Ok(Self {
      request_id,
      provider,
      adapter,
      model,
      probability_semantics,
      answers,
      usage,
      result_digest,
    })
  }

  /// Returns all typed answers in request order.
  #[must_use]
  pub fn answers(&self) -> &[DecisionSignalAnswer] {
    &self.answers
  }

  /// Returns the stable logical request identity echoed by the provider adapter.
  #[must_use]
  pub const fn request_id(&self) -> DecisionSignalRequestId {
    self.request_id
  }

  /// Returns the exact decision service identity echoed by the result.
  #[must_use]
  pub const fn provider(&self) -> &ImmutableReference {
    &self.provider
  }

  /// Returns the exact provider adapter identity echoed by the result.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    &self.adapter
  }
  /// Returns the exact model identity echoed by the result.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns the probability semantics under which the numeric values were produced.
  #[must_use]
  pub const fn probability_semantics(&self) -> &ImmutableReference {
    &self.probability_semantics
  }

  /// Returns the exact result digest.
  #[must_use]
  pub const fn result_digest(&self) -> FactoryDigest {
    self.result_digest
  }

  /// Returns bounded provider usage validated against the request budget.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Returns terminal provider latency in milliseconds.
  #[must_use]
  pub const fn latency_millis(&self) -> u64 {
    self.usage.elapsed_millis
  }

  pub(super) fn is_valid_for(&self, request: &DecisionSignalProviderRequest) -> bool {
    self.request_id == request.id()
      && &self.provider == request.provider()
      && &self.adapter == request.adapter()
      && &self.model == request.model()
      && &self.probability_semantics == request.probability_semantics()
      && answers_match_questions(&self.answers, request.input().questions())
      && self.usage.validate(request.request().budget()).is_ok()
  }

  pub(super) fn consuming_choice(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Option<(&FactoryKey, DecisionSignalProbability, DecisionSignalProbability)> {
    self.answers.iter().find_map(|answer| {
      if answer.question().identity() != request.input().consuming_question() {
        return None;
      }
      match answer.value() {
        DecisionSignalAnswerValue::FiniteChoice {
          selected,
          selected_probability,
          runner_up_probability,
        } => Some((selected, *selected_probability, *runner_up_probability)),
        DecisionSignalAnswerValue::BoundedScore { .. } => None,
      }
    })
  }
}

fn digest_provider_result(
  request_id: DecisionSignalRequestId,
  provider: &ImmutableReference,
  adapter: &ImmutableReference,
  model: &ImmutableReference,
  semantics: &ImmutableReference,
  answers: &[DecisionSignalAnswer],
  usage: BudgetUsage,
) -> FactoryDigest {
  let request_id = request_id.as_uuid();
  let provider = encode_reference(provider);
  let adapter = encode_reference(adapter);
  let model = encode_reference(model);
  let semantics = encode_reference(semantics);
  let usage = format!(
    "{}\0{}\0{}\0{}\0{}",
    usage.attempts, usage.elapsed_millis, usage.tokens, usage.cost_micro_units, usage.output_bytes
  );
  let encoded_answers = answers.iter().map(encode_answer).collect::<Vec<_>>();
  let mut fields = vec![
    request_id.as_bytes().as_slice(),
    provider.as_slice(),
    adapter.as_slice(),
    model.as_slice(),
    semantics.as_slice(),
    usage.as_bytes(),
  ];
  fields.extend(encoded_answers.iter().map(Vec::as_slice));
  FactoryDigest::sha256("octacity.decision-signal.result.v1", &fields)
}

fn encode_answer(answer: &DecisionSignalAnswer) -> Vec<u8> {
  let mut value = format!(
    "{}\0{}\0{}",
    answer.question().identity(),
    answer.question().version(),
    answer.question().digest()
  )
  .into_bytes();
  match answer.value() {
    DecisionSignalAnswerValue::FiniteChoice {
      selected,
      selected_probability,
      runner_up_probability,
    } => value.extend_from_slice(
      format!(
        "\0choice\0{}\0{}\0{}",
        selected,
        selected_probability.parts_per_million(),
        runner_up_probability.parts_per_million()
      )
      .as_bytes(),
    ),
    DecisionSignalAnswerValue::BoundedScore { score, confidence } => {
      value.extend_from_slice(format!("\0score\0{}\0{}", score, confidence.parts_per_million()).as_bytes())
    }
  }
  value
}

fn answers_match_questions(answers: &[DecisionSignalAnswer], questions: &[DecisionSignalQuestion]) -> bool {
  answers.len() == questions.len()
    && answers.iter().zip(questions).all(|(answer, question)| {
      answer.question() == question.reference()
        && match (answer.value(), question.domain()) {
          (
            DecisionSignalAnswerValue::FiniteChoice {
              selected,
              selected_probability,
              runner_up_probability,
            },
            DecisionSignalQuestionDomain::FiniteChoice(choices),
          ) => choices.contains(selected) && selected_probability >= runner_up_probability,
          (
            DecisionSignalAnswerValue::BoundedScore { score, .. },
            DecisionSignalQuestionDomain::BoundedScore(domain),
          ) => domain.contains(*score),
          _ => false,
        }
    })
}

/// Secret-safe terminal provider failure bound to one exact request and model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalProviderFailure {
  request_id: DecisionSignalRequestId,
  provider: ImmutableReference,
  adapter: ImmutableReference,
  model: ImmutableReference,
  state: DecisionSignalState,
  usage: BudgetUsage,
}

impl DecisionSignalProviderFailure {
  /// Constructs a failure classification without retaining raw provider diagnostics.
  pub fn new(
    request_id: DecisionSignalRequestId,
    provider: ImmutableReference,
    adapter: ImmutableReference,
    model: ImmutableReference,
    state: DecisionSignalState,
    usage: BudgetUsage,
  ) -> Result<Self, FactoryError> {
    if state == DecisionSignalState::Completed {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "provider failure state",
      });
    }
    Ok(Self {
      request_id,
      provider,
      adapter,
      model,
      state,
      usage,
    })
  }

  /// Returns the safe terminal classification.
  #[must_use]
  pub const fn state(&self) -> DecisionSignalState {
    self.state
  }

  /// Returns the stable logical request identity echoed by the adapter.
  #[must_use]
  pub const fn request_id(&self) -> DecisionSignalRequestId {
    self.request_id
  }

  /// Returns the exact provider-adapter identity.
  #[must_use]
  pub const fn provider(&self) -> &ImmutableReference {
    &self.provider
  }

  /// Returns the exact provider adapter identity.
  #[must_use]
  pub const fn adapter(&self) -> &ImmutableReference {
    &self.adapter
  }

  /// Returns the exact model identity.
  #[must_use]
  pub const fn model(&self) -> &ImmutableReference {
    &self.model
  }

  /// Returns bounded usage observed before the terminal failure.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Returns terminal provider latency in milliseconds.
  #[must_use]
  pub const fn latency_millis(&self) -> u64 {
    self.usage.elapsed_millis
  }

  pub(super) fn is_valid_for(&self, request: &DecisionSignalProviderRequest) -> bool {
    self.request_id == request.id()
      && &self.provider == request.provider()
      && &self.adapter == request.adapter()
      && &self.model == request.model()
      && self.usage.validate(request.request().budget()).is_ok()
  }
}

/// Provider observation accepted by the pure Decision Signal consumption policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DecisionSignalProviderObservation {
  /// A typed provider result that still requires exact request validation.
  Result(DecisionSignalProviderResult),
  /// A secret-safe provider failure.
  Failure(DecisionSignalProviderFailure),
}
