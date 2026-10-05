-- The existing one-current-registration index resolves the Agent first; this index
-- then finds its non-terminal Lease without scanning historical executions.
CREATE INDEX leases_registration_current_idx
  ON leases (registration_id, leased_at DESC, id)
  WHERE state IN ('active', 'cancellation_requested', 'drain_requested');
