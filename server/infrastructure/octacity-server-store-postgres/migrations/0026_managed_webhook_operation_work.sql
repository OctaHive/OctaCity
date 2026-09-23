-- Managed provider calls are retained before execution. Stable idempotency
-- keys make process crashes, lost responses, and replica retries converge on
-- one remote operation.

CREATE TABLE managed_webhook_operations (
  integration_id UUID NOT NULL REFERENCES webhook_integrations(id),
  operation TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  requested_at TIMESTAMPTZ NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending',
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TIMESTAMPTZ,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  failure_code TEXT,
  diagnostic TEXT,
  completed_at TIMESTAMPTZ,
  PRIMARY KEY (integration_id, operation, idempotency_key),
  CONSTRAINT managed_webhook_operations_operation_check
    CHECK (operation IN ('create', 'observe', 'rotate', 'delete')),
  CONSTRAINT managed_webhook_operations_state_check
    CHECK (state IN ('pending', 'retry_scheduled', 'completed', 'dead_letter')),
  CONSTRAINT managed_webhook_operations_attempts_check CHECK (attempts >= 0),
  CONSTRAINT managed_webhook_operations_next_attempt_check CHECK (
    (state IN ('pending', 'retry_scheduled') AND next_attempt_at IS NOT NULL)
    OR (state IN ('completed', 'dead_letter') AND next_attempt_at IS NULL)
  ),
  CONSTRAINT managed_webhook_operations_claim_check CHECK (
    (claim_owner IS NULL AND claim_expires_at IS NULL)
    OR (claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
  )
);

CREATE UNIQUE INDEX managed_webhook_operations_one_active_idx
  ON managed_webhook_operations (integration_id)
  WHERE state IN ('pending', 'retry_scheduled');

CREATE INDEX managed_webhook_operations_due_idx
  ON managed_webhook_operations (next_attempt_at, requested_at, integration_id)
  WHERE state IN ('pending', 'retry_scheduled');
