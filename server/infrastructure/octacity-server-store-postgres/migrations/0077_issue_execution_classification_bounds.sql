-- Reviewed classification reuses ordinary Work metadata limits (8 KiB of text).
-- JSON escaping plus the frozen target/digests can exceed the former 16 KiB
-- request cap. Keep the metadata-only envelope bounded without rejecting valid
-- ordinary classifications; no private task bodies are stored in this document.
ALTER TABLE issue_execution_requests
  DROP CONSTRAINT issue_execution_requests_document_check,
  ADD CONSTRAINT issue_execution_requests_document_check
    CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 65536);
