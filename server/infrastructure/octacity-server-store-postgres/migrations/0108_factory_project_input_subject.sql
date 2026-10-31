-- Preserve repository-backed history while admitting an explicit, versioned
-- Project/input subject. NULL means no VCS resource; no sentinel identities.
ALTER TABLE factory_work_envelopes
  ALTER COLUMN repository_id DROP NOT NULL,
  ALTER COLUMN repository_version DROP NOT NULL,
  ALTER COLUMN exact_revision DROP NOT NULL;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conrelid = 'factory_work_envelopes'::regclass
      AND conname = 'factory_work_envelopes_subject_form'
  ) THEN
    ALTER TABLE factory_work_envelopes
      ADD CONSTRAINT factory_work_envelopes_subject_form CHECK ((
        (repository_id IS NOT NULL AND repository_version IS NOT NULL
          AND exact_revision IS NOT NULL AND NOT (envelope->'subject' ? 'kind'))
        OR
        (repository_id IS NULL AND repository_version IS NULL AND exact_revision IS NULL
          AND envelope#>>'{subject,kind}' = 'project_input_v1'
          AND envelope#>>'{subject,project_id}' = project_id::text
          AND envelope#>>'{subject,input_digest}' ~ '^[0-9a-f]{64}$')
      ) IS TRUE);
  END IF;
END $$;
