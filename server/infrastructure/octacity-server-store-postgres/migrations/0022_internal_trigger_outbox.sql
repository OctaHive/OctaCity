-- Bounded replica-safe delivery of terminal Build events to internal Triggers.

CREATE INDEX outbox_internal_trigger_claim_idx
  ON outbox_entries (available_at, id)
  WHERE published_at IS NULL
    AND topic IN ('job.completed', 'build.cancellation-requested');

CREATE INDEX triggers_internal_event_match_idx
  ON triggers (kind, (definition ->> 'event_kind'), created_at, id, version)
  WHERE enabled AND kind = 'internal';
