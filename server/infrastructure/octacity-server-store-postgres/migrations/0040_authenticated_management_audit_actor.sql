ALTER TABLE audit_facts
  DROP CONSTRAINT audit_facts_actor_kind_known,
  ADD CONSTRAINT audit_facts_actor_kind_known CHECK (
    actor_kind IN (
      'unauthenticated_management',
      'authenticated_management',
      'agent',
      'trigger',
      'orchestrator',
      'adapter',
      'worker'
    )
  );
