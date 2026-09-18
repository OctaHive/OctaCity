-- Declarative storage shape for append-only Pipeline publication.

ALTER TABLE pipelines
  ADD CONSTRAINT pipelines_project_name_key UNIQUE (project_id, name);

ALTER TABLE pipeline_versions
  ADD CONSTRAINT pipeline_versions_dag_object CHECK (jsonb_typeof(dag_snapshot) = 'object');
