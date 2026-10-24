-- Retain immutable Role/Profile versions and execution-package foreign keys.
ALTER TABLE factory_roles ADD COLUMN deleted BOOLEAN NOT NULL DEFAULT FALSE;
