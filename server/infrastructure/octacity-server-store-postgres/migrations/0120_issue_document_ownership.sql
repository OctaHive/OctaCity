-- Ownership is projected from immutable journal metadata, never another editable source.
-- Existing Project/Work publications keep NULL and retain their original documents and bytes.
ALTER TABLE project_text_preparations
  ADD COLUMN issue_id UUID GENERATED ALWAYS AS ((document->'intent'->'issue'->>'issue_id')::uuid) STORED,
  ADD CONSTRAINT project_text_issue_owner_fk FOREIGN KEY (project_id, issue_id)
    REFERENCES issues(project_id, id);

CREATE INDEX project_text_issue_owner ON project_text_preparations(project_id, issue_id, id)
  WHERE issue_id IS NOT NULL;
