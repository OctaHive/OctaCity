# Server backup, restore, and reconciliation

OctaCity recovery treats PostgreSQL and the configured object-store prefix as
one consistency unit. PostgreSQL is authoritative for visible metadata, while
the object store contains immutable Artifact, report, Build-log, and remote
cache bytes. A backup is recoverable only when both parts carry the same backup
identity and were captured while every server and Agent ingress was quiesced.

An object without committed PostgreSQL metadata is harmless and may be removed
later. Committed visible metadata whose object is absent or fails its digest
check makes the restored unit invalid. The server must not receive traffic in
that state.

## Protected recovery set

Keep the following material together under one encrypted, access-controlled
backup identity:

- the PostgreSQL snapshot and the complete configured object-store prefix,
  including immutable Artifact, report, Build-log, cache, and health objects;
- the server TOML and immutable JobSpec policy used by the restored release;
- JobSpec signing keys, Agent enrollment keys, and cache credential keys;
- references for PostgreSQL and object-store credentials;
- VCS and webhook adapter registries, digest pins, integration identities, and
  provider credential references.

Do not put raw credentials or private keys in backup command arguments,
manifests, evidence logs, or source control. Resolve them through the deployed
secret manager or protected regular files. On Unix, those files must remain
non-symlink, process-owner files with owner-only permissions.

## Capture procedure

1. Remove every public, Agent, cache, and webhook ingress from service.
2. Gracefully stop every server replica and Agent. Confirm that no writer can
   reach PostgreSQL or the object prefix.
3. Create a backup manifest with a unique backup identity, UTC capture time,
   OctaCity release and source revision, PostgreSQL cluster identity and schema
   version, bucket and prefix, and the database and object snapshot identities.
4. Capture PostgreSQL with a protected connection service or credential file.
   For example:

   ```shell
   PGSERVICE=octacity-backup pg_dump --format=custom \
     --file=octacity.pgdump octacity
   ```

5. While the deployment remains offline, snapshot or export the complete
   configured object prefix. Preserve object contents and metadata; do not
   select objects from an independently timed database query.
6. Back up the protected configuration, policy, keys, adapter registries, and
   credential references listed above.
7. Mark the manifest complete only after the database and object-store parts
   have both succeeded and have been assigned the same backup identity.

A provider-native database and bucket snapshot is acceptable if the operator
can prove the shared quiesced interval. Taking two live, unrelated snapshots is
not a coordinated backup.

## Restore and integrity gate

1. Keep all restored listeners and workers offline.
2. Restore PostgreSQL and the object prefix from the same complete backup
   manifest. Restore the exact protected configuration, policy, keys, adapter
   pins, integration identities, and credential references.
3. Select the exact OctaCity release recorded by the manifest. Validate its
   configuration without binding listeners:

   ```shell
   octacity-server validate /etc/octacity/server.toml
   ```

4. Reconcile every visible PostgreSQL object reference against independently
   verified bytes. Rebuilding search is the recommended disaster-recovery path:

   ```shell
   octacity-server reconcile-restore /etc/octacity/server.toml \
     --rebuild-log-search
   ```

   The command binds no listener. It rejects pending or incompatible migration
   history, missing objects, size or digest mismatches, malformed inventory,
   unavailable storage, and incomplete projection reset. A nonzero exit leaves
   the deployment offline; do not bypass it by starting the normal server.
5. Retain the command's non-secret verified counts and backup identity as
   recovery evidence. Never retain URLs, credentials, private keys, or object
   contents in that evidence.
6. Start one server replica from the same configuration and wait for
   `/health/ready`. Only then restore ingress and add replicas or Agents.

The authoritative lease and worker-claim deadlines remain in PostgreSQL. The
offline reconciliation command expires restored Factory ownership before it
verifies objects: current Run and Factory-retention claims are cleared, and
claimed durable outbox records receive a new pending retry record without
rewriting their append-only history. Normal fencing and expiry workers perform
the corresponding checks for Build leases and other work after startup;
expired ownership is never revived by restore.

Factory Runs extend the protected recovery set. Reconciliation verifies every
unreleased task, acceptance, specification, Build-output, ChangeSet, manifest,
and evidence reference for each visible or partially cleaned Run. Where the
Factory recorded an expected digest, the referenced immutable Artifact must
match it. Active or escalated Runs, and terminal Runs protected by an active
Build Result hold, retain those exact references. Terminal unheld Runs are
hidden before their references are released in bounded pages. Released
reference rows and unneeded Factory projection/history rows are then removed in
separate bounded, reclaimable passes; a minimal terminal Run and retention
tombstone remain for audit correlation. Physical Artifact deletion remains
owned by the ordinary Build-retention workflow after no live Factory reference
remains.

## Search projection choices

Build-log search is derived data. Operators may restore its PostgreSQL tables
with the consistency unit, or rebuild them from committed immutable chunks.
`--rebuild-log-search` removes derived documents and applied-operation state,
rebuilds tombstones, resets each indexed watermark, and requeues committed
work. It preserves the authoritative committed watermark. Search therefore
reports `caught_up: false` and the real indexed-through position until workers
finish replay; restored data is never presented as fresh prematurely.

## Rehearsal evidence

Rehearse capture and restore before every release and at the operational
recovery interval. Evidence must show:

- a matched database/object backup identity and successful offline
  reconciliation;
- rejection after deleting or corrupting one database-referenced object;
- successful startup only after reconciliation;
- an expired pre-backup lease remaining fenced after restore; and
- honest search freshness from projection reset through durable catch-up.

The PostgreSQL and MinIO contracts referenced in
[`backend-contracts.md`](../testing/backend-contracts.md) exercise the bounded
inventory, byte-integrity, and projection-watermark pieces of this procedure.
