-- Schedule late-write reconciliation separately from first private-data expiry.
ALTER TABLE factory_source_materializations
  ADD COLUMN next_cleanup_at_millis BIGINT CHECK (next_cleanup_at_millis >= 0);

-- Existing cleaned byte reservations get one reconciliation to initialize their schedule.
UPDATE factory_source_materializations
SET next_cleanup_at_millis = expires_at_millis
WHERE document->>'cleaned_at' IS NOT NULL
  AND jsonb_array_length(document->'objects') > 0;

CREATE INDEX factory_source_cleanup_due
  ON factory_source_materializations(next_cleanup_at_millis, preparation_id)
  WHERE next_cleanup_at_millis IS NOT NULL;
CREATE INDEX factory_source_snapshots_pending_expiry
  ON factory_source_preparations(snapshot_expires_at_millis, id)
  WHERE NOT snapshot_cleaned;
