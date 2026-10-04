WITH candidates AS (
  SELECT 0::integer AS kind_order, 'project'::text AS kind, project.id, project.name AS label,
         NULL::text AS context, octacity_normalize_resource_search_text(project.name) AS normalized_label,
         project.id = $7::uuid AS exact_identifier
  FROM projects AS project
  WHERE $2 AND ($6 OR project.id = ANY($8::uuid[]))
    AND (($7::uuid IS NOT NULL AND project.id = $7)
      OR octacity_normalize_resource_search_text(project.name) LIKE '%' || $1 || '%')
  UNION ALL
  SELECT 1, 'build', build.id, configuration.name, project.name,
         octacity_normalize_resource_search_text(configuration.name), build.id = $7::uuid
  FROM builds AS build
  JOIN build_configurations AS configuration ON configuration.id = build.build_configuration_id
  JOIN projects AS project ON project.id = build.project_id
  WHERE $3 AND build.metadata_visible AND ($6 OR build.id = ANY($9::uuid[]))
    AND (($7::uuid IS NOT NULL AND build.id = $7)
      OR octacity_normalize_resource_search_text(configuration.name) LIKE '%' || $1 || '%')
  UNION ALL
  SELECT 2, 'agent', agent.id, agent.name, pool.name,
         octacity_normalize_resource_search_text(agent.name), agent.id = $7::uuid
  FROM agents AS agent
  JOIN pools AS pool ON pool.id = agent.pool_id AND pool.version = agent.pool_version
  WHERE $4 AND ($6 OR agent.id = ANY($10::uuid[]))
    AND (($7::uuid IS NOT NULL AND agent.id = $7)
      OR octacity_normalize_resource_search_text(agent.name) LIKE '%' || $1 || '%')
  UNION ALL
  SELECT 3, 'agent_pool', pool.id, pool.name, NULL::text,
         octacity_normalize_resource_search_text(pool.name), pool.id = $7::uuid
  FROM pools AS pool
  WHERE $5 AND ($6 OR pool.id = ANY($11::uuid[]))
    AND NOT EXISTS (
      SELECT 1 FROM pools AS newer WHERE newer.id = pool.id AND newer.version > pool.version
    )
    AND (($7::uuid IS NOT NULL AND pool.id = $7)
      OR octacity_normalize_resource_search_text(pool.name) LIKE '%' || $1 || '%')
), ranked AS (
  SELECT kind_order, kind, id, label, context, normalized_label,
         CASE WHEN exact_identifier THEN 0
              WHEN normalized_label LIKE $1 || '%' THEN 1 ELSE 2 END AS match_rank
  FROM candidates
)
SELECT kind, id, label, context, normalized_label, match_rank
FROM ranked
WHERE $12::integer IS NULL OR (match_rank, kind_order, normalized_label, id) >
  ($12, $13::integer, $14::text, $15::uuid)
ORDER BY match_rank, kind_order, normalized_label, id
LIMIT $16
