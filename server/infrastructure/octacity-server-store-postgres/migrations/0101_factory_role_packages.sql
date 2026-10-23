-- Private compilation snapshots share the persisted node operation and Run lifetime.
CREATE TABLE factory_role_execution_packages (
  operation_id bytea PRIMARY KEY CHECK (octet_length(operation_id)=32),
  run_id uuid NOT NULL REFERENCES factory_runs(id),
  role_id uuid NOT NULL,
  role_version bigint NOT NULL CHECK (role_version>0),
  profile_id uuid NOT NULL,
  profile_version bigint NOT NULL CHECK (profile_version>0),
  FOREIGN KEY (role_id,role_version) REFERENCES factory_role_versions(id,version),
  FOREIGN KEY (profile_id,profile_version) REFERENCES factory_role_profile_versions(id,version),
  document jsonb NOT NULL CHECK (jsonb_typeof(document)='object' AND octet_length(document::text)<=2097152),
  digest bytea NOT NULL CHECK (octet_length(digest)=32),
  FOREIGN KEY (run_id,operation_id) REFERENCES factory_flow_build_intents(run_id,operation_id)
);
CREATE INDEX factory_role_execution_packages_role ON factory_role_execution_packages(role_id);
CREATE INDEX factory_role_execution_packages_run ON factory_role_execution_packages(run_id);
