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

mod internal;
mod occurrence;
mod schedule;
mod state;

pub use internal::{
  InternalTriggerProtection, MAX_INTERNAL_TRIGGER_DEPTH, derive_internal_causality, validate_internal_ancestry,
};
pub use occurrence::{
  MAX_TRIGGER_EVENT_KIND_BYTES, MAX_TRIGGER_METADATA_BYTES, MAX_TRIGGER_METADATA_ENTRIES,
  MAX_TRIGGER_METADATA_KEY_BYTES, MAX_TRIGGER_REVISION_BYTES, NormalizedTriggerOccurrence, TerminalBuildEvent,
  TriggerCausality, TriggerCause, TriggerDeduplicationKey, TriggerDefinitionRef, TriggerEventKind, TriggerInputError,
  TriggerKind, TriggerMetadata, TriggerOccurrenceIntent, TriggerTarget,
};
pub use schedule::{
  DueScheduleOccurrences, MAX_SCHEDULE_CATCH_UP, MAX_SCHEDULE_EXPRESSION_BYTES, MAX_SCHEDULE_TIMEZONE_BYTES,
  MissedRunPolicy, ScheduleDefinition, ScheduleInputError,
};
pub use state::{TriggerOccurrenceEvent, TriggerOccurrenceState};
