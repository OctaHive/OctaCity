-- Manual Trigger intent is durable before mutable VCS resolution. Claims make
-- synchronous first attempts and background recovery mutually exclusive.

CREATE TABLE trigger_evaluation_work (
  occurrence_id UUID PRIMARY KEY,
  intent_digest BYTEA NOT NULL,
  payload JSONB NOT NULL,
  resolved_revision TEXT,
  state TEXT NOT NULL DEFAULT 'pending',
  attempts INTEGER NOT NULL DEFAULT 1,
  next_attempt_at TIMESTAMPTZ,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  diagnostic TEXT,
  completed_at TIMESTAMPTZ,
  CONSTRAINT trigger_evaluation_work_digest_check CHECK (octet_length(intent_digest) = 32),
  CONSTRAINT trigger_evaluation_work_revision_check CHECK (
    resolved_revision IS NULL OR octet_length(resolved_revision) BETWEEN 1 AND 512
  ),
  CONSTRAINT trigger_evaluation_work_state_check CHECK (state IN ('pending', 'completed', 'dead_letter')),
  CONSTRAINT trigger_evaluation_work_attempts_check CHECK (attempts > 0),
  CONSTRAINT trigger_evaluation_work_schedule_check CHECK (
    (state = 'pending' AND next_attempt_at IS NOT NULL)
    OR (state IN ('completed', 'dead_letter') AND next_attempt_at IS NULL)
  ),
  CONSTRAINT trigger_evaluation_work_claim_check CHECK (
    (claim_owner IS NULL AND claim_expires_at IS NULL)
    OR (claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
  )
);

CREATE INDEX trigger_evaluation_work_due_idx
  ON trigger_evaluation_work (next_attempt_at, occurrence_id)
  WHERE state = 'pending';
