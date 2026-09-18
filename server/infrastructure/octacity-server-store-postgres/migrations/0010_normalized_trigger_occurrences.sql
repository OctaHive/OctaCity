-- Durable shape for provider-neutral Trigger normalization and causality.

ALTER TABLE trigger_occurrences
  ADD COLUMN build_configuration_id UUID,
  ADD COLUMN build_configuration_version BIGINT,
  ADD COLUMN kind TEXT,
  ADD COLUMN causality JSONB,
  ADD COLUMN provider_metadata JSONB;

UPDATE trigger_occurrences AS occurrence
  SET build_configuration_id = trigger.build_configuration_id,
      build_configuration_version = trigger.build_configuration_version,
      kind = trigger.kind,
      causality = jsonb_build_object(
        'root_occurrence_id', occurrence.id,
        'parent_occurrence_id', NULL,
        'depth', 0
      ),
      provider_metadata = '{}'::JSONB
  FROM triggers AS trigger
  WHERE trigger.id = occurrence.trigger_id AND trigger.version = occurrence.trigger_version;

ALTER TABLE trigger_occurrences
  ALTER COLUMN build_configuration_id SET NOT NULL,
  ALTER COLUMN build_configuration_version SET NOT NULL,
  ALTER COLUMN kind SET NOT NULL,
  ALTER COLUMN causality SET NOT NULL,
  ALTER COLUMN provider_metadata SET NOT NULL,
  ADD CONSTRAINT trigger_occurrences_configuration_fkey
    FOREIGN KEY (build_configuration_id, build_configuration_version)
    REFERENCES build_configuration_versions(build_configuration_id, version);

ALTER TABLE triggers
  ADD CONSTRAINT triggers_kind
    CHECK (kind IN ('manual', 'scheduled', 'external', 'internal'));

ALTER TABLE trigger_occurrences
  ADD CONSTRAINT trigger_occurrences_kind
    CHECK (kind IN ('manual', 'scheduled', 'external', 'internal')),
  ADD CONSTRAINT trigger_occurrences_cause_object
    CHECK (jsonb_typeof(cause) = 'object'),
  ADD CONSTRAINT trigger_occurrences_cause_kind
    CHECK (cause ->> 'kind' = kind),
  ADD CONSTRAINT trigger_occurrences_causality_object
    CHECK (jsonb_typeof(causality) = 'object'),
  ADD CONSTRAINT trigger_occurrences_provider_metadata_object
    CHECK (jsonb_typeof(provider_metadata) = 'object');
