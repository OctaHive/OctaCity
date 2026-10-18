//! Conditional native writes. Only opaque field identities and tokens leave the adapter.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Token {
  intent: String,
  observed: String,
  mutation: Mutation,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Mutation {
  Remove(String),
  Add(String),
  State(IssueState),
}
fn hash(domain: &str, value: &impl Serialize) -> Result<String, WorkApiFailure> {
  let bytes = serde_json::to_vec(&(domain, value)).map_err(|_| FailureClass::ProtocolFault)?;
  Ok(hex::encode(Sha256::digest(bytes)))
}
pub(super) fn mutation(step: &reporter::ProjectionStep) -> Result<Mutation, WorkApiFailure> {
  let bytes = STANDARD.decode(&step.token).map_err(|_| FailureClass::Conflict)?;
  let token: Token = serde_json::from_slice(&bytes).map_err(|_| FailureClass::Conflict)?;
  Ok(token.mutation)
}
pub(super) fn scope_labels<'a>(
  mapping: &'a StatusMapping,
  effects: &crate::status_mapping::StatusEffects,
) -> Vec<&'a String> {
  mapping
    .managed_groups()
    .filter(|group| {
      group.iter().any(|name| {
        effects
          .add()
          .iter()
          .chain(effects.remove())
          .any(|effect| effect.to_lowercase() == name.to_lowercase())
      })
    })
    .flatten()
    .collect()
}
pub(super) fn plan(
  mapping: &StatusMapping,
  api_base: &url::Url,
  intent: &reporter::ProjectionIntent,
  ids: &BTreeMap<String, u64>,
  state: IssueState,
  labels: &[Label],
) -> Result<reporter::ProjectionPlan, WorkApiFailure> {
  // Stable native scope ignores installation aliases, credentials and mapping versions.
  let scope = (api_base.as_str(), &intent.resource.resource_id, &intent.item_id);
  let effects = mapping.effects(&intent.status).map_err(|_| FailureClass::Unsupported)?;
  for label in labels {
    if let Some(expected) = ids.get(&label.name.to_lowercase()) {
      if label.id != Some(*expected) {
        return Err(FailureClass::Conflict.into());
      }
    } else if label.id.is_some_and(|id| ids.values().any(|expected| *expected == id)) {
      return Err(FailureClass::Conflict.into());
    }
  }
  let contains = |name: &str| labels.iter().any(|v| v.name.to_lowercase() == name.to_lowercase());
  let mut fields = BTreeSet::new();
  let mut steps = Vec::new();
  let intent_digest = hash("octacity.github.reporter.intent.v2", intent)?;
  for (name, add) in effects
    .remove()
    .iter()
    .map(|v| (v, false))
    .chain(effects.add().iter().map(|v| (v, true)))
  {
    let group = mapping
      .managed_groups()
      .find(|group| group.iter().any(|v| v.to_lowercase() == name.to_lowercase()))
      .ok_or(FailureClass::Permanent)?;
    let mut owned = Vec::new();
    for name in group {
      let normalized = name.to_lowercase();
      let id = ids.get(&normalized).ok_or(FailureClass::ProtocolFault)?;
      owned.push(hash("octacity.github.issue.label-id.v2", &(scope, id))?);
      // Name-addressed requests can resolve a recreated label after dispatch;
      // native ids alone would miss that overlap. Renames retain the id key.
      owned.push(hash("octacity.github.issue.label-name.v2", &(scope, &normalized))?);
    }
    owned.sort();
    fields.extend(owned.iter().cloned());
    if contains(name) == add {
      continue;
    }
    let observed: BTreeSet<_> = group
      .iter()
      .filter(|v| contains(v))
      .map(|v| ids.get(&v.to_lowercase()).copied().ok_or(FailureClass::ProtocolFault))
      .collect::<Result<_, _>>()?;
    let token = Token {
      intent: intent_digest.clone(),
      observed: hash("octacity.github.reporter.observation.v2", &observed)?,
      mutation: if add {
        Mutation::Add(name.clone())
      } else {
        Mutation::Remove(name.clone())
      },
    };
    steps.push(reporter::ProjectionStep {
      fields: owned,
      token: STANDARD.encode(serde_json::to_vec(&token).map_err(|_| FailureClass::ProtocolFault)?),
    });
  }
  if let Some(desired) = effects.state() {
    let field = hash("octacity.github.issue.state.v2", &scope)?;
    fields.insert(field.clone());
    if state != desired {
      let token = Token {
        intent: intent_digest,
        observed: hash("octacity.github.reporter.observation.v2", &state)?,
        mutation: Mutation::State(desired),
      };
      steps.push(reporter::ProjectionStep {
        fields: vec![field],
        token: STANDARD.encode(serde_json::to_vec(&token).map_err(|_| FailureClass::ProtocolFault)?),
      });
    }
  }
  let plan = reporter::ProjectionPlan {
    receipt: reporter::ProjectionReceipt {
      effects: intent.effects.clone(),
      projection_id: intent.projection_id.clone(),
      resource_id: intent.resource.resource_id.clone(),
      item_id: intent.item_id.clone(),
      configuration_digest: intent.configuration_digest.clone(),
      mapping: intent.mapping.clone(),
      status: intent.status.clone(),
      matches: steps.is_empty(),
    },
    fields: fields.into_iter().collect(),
    steps,
  };
  plan.validate_for(intent).map_err(|_| FailureClass::ProtocolFault)?;
  Ok(plan)
}
