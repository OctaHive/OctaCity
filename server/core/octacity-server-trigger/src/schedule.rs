use std::str::FromStr;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum number of missed occurrences one schedule claim may materialize.
pub const MAX_SCHEDULE_CATCH_UP: u16 = 64;
/// Maximum UTF-8 bytes in one persisted cron expression.
pub const MAX_SCHEDULE_EXPRESSION_BYTES: usize = 256;
/// Maximum UTF-8 bytes in one persisted IANA timezone name.
pub const MAX_SCHEDULE_TIMEZONE_BYTES: usize = 128;

/// Policy applied when more than one persisted occurrence is due.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MissedRunPolicy {
  /// Run the oldest persisted occurrence once and advance past all other missed times.
  RunOnce,
  /// Run oldest occurrences in bounded batches without discarding remaining work.
  CatchUp {
    /// Maximum occurrences returned by one durable claim.
    maximum_occurrences: u16,
  },
}

impl MissedRunPolicy {
  fn limit(self) -> Result<usize, ScheduleInputError> {
    match self {
      Self::RunOnce => Ok(1),
      Self::CatchUp { maximum_occurrences } if (1..=MAX_SCHEDULE_CATCH_UP).contains(&maximum_occurrences) => {
        Ok(usize::from(maximum_occurrences))
      }
      Self::CatchUp { .. } => Err(ScheduleInputError::InvalidCatchUpLimit),
    }
  }
}

/// Durable calendar definition for one scheduled Trigger version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleDefinition {
  /// Cron expression evaluated in the named timezone.
  pub expression: String,
  /// IANA timezone name such as `Europe/Moscow`.
  pub timezone: String,
  /// Explicit handling for occurrences missed while workers were unavailable.
  pub missed_run_policy: MissedRunPolicy,
}

impl ScheduleDefinition {
  /// Validates syntax, timezone, and bounded missed-run behavior.
  pub fn validate(&self) -> Result<(), ScheduleInputError> {
    self.parsed()?;
    self.missed_run_policy.limit()?;
    Ok(())
  }

  /// Finds the first occurrence strictly after the supplied instant.
  pub fn next_after(&self, instant: Timestamp) -> Result<Timestamp, ScheduleInputError> {
    let (schedule, timezone) = self.parsed()?;
    next_timestamp(&schedule, timezone, instant)
  }

  /// Selects the bounded due batch and its durable continuation.
  pub fn due_occurrences(
    &self,
    next_occurrence_at: Timestamp,
    observed_at: Timestamp,
  ) -> Result<DueScheduleOccurrences, ScheduleInputError> {
    self.validate()?;
    if next_occurrence_at > observed_at {
      return Ok(DueScheduleOccurrences {
        occurrences: Vec::new(),
        next_occurrence_at,
      });
    }

    match self.missed_run_policy {
      MissedRunPolicy::RunOnce => Ok(DueScheduleOccurrences {
        occurrences: vec![next_occurrence_at],
        next_occurrence_at: self.next_after(observed_at)?,
      }),
      MissedRunPolicy::CatchUp { .. } => {
        let limit = self.missed_run_policy.limit()?;
        let mut occurrences = Vec::with_capacity(limit);
        let mut next = next_occurrence_at;
        while next <= observed_at && occurrences.len() < limit {
          occurrences.push(next);
          next = self.next_after(next)?;
        }
        Ok(DueScheduleOccurrences {
          occurrences,
          next_occurrence_at: next,
        })
      }
    }
  }

  fn parsed(&self) -> Result<(Schedule, Tz), ScheduleInputError> {
    if self.expression.trim() != self.expression
      || self.expression.is_empty()
      || self.expression.len() > MAX_SCHEDULE_EXPRESSION_BYTES
    {
      return Err(ScheduleInputError::InvalidExpression);
    }
    if self.timezone.trim() != self.timezone
      || self.timezone.is_empty()
      || self.timezone.len() > MAX_SCHEDULE_TIMEZONE_BYTES
    {
      return Err(ScheduleInputError::InvalidTimezone);
    }
    let schedule = Schedule::from_str(&self.expression).map_err(|_| ScheduleInputError::InvalidExpression)?;
    let timezone = Tz::from_str(&self.timezone).map_err(|_| ScheduleInputError::InvalidTimezone)?;
    Ok((schedule, timezone))
  }
}

/// Bounded work selected from one persisted schedule cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DueScheduleOccurrences {
  /// Due source times ordered from oldest to newest.
  pub occurrences: Vec<Timestamp>,
  /// Cursor to persist after every selected occurrence has been evaluated.
  pub next_occurrence_at: Timestamp,
}

