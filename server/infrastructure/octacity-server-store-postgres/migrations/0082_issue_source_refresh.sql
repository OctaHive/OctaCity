-- Safe freshness watermarks, independent of execution input discovery and private page bodies.
CREATE TABLE issue_source_refresh (
  project_id UUID NOT NULL,
  issue_id UUID NOT NULL PRIMARY KEY,
  document JSONB NOT NULL CHECK (jsonb_typeof(document) = 'object' AND octet_length(document::text) <= 32768),
  FOREIGN KEY(project_id,issue_id) REFERENCES issues(project_id,id)
);
-- Existing immutable link receipts initialize the watermark without reading a provider or changing an Issue.
INSERT INTO issue_source_refresh(project_id,issue_id,document)
SELECT l.project_id,l.issue_id,jsonb_build_object('enabled',true,'generation',1,'accepted',o.document,'pending',NULL)
FROM issue_source_links l
JOIN LATERAL (
  SELECT document FROM issue_source_observations
  WHERE project_id=l.project_id AND security_scope=l.security_scope AND native_correlation=l.native_correlation
  ORDER BY (document->>'observed_at')::bigint,id LIMIT 1
) o ON true;
