use std::sync::Arc;

use octacity_server_domain::Timestamp;
use octacity_server_store::{
  ClaimExpiredLeases, LeaseRecoveryAction, LeaseRecoveryStore, RecoverExpiredLease, StoreError, WorkerOwner,
};

/// Summary of one bounded expired-Lease worker pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LeaseExpiryBatchOutcome {
  /// Claims acquired in this pass.
  pub claimed: u16,
  /// Jobs returned to the ready queue.
  pub requeued: u16,
  /// Jobs failed after retry policy exhaustion.
  pub failed: u16,
  /// Jobs terminally cancelled by an existing Build cancellation.
  pub cancelled: u16,
  /// Claims whose Lease was already recovered.
  pub already_recovered: u16,
}

/// Application worker coordinating durable expired-Lease claims and recovery.
pub struct LeaseExpiryWorker<S> {
  store: Arc<S>,
  owner: WorkerOwner,
  batch_size: u16,
}

impl<S> LeaseExpiryWorker<S>
where
  S: LeaseRecoveryStore,
{
  /// Creates one process-instance worker with a validated bounded batch size.
  pub fn new(store: Arc<S>, owner: WorkerOwner, batch_size: u16) -> Result<Self, StoreError> {
    ClaimExpiredLeases::new(
      owner.clone(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      Timestamp::from_unix_millis(1).map_err(|_| StoreError::Unavailable)?,
      batch_size,
    )?;
    Ok(Self {
      store,
      owner,
      batch_size,
    })
  }

  /// Claims and applies one bounded batch using caller-supplied authoritative times.
  pub async fn run_once(
    &self,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
  ) -> Result<LeaseExpiryBatchOutcome, StoreError> {
    let claims = self
      .store
      .claim_expired_leases(ClaimExpiredLeases::new(
        self.owner.clone(),
        observed_at,
        claim_expires_at,
        self.batch_size,
      )?)
      .await?;
    let mut outcome = LeaseExpiryBatchOutcome {
      claimed: u16::try_from(claims.len()).map_err(|_| StoreError::Unavailable)?,
      ..LeaseExpiryBatchOutcome::default()
    };
    for claim in claims {
      let recovered = self
        .store
        .recover_expired_lease(RecoverExpiredLease {
          claim,
          recovered_at: observed_at,
        })
        .await?;
      match recovered.action {
        LeaseRecoveryAction::Requeued => outcome.requeued += 1,
        LeaseRecoveryAction::Failed => outcome.failed += 1,
        LeaseRecoveryAction::Cancelled => outcome.cancelled += 1,
        LeaseRecoveryAction::AlreadyRecovered => outcome.already_recovered += 1,
      }
    }
    Ok(outcome)
  }
}

#[cfg(test)]
mod tests {
  use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    str::FromStr,
    sync::Mutex,
    task::{Context, Poll, Waker},
  };

  use async_trait::async_trait;
  use octacity_server_store::{ExpiredLeaseClaim, MutationDisposition, RecoverExpiredLeaseOutcome};

  use super::*;

  struct FakeStore {
    claims: Mutex<Vec<ExpiredLeaseClaim>>,
  }

  #[async_trait]
  impl LeaseRecoveryStore for FakeStore {
    async fn claim_expired_leases(&self, request: ClaimExpiredLeases) -> Result<Vec<ExpiredLeaseClaim>, StoreError> {
      let mut claims = self.claims.lock().map_err(|_| StoreError::Unavailable)?;
      let take = claims.len().min(usize::from(request.limit.get()));
      Ok(claims.drain(..take).collect())
    }

    async fn recover_expired_lease(
      &self,
      request: RecoverExpiredLease,
    ) -> Result<RecoverExpiredLeaseOutcome, StoreError> {
      let ordinal = request.claim.lease_id.to_string().bytes().last().unwrap_or_default();
      let action = if ordinal.is_multiple_of(2) {
        LeaseRecoveryAction::Requeued
      } else {
        LeaseRecoveryAction::Failed
      };
      Ok(RecoverExpiredLeaseOutcome {
        disposition: MutationDisposition::Applied,
        lease_id: request.claim.lease_id,
        job_id: request.claim.job_id,
        action,
        infrastructure_requeues: u16::from(action == LeaseRecoveryAction::Requeued),
      })
    }
  }

  #[test]
  fn one_pass_is_bounded_and_recovers_only_durable_claims() {
    run_ready(async {
      let owner = WorkerOwner::new("application-worker").unwrap();
      let claims = (1..=3)
        .map(|value| ExpiredLeaseClaim {
          lease_id: id(value),
          job_id: id(value + 100),
          owner: owner.clone(),
          claim_expires_at: time(200),
        })
        .collect();
      let store = Arc::new(FakeStore {
        claims: Mutex::new(claims),
      });
      let worker = LeaseExpiryWorker::new(store.clone(), owner, 2).unwrap();

      let first = worker.run_once(time(100), time(200)).await.unwrap();
      assert_eq!(first.claimed, 2);
      assert_eq!(u32::from(first.requeued) + u32::from(first.failed), 2);
      assert_eq!(store.claims.lock().unwrap().len(), 1);
    });
  }

  fn run_ready<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = Box::pin(future);
    match Pin::as_mut(&mut future).poll(&mut context) {
      Poll::Ready(output) => output,
      Poll::Pending => panic!("in-memory worker future unexpectedly yielded"),
    }
  }

  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }

  fn id<T>(value: u64) -> T
  where
    T: FromStr,
    T::Err: Debug,
  {
    format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
  }
}
