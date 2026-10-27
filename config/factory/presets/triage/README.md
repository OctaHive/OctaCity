# Triage completion document

`completion-v1.json` is the versioned, reviewable process-completion draft for
Project → Systems analyst triage. Its immutable reference hashes the exact UTF-8
`content`, including line endings. `SourceCompletionDocument::review_preset`
binds it to a Project without publishing bytes or creating a Build. A later
explicit Source preparation selection publishes ordinary private Work inputs.

The resulting acceptance-role Artifact has `purpose: process_completion`; its
provenance identifies this document separately from the original ticket. A raw
ticket keeps its missing body and source acceptance. Existing field/body/heading
mappings retain their original required-content behavior.

An operator may instead select an exact already published private Project text
input. The common reader verifies its Project, reference, bytes and retention.
The complete two-stage preset resolver, setup and packaged execution are delivered
by the subsequent tasks of `add-factory-triage-pipeline`.
