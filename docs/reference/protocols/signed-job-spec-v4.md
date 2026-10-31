# Signed JobSpec execution contract v4

Revision 4 adds explicit source-free managed execution. It reuses the signed
envelope, Job/Attempt binding, runtime, protected inputs, permissions, output
quotas and deadlines of [revision 3](signed-job-spec-v3.md). Historical v1–v3
payloads retain their existing encoding and meaning.

## Source absence

The authenticated payload selects `"protocol_version": 4` and replaces the
repository source object with exactly:

```json
{"source": {"kind": "source_free_v1"}}
```

This form has no provider, plugin digest, repository locator, revision or
reference. Unknown fields are rejected. Empty strings, a placeholder repository
and a hidden checkout are not valid substitutes for source absence.

Managed execution does not imply Factory causality. An ordinary managed Job
may omit `factory`; a Factory-created Job retains its exact existing causality.
Neither form may request a source mount, source credentials or ChangeSet
capture/materialization when its source is absent.

## Placement and execution

A source-free template requires negotiated execution contract v4 and an
isolated route enforcing its complete managed capability requirements. An
Agent advertising only v1–v3 remains eligible for compatible repository-backed
Jobs but cannot lease or execute a source-free Job. Payload verification checks
the negotiated revision before filesystem or backend activity.

The common executor creates a private workspace and stages only the selected
protected inputs. It skips source-plugin lookup, VCS resolution and checkout.
Nonempty source credentials are rejected before workspace preparation. Runtime
scratch/output directories and empty runtime path anchors confer no repository
access. Results are published from the output workspace; source provenance and
ChangeSets remain absent. Retry and cancellation use the existing Attempt,
deadline, cleanup and result owners.

## Protected input authority

Stable intent retains immutable metadata, not download URLs. After committing
the Lease, the server resolves the exact consuming Job's retained selection.
Project publications must belong to that Job's Project. Private compiler
documents additionally require the exact retained Factory operation; a Job from another invocation in the same Project cannot borrow them. Knowing an Artifact identifier or
digest grants no read authority.

The server authorizes the complete selection and verifies its publications
before minting downloads. Each capability is bounded by both the Lease and
publication expiry. The Agent independently matches the signed manifest and
checks content integrity before execution. No transfer capability is persisted
in a Build or Job template.
