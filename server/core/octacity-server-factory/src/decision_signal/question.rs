use crate::{FactoryDigest, FactoryError, FactoryKey, FactoryText, ImmutableReference};

/// Maximum finite choices accepted by one Decision Signal request.
pub const MAX_DECISION_SIGNAL_CHOICES: usize = 32;
/// Maximum versioned questions accepted by one Decision Signal request.
pub const MAX_DECISION_SIGNAL_QUESTIONS: usize = 32;
/// Fixed-point denominator used for provider-specific probability values.
pub const DECISION_SIGNAL_PROBABILITY_SCALE: u32 = 1_000_000;

/// A provider-reported probability represented as millionths, without floating-point ambiguity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DecisionSignalProbability(u32);

impl DecisionSignalProbability {
  /// Constructs a probability in the inclusive `0..=1_000_000` range.
  pub const fn new(parts_per_million: u32) -> Result<Self, FactoryError> {
    if parts_per_million > DECISION_SIGNAL_PROBABILITY_SCALE {
      return Err(FactoryError::InvalidDecisionSignal { field: "probability" });
    }
    Ok(Self(parts_per_million))
  }

  /// Returns the fixed-point value in millionths.
  #[must_use]
  pub const fn parts_per_million(self) -> u32 {
    self.0
  }
}

/// Non-empty, duplicate-free finite answer domain retained in request order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalChoices(Vec<FactoryKey>);

impl DecisionSignalChoices {
  /// Constructs a bounded finite domain with at least two distinct choices.
  pub fn try_new(choices: Vec<FactoryKey>) -> Result<Self, FactoryError> {
    if !(2..=MAX_DECISION_SIGNAL_CHOICES).contains(&choices.len()) {
      return Err(FactoryError::CollectionLimitExceeded {
        collection: "Decision Signal choices",
      });
    }
    let mut unique = choices.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != choices.len() {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "duplicate choice",
      });
    }
    Ok(Self(choices))
  }

  /// Borrows the finite choices in their canonical request order.
  #[must_use]
  pub fn as_slice(&self) -> &[FactoryKey] {
    &self.0
  }

  /// Reports whether the domain declares a choice.
  #[must_use]
  pub fn contains(&self, choice: &FactoryKey) -> bool {
    self.0.contains(choice)
  }
}

/// Finite outgoing routes declared for one immutable lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalRouteSet {
  state: FactoryKey,
  choices: DecisionSignalChoices,
}

impl DecisionSignalRouteSet {
  /// Binds a state identity to its complete finite outgoing route domain.
  #[must_use]
  pub const fn new(state: FactoryKey, choices: DecisionSignalChoices) -> Self {
    Self { state, choices }
  }

  /// Returns the declared lifecycle state.
  #[must_use]
  pub const fn state(&self) -> &FactoryKey {
    &self.state
  }

  /// Returns all routes declared for the state.
  #[must_use]
  pub fn choices(&self) -> &DecisionSignalChoices {
    &self.choices
  }
}

/// Provider-neutral media type used for a bounded redacted state document.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DecisionSignalInputMedia {
  /// Canonical JSON encoded as UTF-8.
  CanonicalJson,
  /// Plain UTF-8 text with no provider message envelope.
  Utf8Text,
}

/// Provider-neutral question shape supported by a Decision Signal capability.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DecisionSignalQuestionKind {
  /// One answer from a finite declared choice set.
  FiniteChoice,
  /// One score from a finite versioned numeric domain.
  BoundedScore,
}

/// Inclusive finite integer score domain for one versioned question.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionSignalScoreDomain {
  minimum: i32,
  maximum: i32,
}

impl DecisionSignalScoreDomain {
  /// Constructs a non-empty bounded score domain.
  pub const fn new(minimum: i32, maximum: i32) -> Result<Self, FactoryError> {
    if minimum >= maximum {
      return Err(FactoryError::InvalidDecisionSignal { field: "score domain" });
    }
    Ok(Self { minimum, maximum })
  }

