-- Fold consecutive equal content/provenance before applying the activity cursor.
-- Exact read receipts remain immutable and independently addressable.
WITH link AS (
  SELECT l.native_correlation,
    COALESCE((r.document->>'generation')::bigint,0) AS generation,
    (SELECT COALESCE(max(version),0) FROM issue_source_mappings
      WHERE project_id=l.project_id AND issue_id=l.issue_id) AS mapping
  FROM issue_source_links l
  LEFT JOIN issue_source_refresh r USING(project_id,issue_id)
  WHERE l.project_id=$1 AND l.issue_id=$2 AND l.security_scope=$3
), observations AS (
  SELECT o.id,o.document,(o.document->>'observed_at')::bigint AS at,
    lag(o.document-'observed_at'-'expires_at') OVER (
      ORDER BY (o.document->>'observed_at')::bigint,o.id
    ) AS previous
  FROM issue_source_observations o JOIN link l USING(native_correlation)
  WHERE o.project_id=$1 AND o.security_scope=$3
), changes AS (
  SELECT id,document,at FROM observations
  WHERE (document-'observed_at'-'expires_at') IS DISTINCT FROM previous
), page AS (
  SELECT id,at,jsonb_build_object('kind','source','at',at,'id',encode(id,'hex'),
    'expires_at',(document->>'expires_at')::bigint) AS fact
  FROM changes
  WHERE $4::bigint IS NULL OR (at,4,encode(id,'hex'))>($4,$5,$6)
  ORDER BY at,id LIMIT $7
)
SELECT (SELECT native_correlation FROM link) AS native,
  sha256(convert_to(COALESCE(
    (SELECT string_agg(encode(id,'hex'),'' ORDER BY at,id) FROM changes),''),'UTF8')) AS membership,
  COALESCE((SELECT generation FROM link),0) AS generation,
  COALESCE((SELECT mapping FROM link),0) AS mapping,
  COALESCE((SELECT jsonb_agg(fact ORDER BY at,id) FROM page),'[]'::jsonb) AS rows
