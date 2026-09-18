//! Manual, scheduled, external, and internal Trigger normalization.
//!
//! This module owns occurrence identity and deduplication rules independently
//! of ready-Job placement.
//!
//! A **Trigger** is a versioned rule. A [`NormalizedTriggerOccurrence`] is one
//! source instance of that rule. The **Trigger Engine** evaluates the durable
//! occurrence and creates at most one Build; it is neither the Orchestrator nor
//! the Placement Scheduler. See the [canonical glossary] and the [server
//! ownership guide].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md
//! [server ownership guide]: https://github.com/OctaHive/OctaCity/blob/main/docs/server-architecture.md

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod occurrence;
mod state;

pub use occurrence::{
  MAX_TRIGGER_EVENT_KIND_BYTES, MAX_TRIGGER_METADATA_BYTES, MAX_TRIGGER_METADATA_ENTRIES,
  MAX_TRIGGER_METADATA_KEY_BYTES, MAX_TRIGGER_REVISION_BYTES, NormalizedTriggerOccurrence, TriggerCausality,
  TriggerCause, TriggerDeduplicationKey, TriggerDefinitionRef, TriggerEventKind, TriggerInputError, TriggerKind,
  TriggerMetadata, TriggerOccurrenceIntent, TriggerTarget,
};
pub use state::{TriggerOccurrenceEvent, TriggerOccurrenceState};
