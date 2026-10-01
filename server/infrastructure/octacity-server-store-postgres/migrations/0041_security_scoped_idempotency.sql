-- Isolate replayable management outcomes without changing the caller-selected
-- key or its business-intent fingerprint. The default is deliberately retained
-- for the immediately preceding binary during a single-scope rolling upgrade.
ALTER TABLE idempotency_records
  ADD COLUMN security_scope TEXT;

UPDATE idempotency_records AS record
SET security_scope = CASE
  WHEN record.scope IN (
    'claim-ready-job', 'renew-lease', 'append-job-events', 'complete-job', 'register-agent'
  ) THEN 'legacy-agent-data-plane'
  WHEN record.scope IN (
    'complete-managed-webhook-create', 'observe-managed-webhook',
    'rotate-managed-webhook', 'delete-managed-webhook'
  ) THEN 'legacy-adapter-data-plane'
  WHEN record.scope = 'recover-expired-lease' THEN 'legacy-worker-data-plane'
  WHEN record.scope IN ('accept-trigger', 'suppress-trigger')
    AND NOT EXISTS (
      SELECT 1
      FROM trigger_occurrences AS occurrence
      WHERE occurrence.id::text = record.idempotency_key
        AND occurrence.kind = 'manual'
    ) THEN 'legacy-trigger-data-plane'
  ELSE 'trusted-network'
END;

ALTER TABLE idempotency_records
  ALTER COLUMN security_scope SET DEFAULT 'trusted-network',
  ALTER COLUMN security_scope SET NOT NULL,
  ADD CONSTRAINT idempotency_records_security_scope_valid CHECK (
    octet_length(security_scope) BETWEEN 1 AND 128
    AND security_scope ~ '^[a-z0-9][a-z0-9._:-]*$'
  );

ALTER TABLE idempotency_records
  DROP CONSTRAINT idempotency_records_pkey,
  ADD CONSTRAINT idempotency_records_pkey PRIMARY KEY (scope, security_scope, idempotency_key);

-- Preserve the preceding binary's two-column lookup shape during the rollout.
-- It remains non-unique because the new authoritative identity includes scope.
CREATE INDEX idempotency_records_legacy_lookup_idx
  ON idempotency_records (scope, idempotency_key);

-- Trigger deduplication is another replay index. Keep the caller's identity
-- unchanged and partition the authoritative index by the same protocol scope.
ALTER TABLE trigger_occurrences
  ADD COLUMN security_scope TEXT;

UPDATE trigger_occurrences
SET security_scope = CASE
  WHEN kind = 'manual' THEN 'trusted-network'
  ELSE 'legacy-trigger-data-plane'
END;

ALTER TABLE trigger_occurrences
  ALTER COLUMN security_scope SET DEFAULT 'trusted-network',
  ALTER COLUMN security_scope SET NOT NULL,
  ADD CONSTRAINT trigger_occurrences_security_scope_valid CHECK (
    octet_length(security_scope) BETWEEN 1 AND 128
    AND security_scope ~ '^[a-z0-9][a-z0-9._:-]*$'
  ),
  DROP CONSTRAINT trigger_occurrences_deduplication_key,
  ADD CONSTRAINT trigger_occurrences_deduplication_key
    UNIQUE (security_scope, trigger_id, trigger_version, deduplication_identity);

CREATE INDEX trigger_occurrences_legacy_lookup_idx
  ON trigger_occurrences (trigger_id, trigger_version, deduplication_identity);
