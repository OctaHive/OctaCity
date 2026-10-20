-- Reconciled managed values share the existing freshness record, not another observation journal.
-- Allow the supported 64 labels of up to 256 UTF-8 bytes, JSON escaping and original receipt metadata.
ALTER TABLE issue_source_refresh DROP CONSTRAINT issue_source_refresh_document_check;
ALTER TABLE issue_source_refresh ADD CONSTRAINT issue_source_refresh_document_check
  CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 65536);
