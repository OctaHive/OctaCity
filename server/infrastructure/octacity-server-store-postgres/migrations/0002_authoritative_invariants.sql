-- Indexed invariants for authoritative coordination state.
--
-- These constraints are deliberately kept in PostgreSQL as the final line of
-- defence when multiple server replicas race or a future adapter contains a
-- bug. Application code is still responsible for returning useful typed
-- outcomes before a constraint has to reject a mutation.

CREATE INDEX projects_parent_id_idx ON projects (parent_id);

CREATE FUNCTION octacity_reject_project_cycle()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  IF NEW.parent_id IS NULL THEN
    RETURN NEW;
  END IF;

  IF NEW.parent_id = NEW.id OR EXISTS (
    WITH RECURSIVE descendants(id) AS (
      SELECT id
      FROM projects
      WHERE parent_id = NEW.id
      UNION
      SELECT project.id
      FROM projects AS project
      JOIN descendants AS descendant ON project.parent_id = descendant.id
    )
    SELECT 1 FROM descendants WHERE id = NEW.parent_id
  ) THEN
    RAISE EXCEPTION 'project hierarchy must remain acyclic'
      USING ERRCODE = '23514', CONSTRAINT = 'projects_acyclic';
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER projects_reject_cycle
BEFORE INSERT OR UPDATE OF parent_id ON projects
FOR EACH ROW
EXECUTE FUNCTION octacity_reject_project_cycle();

ALTER TABLE pipeline_versions
  ADD CONSTRAINT pipeline_versions_positive_version CHECK (version > 0);

CREATE FUNCTION octacity_reject_pipeline_snapshot_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'published pipeline snapshots are immutable'
    USING ERRCODE = '23514', CONSTRAINT = 'pipeline_versions_immutable';
END;
$$;

CREATE TRIGGER pipeline_versions_reject_mutation
BEFORE UPDATE OR DELETE ON pipeline_versions
FOR EACH ROW
EXECUTE FUNCTION octacity_reject_pipeline_snapshot_mutation();

ALTER TABLE attempts
  ADD CONSTRAINT attempts_positive_number CHECK (attempt_number > 0),
  ADD CONSTRAINT attempts_build_number_key UNIQUE (build_id, attempt_number);

CREATE FUNCTION octacity_require_next_attempt_number()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
  expected_number BIGINT;
BEGIN
  -- Lock the Build to serialize allocation across server replicas.
  PERFORM 1 FROM builds WHERE id = NEW.build_id FOR UPDATE;
  SELECT COALESCE(MAX(attempt_number), 0) + 1
    INTO expected_number
    FROM attempts
    WHERE build_id = NEW.build_id;

  IF NEW.attempt_number <> expected_number THEN
    RAISE EXCEPTION 'attempt number must be %, got %', expected_number, NEW.attempt_number
      USING ERRCODE = '23514', CONSTRAINT = 'attempts_monotonic_number';
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER attempts_require_next_number
BEFORE INSERT ON attempts
FOR EACH ROW
EXECUTE FUNCTION octacity_require_next_attempt_number();

CREATE FUNCTION octacity_reject_attempt_identity_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'attempt build and number are immutable'
    USING ERRCODE = '23514', CONSTRAINT = 'attempts_immutable_identity';
END;
$$;

CREATE TRIGGER attempts_reject_identity_update
BEFORE UPDATE OF build_id, attempt_number ON attempts
FOR EACH ROW
WHEN (OLD.build_id IS DISTINCT FROM NEW.build_id OR OLD.attempt_number IS DISTINCT FROM NEW.attempt_number)
EXECUTE FUNCTION octacity_reject_attempt_identity_mutation();

CREATE TRIGGER attempts_reject_delete
BEFORE DELETE ON attempts
FOR EACH ROW
EXECUTE FUNCTION octacity_reject_attempt_identity_mutation();

ALTER TABLE jobs
  ADD CONSTRAINT jobs_attempt_id_id_key UNIQUE (attempt_id, id);

ALTER TABLE job_dependencies
  DROP CONSTRAINT job_dependencies_job_id_fkey,
  DROP CONSTRAINT job_dependencies_dependency_job_id_fkey,
  ADD CONSTRAINT job_dependencies_not_self CHECK (job_id <> dependency_job_id),
  ADD CONSTRAINT job_dependencies_job_attempt_fk
    FOREIGN KEY (attempt_id, job_id) REFERENCES jobs(attempt_id, id),
  ADD CONSTRAINT job_dependencies_dependency_attempt_fk
    FOREIGN KEY (attempt_id, dependency_job_id) REFERENCES jobs(attempt_id, id);

CREATE INDEX job_dependencies_dependency_job_id_idx
  ON job_dependencies (dependency_job_id);

ALTER TABLE ready_queue_entries
  ADD CONSTRAINT ready_queue_claim_pair CHECK (
    (claim_owner IS NULL AND claim_expires_at IS NULL)
    OR (claim_owner IS NOT NULL AND claim_expires_at IS NOT NULL)
  );

