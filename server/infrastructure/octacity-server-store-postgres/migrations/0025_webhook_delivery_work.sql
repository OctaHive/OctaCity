-- Raw webhook receipts are durable before adapter execution. Authenticated
-- events are normalized separately so provider identities can be deduplicated
-- without conflating repeated HTTP receipts.

CREATE TABLE webhook_deliveries (
  id UUID PRIMARY KEY,
  integration_id UUID NOT NULL REFERENCES webhook_integrations(id),
  headers JSONB NOT NULL,
  body BYTEA NOT NULL,
  received_at TIMESTAMPTZ NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending_verification',
  verification_attempts INTEGER NOT NULL DEFAULT 0,
  trigger_attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TIMESTAMPTZ,
  claim_owner TEXT,
  claim_expires_at TIMESTAMPTZ,
  provider_delivery_id TEXT,
  failure_code TEXT,
  diagnostic TEXT,
  completed_at TIMESTAMPTZ,
  CONSTRAINT webhook_deliveries_state_check CHECK (
    state IN ('pending_verification', 'retry_scheduled', 'pending_trigger', 'completed', 'suppressed', 'dead_letter')
  ),
  CONSTRAINT webhook_deliveries_attempts_check CHECK (verification_attempts >= 0 AND trigger_attempts >= 0),
  CONSTRAINT webhook_deliveries_next_attempt_check CHECK (
    (state IN ('pending_verification', 'retry_scheduled', 'pending_trigger') AND next_attempt_at IS NOT NULL)
    OR (state IN ('completed', 'suppressed', 'dead_letter') AND next_attempt_at IS NULL)
  ),
  CONSTRAINT webhook_deliveries_claim_check CHECK (
    (claim_owner IS NULL AND claim_expires_at IS NULL)
    OR (claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
  )
);

CREATE TABLE webhook_normalized_events (
  integration_id UUID NOT NULL REFERENCES webhook_integrations(id),
  provider_delivery_id TEXT NOT NULL,
  canonical_delivery_id UUID NOT NULL UNIQUE REFERENCES webhook_deliveries(id),
  event JSONB NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (integration_id, provider_delivery_id)
);

CREATE INDEX webhook_deliveries_due_idx
  ON webhook_deliveries (next_attempt_at, received_at, id)
  WHERE state IN ('pending_verification', 'retry_scheduled', 'pending_trigger');

CREATE INDEX webhook_deliveries_integration_provider_idx
  ON webhook_deliveries (integration_id, provider_delivery_id)
  WHERE provider_delivery_id IS NOT NULL;
