# Shared Project input publication handoff

Work-card tasks 3.2–3.3 supply the shared application/store boundary anticipated by
quickstart design decisions 2–3 and tasks 3.1–3.3 / 4.1–4.4. Quickstart must call
`ProjectTextIntake`, not create another submission journal or byte publisher.

## Delivered shared boundary

- `ProjectTextSubmission` binds exact Project/Configuration/Repository versions,
  reviewed source selection and classification, application-owned external Work
  identity, mapping version, exact UTF-8 task/acceptance documents and replay key.
  It has no Issue or installed Source prerequisite.
- The command separates `ProjectTextIntent` metadata from `documents`.
  Preparation reads omit private bodies; scoped `project_text_documents` reads
  recover the original pair only while it is retained. PostgreSQL stores bodies
  separately and removes their record after cleanup, preserving valid receipt
  metadata without empty-document sentinels. Reuse this contract for recovery.
- `ProjectTextDocuments` validates nonblank exact bytes, supported controls, the
  common prepared-input byte ceiling and existing aggregate Context ceiling.
  It does not normalize whitespace, Unicode or line endings. Quickstart retains
  its stricter **16,384 UTF-8 bytes per field** at its own closed ingress schema;
  the common ceiling also accommodates existing larger Issue documents.
- `ProjectTextInputStore` reserves original private intent, both immutable
  Artifact/upload identities, original retention and bounded preparation ownership
  before I/O. One fenced trusted source freeze precedes publication. Recovery
  observes and verifies immutable bytes before retrying a missing object, then
  makes the complete pair visible. Expired/stale owners cannot publish or admit;
  retries do not renew the original deadline, attempt ceiling or input lifetime.
- The service uses the same ordinary manual admission probe, Work/Run constructor
  and admission transaction as existing Artifact intake. Its original receipt is
  linked atomically with the Project-input operation in memory and PostgreSQL.
  Completed scoped replay precedes target/source lookup and creates no new Run.
- `FactoryInputObject` / `FactoryInputRole` are common immutable byte metadata;
  existing Source names remain aliases with unchanged serialized shapes.
  `ProjectInputQueryStore` and `FactoryArtifactReader::with_project_inputs` add
  retained private reads with Project, exact bytes/media and retention checks.
  These documents cannot be read as produced outputs/evidence. No synthetic
  Build, Job, Agent, Source observation or Issue is required.

Work-card 3.3 adds the PostgreSQL implementations, native runtime reader/intake
composition, existing protected-transfer integration and real MinIO byte/download
qualification. The existing supervised private-input cleanup worker handles
abandoned and admitted Project inputs, ordinary Build holds, exact deletion retry,
private-body retirement and late-write rechecks. Completed replay keeps the
original metadata receipt without restoring bodies. Process-kill and transaction
fault tests qualify recovery through the same authorized submission/Issue commands.

## Reconcile the remaining quickstart tasks

Quickstart 3.1 should qualify its stricter input/HTTP schema on the common types.
Tasks 3.2–3.3 should reuse the delivered metadata/read contracts and the durable
implementation delivered by work-card 3.3. Task 3.4 still owns its preset-specific
real pipeline/Agent qualification using the common protected-transfer boundary;
the shared S3/transfer qualification does not establish a quickstart execution pilot.

Tasks 4.1–4.2 should adapt reviewed preset submissions to the shared intake and
qualify quickstart-specific readiness, conservative classification and user
receipts. Tasks 4.3–4.4 should reuse common durable ownership, recovery, retention
and supervision rather than duplicate them. Quickstart still owns readiness,
bounded HTTP request/status/resume behavior and its asynchronous submission
adaptation. No quickstart task is marked complete by this handoff.

See [work-card verification](../add-factory-work-card/verification-3.2.md) for
public-boundary Red/Green evidence and exact qualification scope.
See [PostgreSQL/crash verification](../add-factory-work-card/verification-3.3.md)
for the durable shared implementation.
See [storage review corrections](../add-factory-work-card/verification-review-fixes.md)
for separated documents, indexed cleanup and schema-73 upgrade qualification.
