-- A Work has one original Run and belongs to at most one same-Project Issue.
ALTER TABLE factory_work_envelopes ADD CONSTRAINT factory_work_project_identity UNIQUE(project_id,id);
ALTER TABLE factory_runs ADD CONSTRAINT factory_run_work_identity UNIQUE(work_envelope_id,id);
CREATE TABLE issue_work_associations (
  work_id UUID PRIMARY KEY,
  run_id UUID NOT NULL UNIQUE,
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL,
  request_id UUID UNIQUE,
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 16384),
  FOREIGN KEY (project_id,issue_id) REFERENCES issues(project_id,id),
  FOREIGN KEY (project_id,work_id) REFERENCES factory_work_envelopes(project_id,id),
  FOREIGN KEY (work_id,run_id) REFERENCES factory_runs(work_envelope_id,id),
  FOREIGN KEY (project_id,issue_id,request_id) REFERENCES issue_execution_requests(project_id,issue_id,id),
  CHECK (document->>'project_id'=project_id::text AND document->>'issue_id'=issue_id::text
    AND document->>'work_id'=work_id::text AND document->>'run_id'=run_id::text)
);
CREATE INDEX issue_work_associations_issue ON issue_work_associations(project_id,issue_id,work_id);
