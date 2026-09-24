use thiserror::Error;

use crate::{NormalizedTriggerOccurrence, TriggerCausality, TriggerDefinitionRef};

/// Maximum number of internal Trigger derivations permitted from one root.
///
/// A stable Trigger identity may occur only once in a lineage as the primary cycle
/// guard. This independent ceiling also bounds long acyclic chains and the
/// amount of ancestry a durable worker must load.
pub const MAX_INTERNAL_TRIGGER_DEPTH: u16 = 32;

/// Why an internal Trigger candidate is intentionally not evaluated.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InternalTriggerProtection {
  /// The candidate Trigger already appears in the causal lineage.
  #[error("the internal trigger would create a causal cycle")]
  Cycle,
  /// The next derivation would exceed the documented causal-depth ceiling.
  #[error("the internal trigger causal depth limit was reached")]
  DepthLimit,
}

/// Derives bounded lineage for one internal Trigger candidate.
///
/// `ancestry` must contain every Trigger definition from the root occurrence
/// through `parent`, in causal order. Repeating an exact immutable Trigger
/// identity is a cycle even if another immutable Trigger version is now active.
/// Separate non-repeating chains are limited by
/// [`MAX_INTERNAL_TRIGGER_DEPTH`].
pub fn derive_internal_causality(
  parent: &NormalizedTriggerOccurrence,
  candidate: TriggerDefinitionRef,
  ancestry: &[TriggerDefinitionRef],
) -> Result<TriggerCausality, InternalTriggerProtection> {
  if ancestry.iter().any(|ancestor| ancestor.id == candidate.id) {
    return Err(InternalTriggerProtection::Cycle);
  }
  let depth = parent
    .causality
    .depth
    .checked_add(1)
    .filter(|depth| *depth <= MAX_INTERNAL_TRIGGER_DEPTH)
    .ok_or(InternalTriggerProtection::DepthLimit)?;
  Ok(TriggerCausality::derived(
    parent.causality.root_occurrence_id,
    parent.id,
    depth,
  ))
}

/// Validates that a persisted ancestry belongs to the claimed parent.
pub fn validate_internal_ancestry(
  parent: &NormalizedTriggerOccurrence,
  ancestry: &[TriggerDefinitionRef],
) -> Result<(), InternalTriggerProtection> {
  let expected_len = usize::from(parent.causality.depth) + 1;
  if ancestry.len() != expected_len || ancestry.last() != Some(&parent.trigger) {
    return Err(InternalTriggerProtection::Cycle);
  }
  if ancestry.len() > usize::from(MAX_INTERNAL_TRIGGER_DEPTH) + 1 {
    return Err(InternalTriggerProtection::DepthLimit);
  }
  let mut unique = std::collections::BTreeSet::new();
  if ancestry.iter().any(|trigger| !unique.insert(trigger.id)) {
    return Err(InternalTriggerProtection::Cycle);
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{fmt::Debug, str::FromStr};

  use octacity_server_domain::{
    BuildConfigurationVersion, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
  };

  use super::*;
  use crate::{TriggerCause, TriggerEventKind, TriggerMetadata, TriggerTarget};

  #[test]
  fn rejects_a_repeated_trigger_before_deriving_an_occurrence() {
    let parent = root_occurrence(definition(1));
    assert_eq!(
      derive_internal_causality(&parent, definition(1), &[definition(1)]),
      Err(InternalTriggerProtection::Cycle)
    );
  }

  #[test]
  fn rejects_a_different_version_of_a_trigger_already_in_the_lineage() {
    let parent = root_occurrence(definition(1));
    let candidate = TriggerDefinitionRef {
      id: definition(1).id,
      version: TriggerVersion::new(2).unwrap(),
    };
    assert_eq!(
      derive_internal_causality(&parent, candidate, &[definition(1)]),
      Err(InternalTriggerProtection::Cycle)
    );
  }

  #[test]
  fn permits_a_bounded_non_repeating_derivation() {
    let parent = root_occurrence(definition(1));
    assert_eq!(
      derive_internal_causality(&parent, definition(2), &[definition(1)]).unwrap(),
      TriggerCausality::derived(parent.id, parent.id, 1)
    );
  }

  #[test]
  fn preserves_root_causality_across_chains_and_fan_out() {
    let root = root_occurrence(definition(1));
    let left = derive_internal_causality(&root, definition(2), &[definition(1)]).unwrap();
    let right = derive_internal_causality(&root, definition(3), &[definition(1)]).unwrap();
    assert_eq!(left.root_occurrence_id, root.id);
    assert_eq!(right.root_occurrence_id, root.id);
    assert_eq!(left.parent_occurrence_id, Some(root.id));
    assert_eq!(right.parent_occurrence_id, Some(root.id));

    let child = NormalizedTriggerOccurrence::derived(
      id(81),
      definition(2),
      target(),
      TriggerIdentity::new("internal:child").unwrap(),
      TriggerCause::Internal {
        source_build_id: id(82),
        event_kind: TriggerEventKind::new("build.succeeded").unwrap(),
      },
      left,
      Timestamp::from_unix_millis(1).unwrap(),
    )
    .unwrap();
    let grandchild = derive_internal_causality(&child, definition(4), &[definition(1), definition(2)]).unwrap();
    assert_eq!(grandchild.root_occurrence_id, root.id);
    assert_eq!(grandchild.parent_occurrence_id, Some(child.id));
    assert_eq!(grandchild.depth, 2);
  }

  #[test]
  fn rejects_the_next_derivation_at_the_depth_ceiling() {
    let root = id::<TriggerOccurrenceId>(90);
    let parent = NormalizedTriggerOccurrence::derived(
      id(91),
      definition(33),
      target(),
      TriggerIdentity::new("internal:parent").unwrap(),
      TriggerCause::Internal {
        source_build_id: id(92),
        event_kind: crate::TriggerEventKind::new("build.succeeded").unwrap(),
      },
      TriggerCausality::derived(root, id(89), MAX_INTERNAL_TRIGGER_DEPTH),
      Timestamp::from_unix_millis(1).unwrap(),
    )
    .unwrap();
    let ancestry = (1..=33).map(definition).collect::<Vec<_>>();
    assert_eq!(
      derive_internal_causality(&parent, definition(34), &ancestry),
      Err(InternalTriggerProtection::DepthLimit)
    );
  }

  fn root_occurrence(trigger: TriggerDefinitionRef) -> NormalizedTriggerOccurrence {
    let occurrence_id = id(80);
    NormalizedTriggerOccurrence::root(
      occurrence_id,
      trigger,
      target(),
      TriggerIdentity::new("manual:root").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      Timestamp::from_unix_millis(0).unwrap(),
    )
    .unwrap()
  }

  fn definition(value: u128) -> TriggerDefinitionRef {
    TriggerDefinitionRef {
      id: id::<TriggerId>(value),
      version: TriggerVersion::INITIAL,
    }
  }

  fn target() -> TriggerTarget {
    TriggerTarget {
      configuration_id: id(70),
      configuration_version: BuildConfigurationVersion::INITIAL,
    }
  }

  fn id<T>(value: u128) -> T
  where
    T: FromStr,
    T::Err: Debug,
  {
    format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
  }
}
