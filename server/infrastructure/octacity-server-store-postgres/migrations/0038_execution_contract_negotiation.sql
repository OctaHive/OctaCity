ALTER TABLE agent_registrations
  ADD COLUMN execution_contract_version SMALLINT NOT NULL DEFAULT 1,
  ADD CONSTRAINT agent_registrations_execution_contract_version_positive
    CHECK (execution_contract_version > 0);
