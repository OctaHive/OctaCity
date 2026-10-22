-- Admission is independent of process presence, draining and registration lifetime.
ALTER TABLE agents ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT TRUE;