  /// Reports whether the domain contains a score.
  #[must_use]
  pub const fn contains(self, score: i32) -> bool {
    score >= self.minimum && score <= self.maximum
  }

  /// Returns the inclusive lower bound.
  #[must_use]
  pub const fn minimum(self) -> i32 {
    self.minimum
  }

  /// Returns the inclusive upper bound.
  #[must_use]
  pub const fn maximum(self) -> i32 {
    self.maximum
  }
}

/// Finite provider-neutral answer domain for one versioned question.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionSignalQuestionDomain {
  /// Exactly one declared symbolic answer.
  FiniteChoice(DecisionSignalChoices),
  /// Exactly one integer inside an inclusive bounded range.
  BoundedScore(DecisionSignalScoreDomain),
}

/// One finite choice and the semantic boundary description shown to a provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalChoiceCriterion {
  choice: FactoryKey,
  description: FactoryText,
}

impl DecisionSignalChoiceCriterion {
  /// Binds one declared choice to its bounded provider-neutral description.
  #[must_use]
  pub const fn new(choice: FactoryKey, description: FactoryText) -> Self {
    Self { choice, description }
  }

  /// Returns the declared finite choice.
  #[must_use]
  pub const fn choice(&self) -> &FactoryKey {
    &self.choice
  }

  /// Returns the semantic boundary description for the choice.
  #[must_use]
  pub const fn description(&self) -> &FactoryText {
    &self.description
  }
}

/// Semantic criteria paired with one typed Decision Signal domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionSignalQuestionCriteria {
  /// One description for every finite choice, in the domain's canonical order.
  FiniteChoice(Vec<DecisionSignalChoiceCriterion>),
  /// Ordered tier descriptions from the score domain's minimum to maximum.
  BoundedScore(Vec<FactoryText>),
}

impl DecisionSignalQuestionDomain {
  pub(super) fn kind(&self) -> DecisionSignalQuestionKind {
    match self {
      Self::FiniteChoice(_) => DecisionSignalQuestionKind::FiniteChoice,
      Self::BoundedScore(_) => DecisionSignalQuestionKind::BoundedScore,
    }
  }
}

/// One exact versioned question and its finite answer domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalQuestion {
  reference: ImmutableReference,
  instructions: FactoryText,
  domain: DecisionSignalQuestionDomain,
  criteria: DecisionSignalQuestionCriteria,
}

impl DecisionSignalQuestion {
  /// Binds one immutable question definition to semantic instructions and a
  /// complete typed answer-domain description.
  pub fn new(
    reference: ImmutableReference,
    instructions: FactoryText,
    domain: DecisionSignalQuestionDomain,
    criteria: DecisionSignalQuestionCriteria,
  ) -> Result<Self, FactoryError> {
    if !criteria_match_domain(&domain, &criteria) {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "question criteria",
      });
    }
    Ok(Self {
      reference,
      instructions,
      domain,
      criteria,
    })
  }

  /// Returns the exact question definition.
  #[must_use]
  pub const fn reference(&self) -> &ImmutableReference {
    &self.reference
  }

  /// Returns the provider-neutral decision instructions.
  #[must_use]
  pub const fn instructions(&self) -> &FactoryText {
    &self.instructions
  }

  /// Returns the finite typed answer domain.
  #[must_use]
  pub const fn domain(&self) -> &DecisionSignalQuestionDomain {
    &self.domain
  }

  /// Returns the semantic descriptions aligned with the typed domain.
  #[must_use]
  pub const fn criteria(&self) -> &DecisionSignalQuestionCriteria {
    &self.criteria
  }
}

fn criteria_match_domain(domain: &DecisionSignalQuestionDomain, criteria: &DecisionSignalQuestionCriteria) -> bool {
  match (domain, criteria) {
    (DecisionSignalQuestionDomain::FiniteChoice(choices), DecisionSignalQuestionCriteria::FiniteChoice(criteria)) => {
      choices.as_slice().len() == criteria.len()
        && choices
          .as_slice()
          .iter()
          .zip(criteria)
          .all(|(choice, criterion)| choice == criterion.choice())
    }
    (DecisionSignalQuestionDomain::BoundedScore(domain), DecisionSignalQuestionCriteria::BoundedScore(criteria)) => {
      i64::try_from(criteria.len())
        .is_ok_and(|count| count == i64::from(domain.maximum()) - i64::from(domain.minimum()) + 1)
    }
    _ => false,
  }
}