/// Stable validation failure for a schedule definition or calculation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ScheduleInputError {
  /// The cron expression is empty or invalid.
  #[error("invalid schedule expression")]
  InvalidExpression,
  /// The timezone is not a recognized IANA name.
  #[error("invalid schedule timezone")]
  InvalidTimezone,
  /// The catch-up batch size is zero or exceeds the domain bound.
  #[error("invalid schedule catch-up limit")]
  InvalidCatchUpLimit,
  /// No next occurrence exists inside the supported timestamp range.
  #[error("schedule has no supported next occurrence")]
  NoNextOccurrence,
}

fn next_timestamp(schedule: &Schedule, timezone: Tz, instant: Timestamp) -> Result<Timestamp, ScheduleInputError> {
  let utc =
    DateTime::<Utc>::from_timestamp_millis(instant.unix_millis()).ok_or(ScheduleInputError::NoNextOccurrence)?;
  let next = schedule
    .after(&utc.with_timezone(&timezone))
    .next()
    .ok_or(ScheduleInputError::NoNextOccurrence)?;
  Timestamp::from_unix_millis(next.timestamp_millis()).map_err(|_| ScheduleInputError::NoNextOccurrence)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn timestamp(value: &str) -> Timestamp {
    let value = DateTime::parse_from_rfc3339(value).unwrap().timestamp_millis();
    Timestamp::from_unix_millis(value).unwrap()
  }

  #[test]
  fn spring_clock_gap_uses_the_next_real_local_hour() {
    let schedule = ScheduleDefinition {
      expression: "0 30 2 * * * *".into(),
      timezone: "Europe/Berlin".into(),
      missed_run_policy: MissedRunPolicy::RunOnce,
    };

    assert_eq!(
      schedule.next_after(timestamp("2025-03-29T02:30:00+01:00")).unwrap(),
      timestamp("2025-03-31T02:30:00+02:00")
    );
  }

  #[test]
  fn fall_clock_fold_does_not_duplicate_one_calendar_occurrence() {
    let schedule = ScheduleDefinition {
      expression: "0 30 2 * * * *".into(),
      timezone: "Europe/Berlin".into(),
      missed_run_policy: MissedRunPolicy::CatchUp { maximum_occurrences: 4 },
    };
    let first = schedule.next_after(timestamp("2025-10-26T01:00:00+02:00")).unwrap();
    let second = schedule.next_after(first).unwrap();

    assert_eq!(first, timestamp("2025-10-26T02:30:00+02:00"));
    assert_eq!(second, timestamp("2025-10-27T02:30:00+01:00"));
  }

  #[test]
  fn catch_up_is_bounded_and_retains_the_next_due_cursor() {
    let schedule = ScheduleDefinition {
      expression: "0 * * * * * *".into(),
      timezone: "UTC".into(),
      missed_run_policy: MissedRunPolicy::CatchUp { maximum_occurrences: 2 },
    };
    let due = schedule
      .due_occurrences(timestamp("2025-01-01T00:01:00Z"), timestamp("2025-01-01T00:05:30Z"))
      .unwrap();

    assert_eq!(due.occurrences.len(), 2);
    assert_eq!(due.next_occurrence_at, timestamp("2025-01-01T00:03:00Z"));
  }

  #[test]
  fn occurrence_is_due_at_the_exact_clock_boundary_but_not_before_it() {
    let schedule = ScheduleDefinition {
      expression: "0 * * * * * *".into(),
      timezone: "UTC".into(),
      missed_run_policy: MissedRunPolicy::RunOnce,
    };
    let next = timestamp("2025-01-01T00:01:00Z");

    assert!(
      schedule
        .due_occurrences(next, timestamp("2025-01-01T00:00:59.999Z"))
        .unwrap()
        .occurrences
        .is_empty()
    );
    assert_eq!(schedule.due_occurrences(next, next).unwrap().occurrences, [next]);
  }

  #[test]
  fn invalid_timezone_and_unbounded_catch_up_are_rejected() {
    let invalid_timezone = ScheduleDefinition {
      expression: "0 * * * * * *".into(),
      timezone: "Local/Guess".into(),
      missed_run_policy: MissedRunPolicy::RunOnce,
    };
    let invalid_limit = ScheduleDefinition {
      expression: "0 * * * * * *".into(),
      timezone: "UTC".into(),
      missed_run_policy: MissedRunPolicy::CatchUp {
        maximum_occurrences: MAX_SCHEDULE_CATCH_UP + 1,
      },
    };

    assert_eq!(invalid_timezone.validate(), Err(ScheduleInputError::InvalidTimezone));
    assert_eq!(invalid_limit.validate(), Err(ScheduleInputError::InvalidCatchUpLimit));
  }
}
