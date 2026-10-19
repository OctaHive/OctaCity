-- Immutable planning catalogs; version zero is retained only by mutations.
CREATE TABLE issue_property_versions (
  project_id UUID NOT NULL REFERENCES projects(id),
  version BIGINT NOT NULL CHECK (version >= 0),
  definition JSONB NOT NULL CHECK (jsonb_typeof(definition) = 'object' AND octet_length(definition::text) <= 1048576),
  published_at_millis BIGINT NOT NULL CHECK (published_at_millis >= 0),
  PRIMARY KEY (project_id, version)
);

-- Existing revisions keep their original JSON and refer to the empty catalog.
INSERT INTO issue_property_versions(project_id, version, definition, published_at_millis)
  SELECT p.id, 0, '{"labels":[],"properties":[]}'::jsonb,
    FLOOR(EXTRACT(EPOCH FROM p.created_at) * 1000)::bigint
  FROM projects p WHERE EXISTS (SELECT 1 FROM issues i WHERE i.project_id=p.id);
ALTER TABLE issue_revisions
  ADD COLUMN property_version BIGINT GENERATED ALWAYS AS
    (COALESCE((document->'planning'->>'definition_version')::bigint, 0)) STORED,
  ADD COLUMN parent_id UUID GENERATED ALWAYS AS
    ((document->'planning'->>'parent_id')::uuid) STORED,
  ADD CONSTRAINT issue_revisions_property_fk FOREIGN KEY (project_id, property_version)
    REFERENCES issue_property_versions(project_id, version),
  ADD CONSTRAINT issue_revisions_parent_fk FOREIGN KEY (project_id, parent_id)
    REFERENCES issues(project_id, id),
  ADD CONSTRAINT issue_revisions_no_self_parent CHECK (parent_id IS NULL OR parent_id <> issue_id);
CREATE INDEX issue_revisions_parent_idx ON issue_revisions(project_id, parent_id, issue_id, version)
  WHERE parent_id IS NOT NULL;