CREATE UNIQUE INDEX leases_one_current_per_job_idx
  ON leases (job_id)
  WHERE state IN ('active', 'cancellation_requested', 'drain_requested');

CREATE FUNCTION octacity_guard_ready_job_ownership()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  PERFORM 1 FROM jobs WHERE id = NEW.job_id FOR UPDATE;

  IF EXISTS (
    SELECT 1
    FROM leases
    WHERE job_id = NEW.job_id
      AND state IN ('active', 'cancellation_requested', 'drain_requested')
  ) THEN
    RAISE EXCEPTION 'job already has a current lease'
      USING ERRCODE = '23514', CONSTRAINT = 'jobs_one_current_queue_or_lease';
  END IF;

  RETURN NEW;
END;
$$;

CREATE FUNCTION octacity_guard_leased_job_ownership()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  PERFORM 1 FROM jobs WHERE id = NEW.job_id FOR UPDATE;

  IF NEW.state IN ('active', 'cancellation_requested', 'drain_requested')
    AND EXISTS (SELECT 1 FROM ready_queue_entries WHERE job_id = NEW.job_id) THEN
    RAISE EXCEPTION 'job is still present in the ready queue'
      USING ERRCODE = '23514', CONSTRAINT = 'jobs_one_current_queue_or_lease';
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER ready_queue_guard_current_ownership
BEFORE INSERT OR UPDATE ON ready_queue_entries
FOR EACH ROW
EXECUTE FUNCTION octacity_guard_ready_job_ownership();

CREATE TRIGGER leases_guard_current_ownership
BEFORE INSERT OR UPDATE OF job_id, state ON leases
FOR EACH ROW
EXECUTE FUNCTION octacity_guard_leased_job_ownership();

ALTER TABLE agent_registrations
  ADD CONSTRAINT agent_registrations_positive_epoch CHECK (epoch > 0),
  ADD CONSTRAINT agent_registrations_agent_epoch_key UNIQUE (agent_id, epoch);

ALTER TABLE job_events
  ADD CONSTRAINT job_events_positive_sequence CHECK (sequence > 0);

CREATE FUNCTION octacity_require_next_event_sequence()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
  expected_sequence BIGINT;
BEGIN
  -- The Job row is the stream lock, avoiding a process-local sequencer.
  PERFORM 1 FROM jobs WHERE id = NEW.job_id FOR UPDATE;
  SELECT COALESCE(MAX(sequence), 0) + 1
    INTO expected_sequence
    FROM job_events
    WHERE job_id = NEW.job_id;

  IF NEW.sequence <> expected_sequence THEN
    RAISE EXCEPTION 'event sequence must be %, got %', expected_sequence, NEW.sequence
      USING ERRCODE = '23514', CONSTRAINT = 'job_events_contiguous_sequence';
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER job_events_require_next_sequence
BEFORE INSERT ON job_events
FOR EACH ROW
EXECUTE FUNCTION octacity_require_next_event_sequence();

CREATE FUNCTION octacity_reject_event_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION 'durable job events are append-only'
    USING ERRCODE = '23514', CONSTRAINT = 'job_events_append_only';
END;
$$;

CREATE TRIGGER job_events_reject_mutation
BEFORE UPDATE OR DELETE ON job_events
FOR EACH ROW
EXECUTE FUNCTION octacity_reject_event_mutation();

CREATE UNIQUE INDEX artifacts_published_job_name_idx
  ON artifacts (job_id, logical_name)
  WHERE published_at IS NOT NULL;

CREATE FUNCTION octacity_preserve_published_artifact_identity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  IF OLD.published_at IS NOT NULL AND TG_OP = 'DELETE' THEN
    RAISE EXCEPTION 'published artifact identity is immutable'
      USING ERRCODE = '23514', CONSTRAINT = 'artifacts_published_identity_immutable';
  END IF;

  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;

  IF OLD.published_at IS NOT NULL AND (
    OLD.build_id IS DISTINCT FROM NEW.build_id
    OR OLD.attempt_id IS DISTINCT FROM NEW.attempt_id
    OR OLD.job_id IS DISTINCT FROM NEW.job_id
    OR OLD.logical_name IS DISTINCT FROM NEW.logical_name
    OR OLD.media_type IS DISTINCT FROM NEW.media_type
    OR OLD.report_format IS DISTINCT FROM NEW.report_format
    OR OLD.byte_length IS DISTINCT FROM NEW.byte_length
    OR OLD.sha256 IS DISTINCT FROM NEW.sha256
    OR OLD.object_identity IS DISTINCT FROM NEW.object_identity
    OR OLD.object_generation IS DISTINCT FROM NEW.object_generation
    OR OLD.published_at IS DISTINCT FROM NEW.published_at
  ) THEN
    RAISE EXCEPTION 'published artifact identity is immutable'
      USING ERRCODE = '23514', CONSTRAINT = 'artifacts_published_identity_immutable';
  END IF;

  RETURN NEW;
END;
$$;

CREATE TRIGGER artifacts_preserve_published_identity
BEFORE UPDATE OR DELETE ON artifacts
FOR EACH ROW
EXECUTE FUNCTION octacity_preserve_published_artifact_identity();
