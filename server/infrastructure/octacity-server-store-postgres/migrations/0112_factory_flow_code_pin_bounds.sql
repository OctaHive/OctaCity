-- Preserve historical NULL pins and existing exact references. The Flow's
-- 16 MiB publication ceiling also bounds retained pins; all 64 definitions'
-- 64 sources with bounded keys/revisions fit below it. Do not rewrite 0109.
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conrelid = 'factory_runs'::regclass
      AND conname = 'factory_runs_flow_code_pins_bounded'
  ) THEN
    ALTER TABLE factory_runs ADD CONSTRAINT factory_runs_flow_code_pins_bounded
      CHECK (flow_code_pins IS NULL OR (
        jsonb_typeof(flow_code_pins) = 'array'
        AND octet_length(flow_code_pins::text) <= 16777216
      ));
  END IF;
END $$;
