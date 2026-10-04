-- Durable attention sources retain state transitions that cannot be
-- reconstructed from the current Build, Agent, and Agent Pool rows. The
-- application remains responsible for classification; this table stores only
-- bounded server-produced source facts and never models recipients or reads.
CREATE TABLE operator_attention_events (
  id UUID PRIMARY KEY,
  source_kind TEXT NOT NULL,
  target_kind TEXT,
  target_id UUID,
  source_identity UUID,
  code TEXT,
  summary TEXT NOT NULL,
  occurred_at TIMESTAMPTZ NOT NULL,
  resolved_at TIMESTAMPTZ,
  CONSTRAINT operator_attention_source_kind
    CHECK (source_kind IN (
      'build_failed',
      'build_succeeded',
      'agent_unavailable',
      'agent_pool_unavailable',
      'critical_system_condition',
      'audit_activity',
      'job_activity'
    )),
  CONSTRAINT operator_attention_target_shape CHECK (
    (source_kind IN ('build_failed', 'build_succeeded') AND target_kind = 'build' AND target_id IS NOT NULL AND source_identity IS NULL) OR
    (source_kind = 'agent_unavailable' AND target_kind = 'agent' AND target_id IS NOT NULL AND source_identity IS NULL) OR
    (source_kind = 'agent_pool_unavailable' AND target_kind = 'agent_pool' AND target_id IS NOT NULL AND source_identity IS NULL) OR
    (source_kind = 'critical_system_condition' AND target_kind IS NULL AND target_id IS NULL AND source_identity IS NOT NULL) OR
    (source_kind IN ('audit_activity', 'job_activity') AND target_kind IS NULL AND target_id IS NULL AND source_identity IS NULL)
  ),
  CONSTRAINT operator_attention_code_shape CHECK (
    (source_kind = 'critical_system_condition' AND code IS NOT NULL AND octet_length(code) BETWEEN 1 AND 64
      AND code ~ '^[a-z0-9_]+$') OR
    (source_kind <> 'critical_system_condition' AND code IS NULL)
  ),
  CONSTRAINT operator_attention_summary_shape CHECK (
    octet_length(summary) BETWEEN 1 AND 512
    AND summary = btrim(summary)
    AND summary !~ '[[:cntrl:]]'
  ),
  CONSTRAINT operator_attention_resolution_order CHECK (resolved_at IS NULL OR resolved_at >= occurred_at)
);

-- Target requests are always bounded and classified before ordering. Keeping
-- the target discriminator first lets one index serve Build, Agent, and Pool
-- scopes as well as unresolved-target settlement.
CREATE INDEX operator_attention_target_order_idx
  ON operator_attention_events (target_kind, target_id, occurred_at DESC, id DESC)
  WHERE source_kind IN ('build_failed', 'agent_unavailable', 'agent_pool_unavailable');

-- Critical conditions have no resource target and are selected explicitly.
CREATE INDEX operator_attention_critical_order_idx
  ON operator_attention_events (occurred_at DESC, id DESC)
  WHERE source_kind = 'critical_system_condition';

-- One active row per runtime source and stable server condition makes repeated
-- readiness checks idempotent without letting one replica resolve another.
CREATE UNIQUE INDEX operator_attention_active_critical_code_idx
  ON operator_attention_events (source_identity, code)
  WHERE source_kind = 'critical_system_condition' AND resolved_at IS NULL;
