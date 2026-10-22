-- One current bounded observation, rather than a parallel telemetry history.
ALTER TABLE agents ADD COLUMN resources JSONB
  CHECK (resources IS NULL OR (jsonb_typeof(resources) = 'object' AND octet_length(resources::text) <= 4096));