/// Bounded request limits advertised by one exact provider/model capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionSignalProviderLimits {
  max_input_bytes: u32,
  max_questions: u16,
  max_choices: u16,
}

impl DecisionSignalProviderLimits {
  /// Constructs positive limits within the Factory's safety ceilings.
  pub const fn new(max_input_bytes: u32, max_questions: u16, max_choices: u16) -> Result<Self, FactoryError> {
    if max_input_bytes == 0 || max_input_bytes > crate::MAX_FACTORY_TEXT_BYTES as u32 {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "maximum input bytes",
      });
    }
    if max_questions == 0 || max_questions > MAX_DECISION_SIGNAL_QUESTIONS as u16 {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "maximum questions",
      });
    }
    if max_choices < 2 || max_choices > MAX_DECISION_SIGNAL_CHOICES as u16 {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "maximum choices",
      });
    }
    Ok(Self {
      max_input_bytes,
      max_questions,
      max_choices,
    })
  }

  /// Returns the maximum canonical input bytes.
  #[must_use]
  pub const fn max_input_bytes(self) -> u32 {
    self.max_input_bytes
  }

  /// Returns the maximum versioned questions.
  #[must_use]
  pub const fn max_questions(self) -> u16 {
    self.max_questions
  }

  /// Returns the maximum finite choices in one question domain.
  #[must_use]
  pub const fn max_choices(self) -> u16 {
    self.max_choices
  }
}

/// Bounded provider input with no provider-specific message or wire type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSignalProviderInput {
  media: DecisionSignalInputMedia,
  state_document: FactoryText,
  questions: Vec<DecisionSignalQuestion>,
  consuming_question: FactoryKey,
  routing_state: Option<FactoryKey>,
  digest: FactoryDigest,
}

impl DecisionSignalProviderInput {
  /// Canonicalizes a bounded redacted document and binds versioned questions.
  pub fn new(
    media: DecisionSignalInputMedia,
    state_document: FactoryText,
    questions: Vec<DecisionSignalQuestion>,
    consuming_question: FactoryKey,
    routing_state: Option<FactoryKey>,
  ) -> Result<Self, FactoryError> {
    if questions.is_empty() || questions.len() > MAX_DECISION_SIGNAL_QUESTIONS {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "question count",
      });
    }
    let mut identities = questions
      .iter()
      .map(|question| question.reference().identity())
      .collect::<Vec<_>>();
    identities.sort_unstable();
    identities.dedup();
    if identities.len() != questions.len()
      || !questions.iter().any(|question| {
        question.reference().identity() == &consuming_question
          && matches!(question.domain(), DecisionSignalQuestionDomain::FiniteChoice(_))
      })
    {
      return Err(FactoryError::InvalidDecisionSignal {
        field: "consuming question",
      });
    }
    let state_document = canonicalize_state_document(media, state_document)?;
    let digest = digest_provider_input(
      media,
      &state_document,
      &questions,
      &consuming_question,
      routing_state.as_ref(),
    );
    Ok(Self {
      media,
      state_document,
      questions,
      consuming_question,
      routing_state,
      digest,
    })
  }

  /// Returns the provider-neutral input media type.
  #[must_use]
  pub const fn media(&self) -> DecisionSignalInputMedia {
    self.media
  }

  /// Returns the bounded redacted state document.
  #[must_use]
  pub const fn state_document(&self) -> &FactoryText {
    &self.state_document
  }

  /// Returns the number of versioned questions in the exact question set.
  #[must_use]
  pub fn questions(&self) -> &[DecisionSignalQuestion] {
    &self.questions
  }

  /// Returns the finite domain whose answer deterministic policy consumes.
  #[must_use]
  pub fn choices(&self) -> &DecisionSignalChoices {
    self
      .questions
      .iter()
      .find(|question| question.reference().identity() == &self.consuming_question)
      .and_then(|question| match question.domain() {
        DecisionSignalQuestionDomain::FiniteChoice(choices) => Some(choices),
        DecisionSignalQuestionDomain::BoundedScore(_) => None,
      })
      .expect("constructor requires a finite consuming question")
  }

  /// Returns the exact consuming-question identity.
  #[must_use]
  pub const fn consuming_question(&self) -> &FactoryKey {
    &self.consuming_question
  }

  /// Returns the exact lifecycle state for a routing request.
  #[must_use]
  pub const fn routing_state(&self) -> Option<&FactoryKey> {
    self.routing_state.as_ref()
  }

  /// Returns the digest calculated from the canonical document and questions.
  #[must_use]
  pub const fn digest(&self) -> FactoryDigest {
    self.digest
  }
}

