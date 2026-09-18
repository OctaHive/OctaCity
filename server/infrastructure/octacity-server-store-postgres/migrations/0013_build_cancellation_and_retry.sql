-- Durable Build cancellation intent and explicit retry causality. Rust store
-- operations own all state decisions and next-Attempt allocation.

ALTER TABLE attempts
  ADD COLUMN retry_of_attempt_id UUID REFERENCES attempts(id),
  ADD CONSTRAINT attempts_retry_not_self CHECK (retry_of_attempt_id IS NULL OR retry_of_attempt_id <> id),
  ADD CONSTRAINT attempts_retry_source_unique UNIQUE (retry_of_attempt_id);

CREATE TABLE build_cancellations (
  build_id UUID PRIMARY KEY REFERENCES builds(id),
  attempt_id UUID NOT NULL REFERENCES attempts(id),
  cancelled_job_ids UUID[] NOT NULL,
  cancelling_job_ids UUID[] NOT NULL,
  attempt_state TEXT NOT NULL,
  build_state TEXT NOT NULL,
  requested_at TIMESTAMPTZ NOT NULL,
  CONSTRAINT build_cancellations_attempt_state CHECK (attempt_state IN ('running', 'cancelled')),
  CONSTRAINT build_cancellations_build_state CHECK (build_state IN ('running', 'cancelled'))
);
