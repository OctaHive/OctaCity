-- Global resource search matches normalized operator-facing names by prefix
-- and substring. Expression indexes keep normalization single-purpose and
-- avoid duplicating authoritative names in a second table or mutable column.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE OR REPLACE FUNCTION octacity_normalize_resource_search_text(value TEXT)
RETURNS TEXT
LANGUAGE SQL
IMMUTABLE
STRICT
PARALLEL SAFE
RETURN translate(
  regexp_replace(btrim(value, E' \t\n\013\f\r'), E'[ \t\n\013\f\r]+', ' ', 'g'),
  'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  'abcdefghijklmnopqrstuvwxyz'
);

CREATE INDEX projects_resource_search_name_idx
  ON projects USING GIN (octacity_normalize_resource_search_text(name) gin_trgm_ops);

CREATE INDEX build_configurations_resource_search_name_idx
  ON build_configurations USING GIN (octacity_normalize_resource_search_text(name) gin_trgm_ops);

CREATE INDEX agents_resource_search_name_idx
  ON agents USING GIN (octacity_normalize_resource_search_text(name) gin_trgm_ops);

CREATE INDEX pools_resource_search_name_idx
  ON pools USING GIN (octacity_normalize_resource_search_text(name) gin_trgm_ops);

-- Build names come from their Build Configuration. Once that bounded name
-- match resolves, this partial index finds only visible Builds without a
-- cross-Project scan. Exact Build identity continues to use builds_pkey.
CREATE INDEX builds_configuration_resource_search_idx
  ON builds (build_configuration_id, id)
  WHERE metadata_visible;
