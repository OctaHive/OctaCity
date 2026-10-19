-- Bodies have a separate lifetime and are never read or rewritten with metadata.
CREATE TABLE project_text_documents (
  preparation_id BYTEA PRIMARY KEY REFERENCES project_text_preparations(id),
  documents JSONB NOT NULL CHECK (jsonb_typeof(documents) = 'object'
    AND octet_length(documents::text) <= 8388608)
);
INSERT INTO project_text_documents(preparation_id,documents)
  SELECT p.id,p.document->'submission'->'documents' FROM project_text_preparations p
  WHERE EXISTS (SELECT 1 FROM project_text_input_objects o
    WHERE o.preparation_id=p.id AND o.deleted_at_millis IS NULL);
UPDATE project_text_preparations SET document=(document-'submission') ||
  jsonb_build_object('intent',(document->'submission')-'documents');
