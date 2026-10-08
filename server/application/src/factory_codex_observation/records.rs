//! Strict wire records emitted by the pinned Codex plugin.

use std::collections::BTreeMap;

use octacity_server_factory::{FactoryRepositoryPath, FactorySafeText, ImplementationOutcome};
use serde::Deserialize;

use super::{CodexHarnessOutcome, FactoryImplementationReport};

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum HarnessOutcomeWire {
  Completed,
  Blocked,
  NeedsInput,
  BudgetExhausted,
  Failed,
}

impl From<HarnessOutcomeWire> for CodexHarnessOutcome {
  fn from(value: HarnessOutcomeWire) -> Self {
    match value {
      HarnessOutcomeWire::Completed => Self::Completed,
      HarnessOutcomeWire::Blocked => Self::Blocked,
      HarnessOutcomeWire::NeedsInput => Self::NeedsInput,
      HarnessOutcomeWire::BudgetExhausted => Self::BudgetExhausted,
      HarnessOutcomeWire::Failed => Self::Failed,
    }
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodexResultRecord {
  pub(super) format_version: u16,
  pub(super) outcome: HarnessOutcomeWire,
  pub(super) structured_result: ImplementationReportWire,
  pub(super) harness_identifiers: BTreeMap<String, String>,
  pub(super) usage: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImplementationReportWire {
  outcome: ImplementationOutcome,
  summary: String,
  decisions: Vec<String>,
  assumptions: Vec<String>,
  unresolved_items: Vec<String>,
  changed_components: Vec<String>,
  validation_observations: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CodexProvenanceRecord {
  pub(super) format_version: u16,
  pub(super) trace_format_version: u16,
  pub(super) result_format_version: u16,
  pub(super) plugin: SoftwareIdentity,
  pub(super) codex: SoftwareIdentity,
  pub(super) settings: InvocationSettings,
  pub(super) prompt_digest: DigestIdentity,
  pub(super) source_revision: Option<String>,
  pub(super) timing: Timing,
  pub(super) outcome: HarnessOutcomeWire,
  pub(super) usage: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SoftwareIdentity {
  pub(super) name: String,
  pub(super) version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InvocationSettings {
  pub(super) model: Option<String>,
  pub(super) reasoning_effort: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DigestIdentity {
  pub(super) algorithm: String,
  pub(super) value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Timing {
  pub(super) started_unix_millis: u64,
  pub(super) finished_unix_millis: u64,
  pub(super) duration_millis: u64,
}

pub(super) fn implementation_report(
  harness_outcome: CodexHarnessOutcome,
  wire: ImplementationReportWire,
) -> Option<FactoryImplementationReport> {
  let mut decisions = safe_texts(wire.decisions)?;
  let mut assumptions = safe_texts(wire.assumptions)?;
  let mut unresolved_items = safe_texts(wire.unresolved_items)?;
  let mut changed_components = wire
    .changed_components
    .into_iter()
    .map(FactoryRepositoryPath::new)
    .collect::<Result<Vec<_>, _>>()
    .ok()?;
  let mut validation_observations = safe_texts(wire.validation_observations)?;
  canonicalize(&mut decisions)?;
  canonicalize(&mut assumptions)?;
  canonicalize(&mut unresolved_items)?;
  canonicalize(&mut changed_components)?;
  canonicalize(&mut validation_observations)?;
  Some(FactoryImplementationReport {
    harness_outcome,
    outcome: wire.outcome,
    summary: FactorySafeText::new(wire.summary).ok()?,
    decisions,
    assumptions,
    unresolved_items,
    changed_components,
    validation_observations,
  })
}

fn safe_texts(values: Vec<String>) -> Option<Vec<FactorySafeText>> {
  values
    .into_iter()
    .map(FactorySafeText::new)
    .collect::<Result<Vec<_>, _>>()
    .ok()
}

fn canonicalize<T: Ord>(values: &mut [T]) -> Option<()> {
  values.sort();
  values.windows(2).all(|pair| pair[0] != pair[1]).then_some(())
}
