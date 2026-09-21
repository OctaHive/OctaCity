use octacity_protocol::LeaseFence as ProtocolLeaseFence;
use octacity_server_domain::{AttemptNumber, JobId, LeaseId, Timestamp};
use octacity_server_store::{LeaseAccess, LeaseFence};

use crate::AuthorizedAgent;

pub(crate) struct ParsedLease {
  pub(crate) access: LeaseAccess,
  pub(crate) job_id: JobId,
  pub(crate) attempt: AttemptNumber,
}

pub(crate) fn parse_lease(lease: &ProtocolLeaseFence, authorized: &AuthorizedAgent) -> Result<ParsedLease, ()> {
  Ok(ParsedLease {
    access: LeaseAccess {
      lease_id: lease.lease_id.parse::<LeaseId>().map_err(|_| ())?,
      fence: decode_fence(&lease.fencing_token)?,
      agent_id: authorized.agent_id(),
      registration_epoch: authorized.registration_epoch(),
    },
    job_id: lease.job_id.parse::<JobId>().map_err(|_| ())?,
    attempt: AttemptNumber::new(u64::from(lease.attempt)).map_err(|_| ())?,
  })
}

pub(crate) fn timestamp(value: i64) -> Result<Timestamp, ()> {
  Timestamp::from_unix_millis(value).map_err(|_| ())
}

fn decode_fence(value: &str) -> Result<LeaseFence, ()> {
  if value.len() != 64 || !value.is_ascii() {
    return Err(());
  }
  let mut bytes = [0_u8; 32];
  let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
  if !remainder.is_empty() {
    return Err(());
  }
  for (target, pair) in bytes.iter_mut().zip(pairs) {
    let high = hex_nibble(pair[0]).ok_or(())?;
    let low = hex_nibble(pair[1]).ok_or(())?;
    *target = (high << 4) | low;
  }
  Ok(LeaseFence::from_bytes(bytes))
}

const fn hex_nibble(value: u8) -> Option<u8> {
  match value {
    b'0'..=b'9' => Some(value - b'0'),
    b'a'..=b'f' => Some(value - b'a' + 10),
    b'A'..=b'F' => Some(value - b'A' + 10),
    _ => None,
  }
}
