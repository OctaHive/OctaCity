-- Freeze reviewed access with accepted execution, without consulting mutable defaults
-- during recovery. NULL preserves the original authority of historical Runs.
ALTER TABLE factory_runs ADD COLUMN flow_access_profile JSONB;
ALTER TABLE factory_runs ADD CONSTRAINT factory_runs_access_profile_shape CHECK (
  flow_access_profile IS NULL OR (
    jsonb_typeof(flow_access_profile) = 'object'
    AND octet_length(flow_access_profile::text) <= 524288
  )
);
