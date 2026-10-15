-- Private dispatch intents reserve unknown external usage before network I/O.
CREATE TABLE factory_decision_signal_requests (
  request_id UUID PRIMARY KEY,
  run_id UUID NOT NULL,
  node_attempt_id UUID NOT NULL,
  input_digest BYTEA NOT NULL CHECK (octet_length(input_digest)=32),
  request_digest BYTEA NOT NULL CHECK (octet_length(request_digest)=32),
  document BYTEA NOT NULL CHECK (octet_length(document) BETWEEN 2 AND 4194304),
  reserved_at_millis BIGINT NOT NULL CHECK (reserved_at_millis>=0),
  FOREIGN KEY (node_attempt_id,run_id) REFERENCES factory_node_attempts(id,run_id),
  FOREIGN KEY (run_id,input_digest) REFERENCES factory_flow_inputs(run_id,id)
);
CREATE INDEX factory_decision_signal_requests_node ON factory_decision_signal_requests(node_attempt_id,run_id);
