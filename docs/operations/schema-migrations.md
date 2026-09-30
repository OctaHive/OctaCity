# Server schema migration and rollback contract

OctaCity uses ordered, forward-only PostgreSQL migrations. A server accepts a
database only when the rows in `_sqlx_migrations` are an exact, uninterrupted
checksum-matching prefix of the migrations embedded in that binary:

- an empty or exact older prefix is `Pending` and may be migrated forward;
- the complete embedded history is `Current`;
- a failed row, gap, checksum change, reordered version, or migration unknown
  to the binary is `Incompatible`.

Readiness performs this classification without taking the migration advisory
lock. It runs the migrator only for `Pending`; `Incompatible` keeps readiness
false, so the server accepts neither management mutations nor Agent leases.
Migration failure also keeps readiness false. The failed PostgreSQL session is
discarded so its session-level advisory lock cannot block a later retry.

## Declared rollback window

The PostgreSQL adapter declares `PREVIOUS_BINARY_SCHEMA_VERSION`, which is the
single schema boundary supported for rollback to the immediately preceding
server binary. A unit test requires it to remain exactly one migration behind
`current_schema_version()`, so every schema change must deliberately move the
window and rehearse it in CI.

The window is snapshot-based. It does **not** permit a previous binary to open
a database already migrated by a newer binary. Before an upgrade:

1. stop mutating traffic and record the verified previous release;
2. capture the database at `PREVIOUS_BINARY_SCHEMA_VERSION` as part of the
   same consistency unit as object storage;
3. retain that immutable snapshot until the rollback window closes;
4. start the new binary and allow its forward migration;
5. if rollback is required, stop the new binary, restore the pre-upgrade
   consistency unit, verify the restored migration history, and only then
   start the previous binary.

Never attempt rollback by deleting `_sqlx_migrations` rows, editing checksums,
or applying ad-hoc down SQL. Those actions can make the history appear older
without restoring the data shape and object-store state expected by the old
binary.

## Executable rehearsal

The PostgreSQL migration contract creates an actual disposable database at the
declared previous boundary, writes a durable marker, and snapshots it with
PostgreSQL `CREATE DATABASE ... TEMPLATE`. Independent restores then prove:

- the current migrator advances the snapshot and preserves its data;
- an intentionally failing transactional migration leaves neither its schema
  changes nor history row and can be retried successfully;
- the previous migration set accepts the restored snapshot and cannot observe
  the newer column;
- newer, checksum-changed, and discontinuous histories are incompatible.

CI runs this ignored integration contract with the other PostgreSQL tests. To
run it locally against a disposable PostgreSQL administration database:

```shell
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-secret@127.0.0.1:15432/postgres
cargo test -p octacity-server-store-postgres --test migrations -- --ignored
```

The configured role must be allowed to create and drop disposable databases.
The template snapshot is test evidence, not the production backup mechanism;
the coordinated database/object-store backup procedure owns that operational
mechanism. Follow [`backup-restore.md`](backup-restore.md) for
the protected recovery set, consistency boundary, and offline integrity gate.
