-- A confirmed technical failure is distinct from pending external work.
-- The Run remains operator-retryable and keeps its original reservation.
ALTER TABLE factory_runs DROP CONSTRAINT factory_runs_state_known;
ALTER TABLE factory_runs ADD CONSTRAINT factory_runs_state_known
  CHECK (state IN ('admitted', 'running', 'waiting', 'stopped', 'cancelled', 'completed'));
