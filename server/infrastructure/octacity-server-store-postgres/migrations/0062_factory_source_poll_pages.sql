-- A retained normalized page is capped at 4 MiB by the shared store contract.
-- jsonb::text adds whitespace, so allow twice that payload plus the old 32 KiB envelope.
ALTER TABLE factory_source_scans DROP CONSTRAINT factory_source_scans_document_check;
ALTER TABLE factory_source_scans ADD CONSTRAINT factory_source_scans_document_check
  CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) BETWEEN 2 AND 8421376);
CREATE INDEX factory_source_poll_expiry_idx ON factory_source_scans
  (((document#>>'{progress,polling,pending_page,expires_at}')::bigint), kind, id)
  WHERE document#>>'{progress,polling,pending_page,expires_at}' IS NOT NULL;
