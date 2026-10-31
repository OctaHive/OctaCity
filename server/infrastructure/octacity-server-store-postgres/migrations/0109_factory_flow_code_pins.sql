-- Keep exact admitted resources with the existing Run owner. Historical runs
-- have no explicit source catalogue and retain their original NULL form.
ALTER TABLE factory_runs ADD COLUMN IF NOT EXISTS flow_code_pins jsonb;
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conrelid = 'factory_runs'::regclass
      AND conname = 'factory_runs_flow_code_pins_shape'
  ) THEN
    ALTER TABLE factory_runs ADD CONSTRAINT factory_runs_flow_code_pins_shape
      CHECK (flow_code_pins IS NULL OR jsonb_typeof(flow_code_pins) = 'array');
  END IF;
END $$;
