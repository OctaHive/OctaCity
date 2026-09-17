-- Allocate every Project's durable log-index work positions without gaps.
-- The counter update and work insert share one transaction, so a rejected or
-- rolled-back work item cannot advance the authoritative watermark.

CREATE TABLE log_index_project_positions (
  project_id UUID PRIMARY KEY REFERENCES projects(id),
  committed_through BIGINT NOT NULL DEFAULT 0,
  CONSTRAINT log_index_project_positions_non_negative CHECK (committed_through >= 0)
);

DO $$
BEGIN
  IF EXISTS (
    SELECT project_id
      FROM log_indexing_work
      GROUP BY project_id
      HAVING MIN(position) <> 1 OR MAX(position) <> COUNT(*)
  ) THEN
    RAISE EXCEPTION 'existing log-index work contains non-contiguous project positions';
  END IF;
END;
$$;

INSERT INTO log_index_project_positions (project_id, committed_through)
SELECT project_id, MAX(position)
  FROM log_indexing_work
  GROUP BY project_id;

CREATE FUNCTION octacity_require_next_log_index_position()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
  required_position BIGINT;
BEGIN
  INSERT INTO log_index_project_positions (project_id, committed_through)
  VALUES (NEW.project_id, 0)
  ON CONFLICT (project_id) DO NOTHING;

  SELECT committed_through + 1
    INTO required_position
    FROM log_index_project_positions
    WHERE project_id = NEW.project_id
    FOR UPDATE;

  -- A NULL input asks the database to allocate the next position. Explicit
  -- values remain useful for deterministic imports and are accepted only when
  -- they are exactly the next position.
  IF NEW.position IS NULL THEN
    NEW.position := required_position;
  ELSIF NEW.position > 0 AND NEW.position <> required_position THEN
    RAISE EXCEPTION 'log-index position must be the next project position'
      USING ERRCODE = '23514', CONSTRAINT = 'log_indexing_work_contiguous_position';
  END IF;

  IF NEW.position > 0 THEN
    UPDATE log_index_project_positions
      SET committed_through = NEW.position
      WHERE project_id = NEW.project_id;
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER log_indexing_work_require_next_position
BEFORE INSERT ON log_indexing_work
FOR EACH ROW
EXECUTE FUNCTION octacity_require_next_log_index_position();

CREATE FUNCTION octacity_preserve_log_index_position_identity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  IF NEW.project_id IS DISTINCT FROM OLD.project_id
    OR NEW.position IS DISTINCT FROM OLD.position THEN
    RAISE EXCEPTION 'log-index work position identity is immutable'
      USING ERRCODE = '55000';
  END IF;
  RETURN NEW;
END;
$$;

CREATE TRIGGER log_indexing_work_preserve_position_identity
BEFORE UPDATE ON log_indexing_work
FOR EACH ROW
EXECUTE FUNCTION octacity_preserve_log_index_position_identity();
