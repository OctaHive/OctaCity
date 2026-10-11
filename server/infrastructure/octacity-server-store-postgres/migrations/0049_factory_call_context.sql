-- Unreleased Factory Context is frozen in common node input documents.
-- Retained input/output Artifact roles do not select logical stages.

ALTER TABLE factory_artifact_references
  DROP CONSTRAINT factory_artifact_references_role_known,
  ADD CONSTRAINT factory_artifact_references_role_known CHECK (
    role IN ('task', 'acceptance', 'specification', 'build_output', 'changeset_bundle',
             'changeset_manifest', 'evidence', 'node_context')
  );
