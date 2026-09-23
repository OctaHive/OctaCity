-- Unmanaged webhook configuration is immutable and is committed with the
-- external Trigger that consumes its authenticated normalized deliveries.

CREATE TABLE webhook_integrations (
  id UUID PRIMARY KEY,
  trigger_id UUID NOT NULL,
  trigger_version BIGINT NOT NULL,
  definition JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  UNIQUE (trigger_id, trigger_version),
  FOREIGN KEY (trigger_id, trigger_version) REFERENCES triggers(id, version)
);
