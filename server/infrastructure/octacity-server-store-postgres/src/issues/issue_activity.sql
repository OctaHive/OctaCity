-- Only safe scalar metadata is projected; requirement/comment bodies are never selected.
WITH rows AS (
  SELECT 0 AS kind,(document->>'updated_at')::bigint AS at,version::text AS identity,
    jsonb_build_object('kind','revision','at',(document->>'updated_at')::bigint,'version',version) AS fact
  FROM issue_revisions WHERE project_id=$1 AND issue_id=$2 AND version<=$3
  UNION ALL
  SELECT 1,(document->>'occurred_at')::bigint,version::text,
    jsonb_build_object('kind','status','at',(document->>'occurred_at')::bigint,'version',version,'from',document->'from','to',document->'to','actor',document->'actor')
  FROM issue_status_history WHERE project_id=$1 AND issue_id=$2 AND version<=$3
  UNION ALL
  SELECT 2,(document->>'requested_at')::bigint,id::text,
    jsonb_build_object('kind','execution','at',(document->>'requested_at')::bigint,'request_id',id,'requirements_version',document->'intent'->'expected_version')
  FROM issue_execution_requests WHERE project_id=$1 AND issue_id=$2 AND sequence<=$4
  UNION ALL
  SELECT 3,(metadata->>'created_at')::bigint,id::text,
    jsonb_build_object('kind','discussion','comment',metadata)
  FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND sequence<=$5
)
SELECT fact FROM rows
WHERE kind=ANY($6::int[]) AND ($7::bigint IS NULL OR (at,kind,identity)>($7,$8,$9))
ORDER BY at,kind,identity LIMIT $10
