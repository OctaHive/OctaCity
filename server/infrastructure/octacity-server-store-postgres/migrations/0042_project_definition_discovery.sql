-- Project-scoped definition discovery filters by ownership and walks stable
-- identities in ascending order. Immutable version primary keys already serve
-- each lateral current-version lookup.
CREATE INDEX pipelines_project_discovery_idx
  ON pipelines (project_id, id);

CREATE INDEX repositories_project_discovery_idx
  ON repositories (project_id, id);

CREATE INDEX build_configurations_project_discovery_idx
  ON build_configurations (project_id, id);

-- Trigger discovery needs no additional index: triggers_pkey supplies stable
-- identity/version order and the Build Configuration index above proves
-- Project ownership. Adding a second Trigger index would duplicate that path.
