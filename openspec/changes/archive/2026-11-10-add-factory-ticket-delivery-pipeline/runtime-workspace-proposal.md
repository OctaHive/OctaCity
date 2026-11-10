# Proposed Codex execution workspace contract

Status: approved by the operator on 2026-11-07 and qualified in both Delivery modes through confirmed PR receipts (tasks 6.3–6.4). Published upstream in Octa 0.9.0 on 2026-11-08; OctaCity now consumes that release without a local adapter patch.

## Observed failure

The standard stand's Tester reaches a real ordinary Build and native Codex,
but cannot publish the required test file. An isolated native-client probe
reproduces this without Factory orchestration. Bubblewrap reports
`Can't mkdir /workspace/.git: Read-only file system`; precreating that directory
merely moves the failure to `.agents`, then `.aws`. A non-root Bubblewrap
namespace probe and the native protected verifier isolation test both pass.

Factory's read-only workspace parent is intentional. Only the selected source,
scratch and output subtrees are exposed with their separately declared access.
Codex's workspace-write sandbox currently receives that parent as its working
directory and tries to prepare protected metadata beneath it.

Adding provider-specific placeholder directories or making the parent writable
would obscure this mismatch. Neither is proposed.

## Proposed adapter extension

Keep the Octa task's directory and resource publication paths unchanged. Extend
the common Codex plugin with optional, bounded workspace-relative settings for
the native client's working directory and additional writable directories.
Existing tasks without the settings retain their existing behavior.

Factory derives the settings from its admitted layout and effective grants:

- Native working directory: `scratch`.
- Additional writable directory: `output`.
- Additional writable directories `source/<path>` only for the exact admitted capture write paths. The read-only source root is never advertised as a writable directory.
- No writable source directory for Tester, Reviewer, or a source-free step.

The plugin resolves the selected directories beneath the effective task
directory, rejects escapes and substitutions, and supplies explicit Codex
working-directory and writable-directory arguments. It retains sandboxing,
noninteractive approvals, tool authorization, and the existing model, credential,
record, and resource contracts. Compiler instructions name the absolute selected
source and output paths so changing the native working directory is unambiguous.

Advertise and require an explicit adapter capability. Older installed adapters
must report incompatibility rather than ignore the settings. Package any required
adapter update through the existing pinned Octa installation and qualification
mechanism; do not substitute an untracked local executable.

This adds no UI fields or state owner, and does not broaden the admitted mounts,
network, secrets or source scope.

## Required Red/Green qualification

1. At the native adapter boundary, reproduce writing a declared output beneath
   a read-only workspace parent; also prove that an ungranted source stays
   unwritable and directory escapes are rejected.
2. Test compiler derivation for writable source, read-only source and no-code
   steps, while preserving the original artifact paths and fencing.
3. Test installed capability detection and repeatable installation updates that
   preserve existing profiles and published Flows.
4. Run both Delivery modes from Console through baseline, Coder, independent
   review, the same retained protected suite, and a confirmed draft PR receipt.

Tasks 6.3 and 6.4 record the completed native qualification and exact PR receipts.
