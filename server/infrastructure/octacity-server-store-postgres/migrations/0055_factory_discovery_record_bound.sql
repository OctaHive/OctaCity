-- Discovery facts are an exact index of the bounded canonical result document.
-- Its existing BYTEA ceiling (MAX_FLOW_DATA_BYTES = 1048576) is shared with
-- FlowNodeRecord construction/restoration. A second limit on JSONB's expanded
-- text rejects valid escaped revisions and complete bounded provenance.
ALTER TABLE factory_flow_records
  DROP CONSTRAINT factory_flow_records_discovery_matches_document;
ALTER TABLE factory_flow_records
  ADD CONSTRAINT factory_flow_records_discovery_matches_document CHECK (
    discovery_facts IS NOT DISTINCT FROM (convert_from(document, 'UTF8')::jsonb -> 'discovery')
    AND (discovery_facts IS NULL OR jsonb_typeof(discovery_facts) = 'object')
    AND observed_at_millis = (convert_from(document, 'UTF8')::jsonb ->> 'observed_at')::bigint
  );
