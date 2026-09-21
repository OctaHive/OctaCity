-- Keep the placement binding self-contained on the Lease row.  The
-- registration foreign key already identifies an epoch, but the explicit
-- snapshot makes the fencing boundary queryable without deriving it from
-- mutable Agent state.

ALTER TABLE agent_registrations
  ADD CONSTRAINT agent_registrations_id_epoch_key UNIQUE (id, epoch);

ALTER TABLE leases
  ADD COLUMN registration_epoch BIGINT;

UPDATE leases AS lease
SET registration_epoch = registration.epoch
FROM agent_registrations AS registration
WHERE registration.id = lease.registration_id;

ALTER TABLE leases
  ALTER COLUMN registration_epoch SET NOT NULL,
  ADD CONSTRAINT leases_registration_epoch_positive CHECK (registration_epoch > 0),
  ADD CONSTRAINT leases_registration_binding_fk
    FOREIGN KEY (registration_id, registration_epoch)
    REFERENCES agent_registrations(id, epoch),
  ADD CONSTRAINT leases_expiry_after_issue CHECK (expires_at > leased_at);
