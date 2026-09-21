ALTER TABLE jobs
  ADD COLUMN infrastructure_requeues BIGINT NOT NULL DEFAULT 0,
  ADD CONSTRAINT jobs_infrastructure_requeues_nonnegative CHECK (infrastructure_requeues >= 0);

ALTER TABLE worker_claims
  ADD COLUMN completed_at TIMESTAMPTZ;

CREATE INDEX leases_expiry_work_idx
  ON leases (expires_at, id)
  WHERE state IN ('active', 'cancellation_requested', 'drain_requested');

CREATE INDEX worker_claims_reclaim_idx
  ON worker_claims (work_kind, expires_at, work_identity);
