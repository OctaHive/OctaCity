-- Execution-neutral consumers share the Source scan model, with separately scoped bounded private progress.
CREATE TABLE issue_source_reads (
  project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  security_scope TEXT NOT NULL CHECK(octet_length(security_scope) BETWEEN 1 AND 256),
  target BYTEA NOT NULL CHECK(octet_length(target)=32),
  -- Shared 4 MiB page ceiling plus 64 KiB for claim, cursors and lane metadata.
  document BYTEA NOT NULL CHECK(octet_length(document) BETWEEN 1 AND 4259840),
  PRIMARY KEY(project_id,security_scope,target)
);
