-- Project-scoped current catalogs and owned-role profile selection.
CREATE INDEX factory_roles_project_discovery_idx ON factory_roles (project_id, id);
CREATE INDEX factory_role_profiles_project_discovery_idx ON factory_role_profiles (project_id, id);
CREATE INDEX factory_role_profile_versions_role_idx ON factory_role_profile_versions (role_id, id, version);
