-- Typed intake observations remain append-only with their code-owned decisions.
CREATE TABLE factory_triage_records (
  run_id UUID NOT NULL REFERENCES factory_runs(id),
  phase SMALLINT NOT NULL CHECK (phase BETWEEN 0 AND 2),
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  record JSONB NOT NULL CHECK (jsonb_typeof(record) = 'object' AND octet_length(record::text) BETWEEN 2 AND 1048576),
  PRIMARY KEY (run_id, phase),
  UNIQUE (run_id, id)
);
-- Schemas 47-51 are the same unreleased Factory slice; preceding binary schema is 46.
