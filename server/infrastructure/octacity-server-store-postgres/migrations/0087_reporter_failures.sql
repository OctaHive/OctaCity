-- Classified safe rejection facts reuse the existing append-only delivery journal.
ALTER TABLE factory_outbox_records ADD COLUMN failure JSONB;
ALTER TABLE factory_outbox_records ADD CONSTRAINT factory_outbox_failure CHECK (
  failure IS NULL OR (state='failed' AND jsonb_typeof(failure)='string'
    AND octet_length(failure::text)<=16
    AND failure IN ('"conflict"'::jsonb, '"unsupported"'::jsonb, '"exhausted"'::jsonb, '"forbidden"'::jsonb))
);

-- Private origin of installed writer authority; public mapping documents expose no connection handle.
ALTER TABLE issue_source_mappings ADD COLUMN writer_access JSONB;
ALTER TABLE issue_source_mappings ADD CONSTRAINT issue_source_writer_access CHECK (
  writer_access IS NULL OR (jsonb_typeof(writer_access)='object'
    AND writer_access ?& ARRAY['connection_id','connection_version','configuration']
    AND octet_length(writer_access::text)<=1024)
);
