//! Concrete bounded event spool made of immutable per-sequence records.
//!
//! A record is created and synced before it becomes sendable. Acknowledging a
//! contiguous prefix first syncs the cursor and then removes those immutable
//! files, allowing a recovered network connection to release backpressure.

use std::{collections::VecDeque, fs, io::Write as _, path::PathBuf};

use octacity_protocol::{AttemptEventEnvelope, LeaseFence, MAX_EVENT_BATCH_RECORDS};
use thiserror::Error;

/// Local durability and batching limits supplied by agent configuration.
#[derive(Clone, Debug)]
pub struct SpoolLimits {
  /// Maximum encoded bytes retained before runner backpressure applies.
  pub max_bytes: u64,
  /// Maximum unacknowledged event records retained locally.
  pub max_records: usize,
  /// Maximum encoded bytes selected for one coordinator append.
  pub batch_bytes: usize,
  /// Maximum records selected for one coordinator append.
  pub batch_records: usize,
}

impl SpoolLimits {
  /// Validates that batch limits fit within spool and wire bounds.
  pub fn validate(&self) -> Result<(), SpoolError> {
    if self.max_bytes == 0 || self.max_records == 0 || self.batch_bytes == 0 || self.batch_records == 0 {
      return Err(SpoolError::Invalid("spool limits must be greater than zero".to_owned()));
    }
    if self.batch_records > self.max_records || self.batch_records > MAX_EVENT_BATCH_RECORDS {
      return Err(SpoolError::Invalid(
        "event batch record limit exceeds a spool or protocol bound".to_owned(),
      ));
    }
    if self.batch_bytes as u64 > self.max_bytes {
      return Err(SpoolError::Invalid(
        "event batch byte limit exceeds the spool byte limit".to_owned(),
      ));
    }
    Ok(())
  }
}

