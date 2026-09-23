-- Managed integrations retain protected handles separately from their
-- secret-free normalized remote registration state. A null remote state is a
-- durable creation intent that may be retried after a lost provider response.

ALTER TABLE webhook_integrations
  ADD COLUMN management_mode TEXT NOT NULL DEFAULT 'unmanaged',
  ADD COLUMN administration_credential_handle TEXT,
  ADD COLUMN remote_registration JSONB,
  ADD COLUMN registration_observed_at TIMESTAMPTZ,
  ADD CONSTRAINT webhook_integrations_management_mode_check
    CHECK (management_mode IN ('unmanaged', 'managed')),
  ADD CONSTRAINT webhook_integrations_managed_configuration_check
    CHECK (
      (management_mode = 'unmanaged' AND administration_credential_handle IS NULL)
      OR
      (management_mode = 'managed' AND administration_credential_handle IS NOT NULL)
    );
