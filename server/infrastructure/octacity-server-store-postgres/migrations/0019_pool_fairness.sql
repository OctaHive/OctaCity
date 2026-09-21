ALTER TABLE pools
  ADD COLUMN fairness_policy TEXT NOT NULL DEFAULT 'priority_fifo',
  ADD CONSTRAINT pools_fairness_policy
    CHECK (fairness_policy IN ('priority_fifo', 'configuration_fair'));