#[derive(Debug, Error)]
/// Failure to configure or durably mutate the attempt event spool.
pub enum SpoolError {
  /// Local limits are zero or contradict one another.
  #[error("invalid event spool configuration: {0}")]
  Invalid(String),
  /// One encoded record cannot fit within the configured spool and batch.
  #[error("event record requires {record_bytes} bytes, exceeding the spool limit {maximum_bytes}")]
  RecordTooLarge {
    /// Encoded record size.
    record_bytes: u64,
    /// Effective per-record byte ceiling.
    maximum_bytes: u64,
  },
  /// Backpressure must wait for the delivery task to acknowledge records.
  #[error("event spool is full")]
  Full,
  /// A local spool filesystem operation failed.
  #[error("event spool I/O failed: {0}")]
  Io(#[source] std::io::Error),
  /// An event could not be encoded for durable storage.
  #[error("event spool JSON failed: {0}")]
  Json(#[source] serde_json::Error),
  /// The coordinator acknowledged data outside the current contiguous range.
  #[error("invalid event acknowledgement {acknowledged}; current range is {current}..={last}")]
  InvalidAcknowledgement {
    /// Cursor returned by the coordinator.
    acknowledged: u64,
    /// Last cursor already persisted locally.
    current: u64,
    /// Last event sequence created locally.
    last: u64,
  },
}

struct Record {
  envelope: AttemptEventEnvelope,
  path: PathBuf,
  bytes: u64,
}

/// Append-only unacknowledged records for one fenced attempt.
pub(crate) struct EventSpool {
  events_dir: PathBuf,
  acknowledgements: fs::File,
  fence: LeaseFence,
  limits: SpoolLimits,
  records: VecDeque<Record>,
  unacknowledged_bytes: u64,
  acknowledged_sequence: u64,
  last_sequence: u64,
}

impl EventSpool {
  /// Creates an empty, exclusive spool below a newly owned attempt directory.
  pub(crate) fn create(attempt_root: PathBuf, fence: LeaseFence, limits: SpoolLimits) -> Result<Self, SpoolError> {
    limits.validate()?;
    let events_dir = attempt_root.join("events");
    fs::create_dir(&events_dir).map_err(SpoolError::Io)?;
    let acknowledgements = fs::OpenOptions::new()
      .create_new(true)
      .write(true)
      .open(attempt_root.join("acknowledgements"))
      .map_err(SpoolError::Io)?;
    Ok(Self {
      events_dir,
      acknowledgements,
      fence,
      limits,
      records: VecDeque::new(),
      unacknowledged_bytes: 0,
      acknowledged_sequence: 0,
      last_sequence: 0,
    })
  }

  /// Persists an event and returns its assigned attempt-stream sequence.
  pub(crate) fn append(&mut self, kind: octacity_protocol::AttemptEventKind) -> Result<u64, SpoolError> {
    let sequence = self
      .last_sequence
      .checked_add(1)
      .ok_or_else(|| SpoolError::Invalid("event sequence overflowed".to_owned()))?;
    let envelope = AttemptEventEnvelope {
      job_id: self.fence.job_id.clone(),
      attempt: self.fence.attempt,
      lease_id: self.fence.lease_id.clone(),
      fencing_token: self.fence.fencing_token.clone(),
      stream_sequence: sequence,
      occurred_at_unix_ms: unix_now_millis(),
      kind,
    };
    envelope
      .validate(&self.fence)
      .map_err(|error| SpoolError::Invalid(error.to_string()))?;
    let mut encoded = serde_json::to_vec(&envelope).map_err(SpoolError::Json)?;
    encoded.push(b'\n');
    let bytes = encoded.len() as u64;
    let maximum_record_bytes = self.limits.max_bytes.min(self.limits.batch_bytes as u64);
    if bytes > maximum_record_bytes {
      return Err(SpoolError::RecordTooLarge {
        record_bytes: bytes,
        maximum_bytes: maximum_record_bytes,
      });
    }
    if self.records.len() >= self.limits.max_records
      || self.unacknowledged_bytes.saturating_add(bytes) > self.limits.max_bytes
    {
      return Err(SpoolError::Full);
    }
    let path = self.events_dir.join(format!("{sequence:020}.json"));
    let mut file = fs::OpenOptions::new()
      .create_new(true)
      .write(true)
      .open(&path)
      .map_err(SpoolError::Io)?;
    file.write_all(&encoded).map_err(SpoolError::Io)?;
    file.sync_data().map_err(SpoolError::Io)?;
    self.records.push_back(Record { envelope, path, bytes });
    self.unacknowledged_bytes += bytes;
    self.last_sequence = sequence;
    Ok(sequence)
  }

  /// Clones the oldest batch without advancing the durable cursor.
  pub(crate) fn pending_batch(&self) -> Vec<AttemptEventEnvelope> {
    let mut bytes = 0_usize;
    self
      .records
      .iter()
      .take(self.limits.batch_records)
      .take_while(|record| {
        let Ok(record_bytes) = usize::try_from(record.bytes) else {
          return false;
        };
        if bytes != 0 && bytes.saturating_add(record_bytes) > self.limits.batch_bytes {
          return false;
        }
        bytes = bytes.saturating_add(record_bytes);
        true
      })
      .map(|record| record.envelope.clone())
      .collect()
  }

  /// Durably advances the contiguous cursor before reclaiming acknowledged files.
  pub(crate) fn acknowledge(&mut self, acknowledged: u64) -> Result<(), SpoolError> {
    if acknowledged < self.acknowledged_sequence || acknowledged > self.last_sequence {
      return Err(SpoolError::InvalidAcknowledgement {
        acknowledged,
        current: self.acknowledged_sequence,
        last: self.last_sequence,
      });
    }
    if acknowledged == self.acknowledged_sequence {
      return Ok(());
    }
    writeln!(self.acknowledgements, "{acknowledged}").map_err(SpoolError::Io)?;
    self.acknowledgements.sync_data().map_err(SpoolError::Io)?;
    while self
      .records
      .front()
      .is_some_and(|record| record.envelope.stream_sequence <= acknowledged)
    {
      let record = self.records.pop_front().expect("front record was present");
      fs::remove_file(record.path).map_err(SpoolError::Io)?;
      self.unacknowledged_bytes -= record.bytes;
    }
    self.acknowledged_sequence = acknowledged;
    Ok(())
  }

  /// Returns whether every persisted record has been acknowledged.
  pub(crate) fn is_empty(&self) -> bool {
    self.records.is_empty()
  }

  #[cfg(test)]
  fn acknowledged_sequence(&self) -> u64 {
    self.acknowledged_sequence
  }

  /// Returns the last sequence assigned by this spool.
  pub(crate) fn last_sequence(&self) -> u64 {
    self.last_sequence
  }

  #[cfg(test)]
  fn unacknowledged_records(&self) -> usize {
    self.records.len()
  }
}

fn unix_now_millis() -> i64 {
  let millis = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis();
  i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
  use octacity_protocol::{AgentLifecycleEvent, AttemptEventKind, JobLifecycleState};

  use super::*;

  fn fence() -> LeaseFence {
    LeaseFence {
      lease_id: "lease-1".to_owned(),
      job_id: "job-1".to_owned(),
      attempt: 1,
      fencing_token: "fence-1".to_owned(),
    }
  }

  fn event(state: JobLifecycleState) -> AttemptEventKind {
    AttemptEventKind::Agent {
      event: AgentLifecycleEvent::StateChanged { state },
    }
  }

  fn limits(max_records: usize, batch_records: usize) -> SpoolLimits {
    SpoolLimits {
      max_bytes: 16 * 1024,
      max_records,
      batch_bytes: 8 * 1024,
      batch_records,
    }
  }

  #[test]
  fn persists_contiguous_batches_and_reclaims_acknowledged_records() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("attempt");
    fs::create_dir(&root).unwrap();
    let mut spool = EventSpool::create(root.clone(), fence(), limits(3, 2)).unwrap();
    assert_eq!(spool.append(event(JobLifecycleState::Preparing)).unwrap(), 1);
    assert_eq!(spool.append(event(JobLifecycleState::Running)).unwrap(), 2);
    assert_eq!(spool.append(event(JobLifecycleState::Freezing)).unwrap(), 3);
    assert!(matches!(
      spool.append(event(JobLifecycleState::Cleaning)),
      Err(SpoolError::Full)
    ));
    assert_eq!(
      spool
        .pending_batch()
        .iter()
        .map(|event| event.stream_sequence)
        .collect::<Vec<_>>(),
      vec![1, 2]
    );
    spool.acknowledge(2).unwrap();
    assert_eq!(spool.acknowledged_sequence(), 2);
    assert_eq!(spool.unacknowledged_records(), 1);
    assert_eq!(spool.append(event(JobLifecycleState::Cleaning)).unwrap(), 4);
    assert!(!root.join("events/00000000000000000001.json").exists());
    assert!(root.join("events/00000000000000000003.json").exists());
  }

  #[test]
  fn rejects_records_and_acknowledgements_outside_bounds() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("attempt");
    fs::create_dir(&root).unwrap();
    let mut spool = EventSpool::create(
      root,
      fence(),
      SpoolLimits {
        max_bytes: 1,
        max_records: 1,
        batch_bytes: 1,
        batch_records: 1,
      },
    )
    .unwrap();
    assert!(matches!(
      spool.append(event(JobLifecycleState::Preparing)),
      Err(SpoolError::RecordTooLarge { .. })
    ));

    let root = directory.path().join("another-attempt");
    fs::create_dir(&root).unwrap();
    let mut spool = EventSpool::create(root, fence(), limits(2, 2)).unwrap();
    spool.append(event(JobLifecycleState::Preparing)).unwrap();
    assert!(matches!(
      spool.acknowledge(2),
      Err(SpoolError::InvalidAcknowledgement { .. })
    ));
  }
}
