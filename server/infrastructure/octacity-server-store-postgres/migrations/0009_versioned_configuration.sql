-- Declarative storage shape for immutable Repository and Build Configuration publication.

ALTER TABLE repositories
  ADD CONSTRAINT repositories_project_name_key UNIQUE (project_id, name);

ALTER TABLE repository_versions
  ADD CONSTRAINT repository_versions_positive_version CHECK (version > 0),
  ADD CONSTRAINT repository_versions_selection_object CHECK (jsonb_typeof(selection_policy) = 'object');

ALTER TABLE build_configurations
  ADD CONSTRAINT build_configurations_project_name_key UNIQUE (project_id, name);

ALTER TABLE build_configuration_versions
  ADD CONSTRAINT build_configuration_versions_positive_version CHECK (version > 0),
  ADD CONSTRAINT build_configuration_versions_snapshot_object CHECK (jsonb_typeof(configuration_snapshot) = 'object');
