UPDATE audit_facts
SET actor_kind = 'unauthenticated_management'
WHERE actor_kind = 'management_api';

ALTER TABLE audit_facts
  ADD CONSTRAINT audit_facts_actor_kind_known CHECK (
    actor_kind IN ('unauthenticated_management', 'agent', 'trigger', 'orchestrator', 'adapter', 'worker')
  ),
  ADD CONSTRAINT audit_facts_outcome_known CHECK (outcome = 'accepted');

CREATE INDEX audit_facts_newest_first
  ON audit_facts (occurred_at DESC, id DESC);

CREATE INDEX audit_facts_target_newest_first
  ON audit_facts (target_kind, target_identity, occurred_at DESC, id DESC);

CREATE INDEX audit_facts_operation_newest_first
  ON audit_facts (operation, occurred_at DESC, id DESC);
