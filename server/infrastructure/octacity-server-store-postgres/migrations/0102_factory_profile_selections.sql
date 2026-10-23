-- Pre-dispatch selections extend the common fenced Flow data journal.
-- Ordinary bounded metadata cleanup removes selections before their frozen inputs.
CREATE TABLE factory_flow_profile_selections (
  run_id UUID NOT NULL,
  id BYTEA NOT NULL CHECK (octet_length(id) = 32),
  input_digest BYTEA NOT NULL CHECK (octet_length(input_digest) = 32),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 1048576),
  PRIMARY KEY (run_id, id),
  FOREIGN KEY (run_id, input_digest) REFERENCES factory_flow_inputs(run_id, id)
);
CREATE INDEX factory_flow_profile_selections_input ON factory_flow_profile_selections(run_id, input_digest);