fn canonicalize_state_document(
  media: DecisionSignalInputMedia,
  document: FactoryText,
) -> Result<FactoryText, FactoryError> {
  if media == DecisionSignalInputMedia::Utf8Text {
    return Ok(document);
  }
  let value =
    serde_json::from_str::<serde_json::Value>(document.as_str()).map_err(|_| FactoryError::InvalidDecisionSignal {
      field: "canonical JSON input",
    })?;
  let canonical = serde_json::to_string(&value).map_err(|_| FactoryError::InvalidDecisionSignal {
    field: "canonical JSON input",
  })?;
  FactoryText::new(canonical)
}

fn digest_provider_input(
  media: DecisionSignalInputMedia,
  document: &FactoryText,
  questions: &[DecisionSignalQuestion],
  consuming_question: &FactoryKey,
  routing_state: Option<&FactoryKey>,
) -> FactoryDigest {
  let media = match media {
    DecisionSignalInputMedia::CanonicalJson => b"canonical-json".as_slice(),
    DecisionSignalInputMedia::Utf8Text => b"utf8-text".as_slice(),
  };
  let routing_state = routing_state.map_or(b"".as_slice(), |state| state.as_str().as_bytes());
  let mut fields = vec![
    media,
    document.as_str().as_bytes(),
    consuming_question.as_str().as_bytes(),
    routing_state,
  ];
  let encoded_questions = questions.iter().map(encode_question).collect::<Vec<_>>();
  fields.extend(encoded_questions.iter().map(Vec::as_slice));
  FactoryDigest::sha256("octacity.decision-signal.input.v1", &fields)
}

fn encode_question(question: &DecisionSignalQuestion) -> Vec<u8> {
  let mut value = format!(
    "{}\0{}\0{}",
    question.reference().identity(),
    question.reference().version(),
    question.reference().digest()
  )
  .into_bytes();
  value.push(0);
  value.extend_from_slice(question.instructions().as_str().as_bytes());
  match question.domain() {
    DecisionSignalQuestionDomain::FiniteChoice(choices) => {
      value.extend_from_slice(b"\0choice");
      let DecisionSignalQuestionCriteria::FiniteChoice(criteria) = question.criteria() else {
        unreachable!("question criteria are validated by construction")
      };
      for (choice, criterion) in choices.as_slice().iter().zip(criteria) {
        value.push(0);
        value.extend_from_slice(choice.as_str().as_bytes());
        value.push(0);
        value.extend_from_slice(criterion.description().as_str().as_bytes());
      }
    }
    DecisionSignalQuestionDomain::BoundedScore(domain) => {
      value.extend_from_slice(format!("\0score\0{}\0{}", domain.minimum(), domain.maximum()).as_bytes());
      let DecisionSignalQuestionCriteria::BoundedScore(criteria) = question.criteria() else {
        unreachable!("question criteria are validated by construction")
      };
      for criterion in criteria {
        value.push(0);
        value.extend_from_slice(criterion.as_str().as_bytes());
      }
    }
  }
  value
}
