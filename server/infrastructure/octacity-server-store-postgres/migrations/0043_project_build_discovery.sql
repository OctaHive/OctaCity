-- Project Build discovery always excludes deleted metadata and walks the
-- newest-first composite cursor. Exact filters retain that order so bounded
-- pages do not sort or inspect unrelated rows.
CREATE INDEX builds_project_discovery_idx
  ON builds (project_id, created_at DESC, id DESC)
  WHERE metadata_visible;

CREATE INDEX builds_project_configuration_discovery_idx
  ON builds (project_id, build_configuration_id, created_at DESC, id DESC)
  WHERE metadata_visible;

CREATE INDEX builds_project_state_discovery_idx
  ON builds (project_id, state, created_at DESC, id DESC)
  WHERE metadata_visible;

-- attempts_build_number_key already supports the lateral current-Attempt
-- lookup through its (build_id, attempt_number) unique index.
