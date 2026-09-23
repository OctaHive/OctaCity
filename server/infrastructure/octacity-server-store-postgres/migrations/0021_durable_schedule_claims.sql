ALTER TABLE schedules
  ADD CONSTRAINT schedules_claim_pair CHECK (
    (claim_owner IS NULL AND claim_expires_at IS NULL)
    OR (claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
  ),
  ADD CONSTRAINT schedules_claim_owner_nonempty CHECK (
    claim_owner IS NULL OR (octet_length(claim_owner) BETWEEN 1 AND 128)
  );

CREATE INDEX schedules_due_work_idx
  ON schedules (next_occurrence_at, trigger_id, trigger_version);
