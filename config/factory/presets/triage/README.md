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

## Reviewed Project context

`TriageProjectContextDraft` compiles a reviewed same-Project selection to an
ordinary `FlowContextBinding`. Goals and controlling rules are required exact
Artifact references; supporting documents use explicitly selected aliases.
Missing goals/rules and a foreign draft Project have distinct safe diagnostics.
The common binding and Context Manifest enforce their existing size/count bounds.
Compilation performs no document I/O and grants no read or execution authority.
The common Artifact reader still checks ownership, exact content, publication
and retention when Context is prepared or consumed. An unavailable input reports
its logical name and original safe failure class without document/backend text.

Apply `ticket_binding()` and `bind(project_id)` to the ordinary Project-stage
template before publication. The former loads `ticket-input-v1.json`: `task`
and `completion` come only from the admitted Work, while the selected code
Repository/revision stays in the immutable Work/node subject. A source ticket
repository, ticket text or prior model output cannot replace that subject.
The latter fixes `project-goals` as specification data, `project-rules` as
controlling policy and each supporting `document.<alias>` as specification data.
All selections are unconditional; a document alias cannot replace `task`.

Declare the same required identities in the selected Profile's ordinary
`context_requirements`. The role package retains those exact inputs and rules;
Role display names and supporting text grant no tools, credentials or permission
to waive controlling rules. Changing a draft requires a newly reviewed ordinary
publication; retained bindings/packages keep their original identities.
Do not forward this whole Context implicitly to the analyst: its separate
selection and accepted handoff are qualified by the subsequent tasks.
