-- Exact accepted actions may retain the supported 256 record digests plus
-- candidate/action provenance and a bounded reason. Keep history metadata-only
-- and bounded while allowing that inventory; no raw evidence is stored here.
ALTER TABLE issue_status_history
  DROP CONSTRAINT issue_status_history_document_check,
  ADD CONSTRAINT issue_status_history_document_check
    CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 65536);
