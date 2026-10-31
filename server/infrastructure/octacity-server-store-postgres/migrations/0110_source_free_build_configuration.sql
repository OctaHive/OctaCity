-- Explicit source absence stays on the existing immutable Configuration owner.
-- Historical repository-backed versions and foreign keys retain their meaning.
ALTER TABLE build_configuration_versions
  ALTER COLUMN repository_id DROP NOT NULL,
  ALTER COLUMN repository_version DROP NOT NULL;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid = 'build_configuration_versions'::regclass
      AND conname = 'build_configuration_versions_source_form'
  ) THEN
    ALTER TABLE build_configuration_versions
      ADD CONSTRAINT build_configuration_versions_source_form CHECK ((
        (repository_id IS NOT NULL AND repository_version IS NOT NULL
          AND configuration_snapshot->'source' IS NULL)
        OR
        (repository_id IS NULL AND repository_version IS NULL
          AND configuration_snapshot->'source' = '{"kind":"source_free_v1"}'::jsonb)
      ) IS TRUE);
  END IF;
END $$;
