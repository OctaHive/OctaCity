-- Source mode is retained in the existing immutable input snapshot.
-- No historical Build, Trigger, Attempt or Job is rewritten.
ALTER TABLE builds
  ALTER COLUMN repository_id DROP NOT NULL,
  ALTER COLUMN repository_version DROP NOT NULL,
  ALTER COLUMN immutable_revision DROP NOT NULL;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid = 'builds'::regclass AND conname = 'builds_source_form'
  ) THEN
    ALTER TABLE builds ADD CONSTRAINT builds_source_form CHECK ((
      (repository_id IS NOT NULL AND repository_version IS NOT NULL AND immutable_revision IS NOT NULL
        AND input_snapshot#>>'{source,kind}' IS DISTINCT FROM 'source_free_v1')
      OR
      (repository_id IS NULL AND repository_version IS NULL AND immutable_revision IS NULL
        AND input_snapshot->'source' = '{"kind":"source_free_v1"}'::jsonb)
    ) IS TRUE);
  END IF;
END $$;
