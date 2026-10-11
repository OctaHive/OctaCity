//! Immutable public repository deliverables, independent of logical Flow nodes.
use crate::{FactoryDigest, FactoryError, FactoryKey, FactoryRepositoryPath, FactoryTaskSubject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One configured class of developer-visible changes required for this Work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateDeliverableRequirement {
  /// Operator-owned category, with no stage semantics.
  pub kind: FactoryKey,
  /// Permitted directory or file alternatives for this category.
  pub paths: Vec<FactoryRepositoryPath>,
  /// Minimum distinct added or updated files; deletions do not qualify.
  pub min_files: u16,
}

/// Frozen repository scope and required applicable deliverable categories.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PolicyWire")]
pub struct CandidateDeliverablePolicy {
  permitted_paths: Vec<FactoryRepositoryPath>,
  forbidden_paths: Vec<FactoryRepositoryPath>,
  required: Vec<CandidateDeliverableRequirement>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyWire {
  permitted_paths: Vec<FactoryRepositoryPath>,
  forbidden_paths: Vec<FactoryRepositoryPath>,
  required: Vec<CandidateDeliverableRequirement>,
}
impl TryFrom<PolicyWire> for CandidateDeliverablePolicy {
  type Error = FactoryError;
  fn try_from(w: PolicyWire) -> Result<Self, Self::Error> {
    if !valid_paths(&w.permitted_paths)
      || (!w.forbidden_paths.is_empty() && !valid_paths(&w.forbidden_paths))
      || w.required.is_empty()
      || w.required.len() > crate::MAX_FLOW_SCHEMA_ENTRIES
      || !w.required.windows(2).all(|pair| pair[0].kind < pair[1].kind)
      || w.required.iter().any(|r| {
        r.min_files == 0
          || usize::from(r.min_files) > crate::MAX_FLOW_SCHEMA_ITEMS as usize
          || !valid_paths(&r.paths)
          || r.paths.iter().any(|path| {
            !w.permitted_paths.iter().any(|root| path.is_within(root))
              || w.forbidden_paths.iter().any(|root| path.is_within(root))
          })
      })
    {
      return Err(invalid());
    }
    Ok(Self {
      permitted_paths: w.permitted_paths,
      forbidden_paths: w.forbidden_paths,
      required: w.required,
    })
  }
}
fn valid_paths(paths: &[FactoryRepositoryPath]) -> bool {
  !paths.is_empty()
    && paths.len() <= crate::MAX_FLOW_SCHEMA_ENTRIES
    && paths.windows(2).all(|pair| pair[0] < pair[1])
    && !paths
      .iter()
      .enumerate()
      .any(|(i, path)| paths.iter().take(i).any(|parent| path.is_within(parent)))
}

impl CandidateDeliverablePolicy {
  /// Reports whether this policy only narrows an installed capture ceiling.
  /// Every original exclusion and required class must remain enforceable.
  #[must_use]
  pub fn is_no_broader_than(&self, ceiling: &Self) -> bool {
    self
      .permitted_paths
      .iter()
      .all(|path| ceiling.permitted_paths.iter().any(|root| path.is_within(root)))
      && ceiling
        .forbidden_paths
        .iter()
        .all(|path| self.forbidden_paths.iter().any(|root| path.is_within(root)))
      && ceiling.required.iter().all(|required| {
        self.required.iter().any(|selected| {
          selected.kind == required.kind
            && selected.min_files >= required.min_files
            && selected
              .paths
              .iter()
              .all(|path| required.paths.iter().any(|root| path.is_within(root)))
        })
      })
  }
}

/// Trusted observations of exact candidate file bytes; absence denotes deletion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateFileObservation {
  /// Portable repository-relative changed path.
  pub path: FactoryRepositoryPath,
  /// Raw content SHA-256 independently computed from the exact candidate tree.
  pub content_digest: Option<FactoryDigest>,
}

/// Immutable deliverable contract derived from accepted Work and a pinned policy.
/// The input digest also fixes the selected predecessor and declared rework input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ContractWire")]
pub struct CandidateDeliverableContract {
  accepted_contract: FactoryDigest,
  subject: FactoryTaskSubject,
  input_digest: FactoryDigest,
  policy: CandidateDeliverablePolicy,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractWire {
  accepted_contract: FactoryDigest,
  subject: FactoryTaskSubject,
  input_digest: FactoryDigest,
  policy: CandidateDeliverablePolicy,
}
impl TryFrom<ContractWire> for CandidateDeliverableContract {
  type Error = FactoryError;
  fn try_from(w: ContractWire) -> Result<Self, Self::Error> {
    Self::new(w.accepted_contract, w.subject, w.input_digest, w.policy)
  }
}
impl CandidateDeliverableContract {
  /// Freezes the previously verified acceptance identity and all execution inputs.
  pub fn new(
    accepted_contract: FactoryDigest,
    subject: FactoryTaskSubject,
    input_digest: FactoryDigest,
    policy: CandidateDeliverablePolicy,
  ) -> Result<Self, FactoryError> {
    let contract = Self {
      accepted_contract,
      subject,
      input_digest,
      policy,
    };
    contract.digest()?;
    Ok(contract)
  }
  /// Returns the exact base or selected predecessor that may be edited.
  #[must_use]
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }
  /// Returns the independently verified accepted Work identity.
  #[must_use]
  pub const fn accepted_contract(&self) -> FactoryDigest {
    self.accepted_contract
  }
  /// Returns the exact applicable policy frozen for this execution.
  #[must_use]
  pub const fn policy(&self) -> &CandidateDeliverablePolicy {
    &self.policy
  }
  /// Content-addresses acceptance, predecessor, input and every applicable requirement.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
    if bytes.len() > crate::MAX_FLOW_SCHEMA_BYTES {
      return Err(invalid());
    }
    Ok(FactoryDigest::sha256(
      "octacity.factory.candidate-deliverables.v1",
      &[&bytes],
    ))
  }
  /// Checks independently captured changes; model claims never satisfy requirements.
  /// Protected content hashes come only from the validator's explicit test bundle.
  pub fn verify_files(
    &self,
    files: &[CandidateFileObservation],
    protected: &[FactoryDigest],
  ) -> Result<(), FactoryError> {
    self.verify_candidate(files, files, protected)
  }
  /// Checks the current edit scope and cumulative Work deliverables separately.
  /// Qualified cumulative observations come from the original immutable Work revision;
  /// they may contain previously accepted changes outside this node's writable scope.
  /// Earlier required updates survive rework only while still present in that result.
  pub fn verify_candidate(
    &self,
    changes: &[CandidateFileObservation],
    cumulative: &[CandidateFileObservation],
    protected: &[FactoryDigest],
  ) -> Result<(), FactoryError> {
    if !valid_observations(changes)
      || !valid_observations(cumulative)
      || protected.len() > crate::MAX_CONTEXT_MANIFEST_ENTRIES
      || changes.iter().any(|file| {
        !self.policy.permitted_paths.iter().any(|root| file.path.is_within(root))
          || self.policy.forbidden_paths.iter().any(|root| file.path.is_within(root))
      })
      || changes
        .iter()
        .chain(cumulative)
        .any(|file| file.content_digest.is_some_and(|digest| protected.contains(&digest)))
      || self.policy.required.iter().any(|required| {
        cumulative
          .iter()
          .filter(|file| file.content_digest.is_some() && required.paths.iter().any(|root| file.path.is_within(root)))
          .count()
          < usize::from(required.min_files)
      })
    {
      return Err(invalid());
    }
    Ok(())
  }
}
fn valid_observations(files: &[CandidateFileObservation]) -> bool {
  files.len() <= crate::MAX_FLOW_SCHEMA_ITEMS as usize
    && files.iter().map(|file| &file.path).collect::<BTreeSet<_>>().len() == files.len()
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Candidate deliverables",
  }
}

/// Pinned capture parameters and an explicit projection of verified acceptance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateDeliverableBinding {
  /// Declared input selection of the accepted contract digest.
  pub accepted_contract: crate::FlowDataMapping,
  /// Immutable applicable path and category requirements.
  pub policy: CandidateDeliverablePolicy,
}
impl CandidateDeliverableBinding {
  /// Freezes one contract without reading hidden artifacts or mutable state.
  pub fn freeze(&self, input: &crate::FlowNodeInput) -> Result<CandidateDeliverableContract, FactoryError> {
    let schema = crate::FlowDataSchema::new(
      FactoryKey::new("factory.accepted-contract-digest")?,
      FactoryKey::new("v1")?,
      crate::FlowValueSchema::String {
        min_bytes: 64,
        max_bytes: 64,
      },
    )?;
    let selected = self.accepted_contract.project(input.payload(), &schema)?;
    let digest = selected
      .value()
      .as_str()
      .ok_or_else(invalid)?
      .parse()
      .map_err(|_| invalid())?;
    CandidateDeliverableContract::new(
      digest,
      input.context().ok_or_else(invalid)?.subject().clone(),
      input.digest()?,
      self.policy.clone(),
    )
  }
}
