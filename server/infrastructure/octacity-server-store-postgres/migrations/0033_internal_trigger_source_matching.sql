-- Bound source-scoped internal Trigger matching by immutable upstream target,
-- terminal outcome, publication time, and stable Trigger lineage.
CREATE INDEX triggers_internal_source_match_idx
  ON triggers (
    (definition #>> '{upstream,configuration_id}'),
    (definition #>> '{upstream,configuration_version}'),
    (definition ->> 'event_kind'),
    created_at,
    id,
    version DESC
  )
  WHERE kind = 'internal';
