# 6.11 — installed read-only review/check observations

Date: 2026-10-21. Scope: task 6.11 only; review REST/generated clients, Console and
persisted typed check-result projections remain subsequent tasks.

## Public boundaries and Red → Green

All commands ran at the repository root through `/tmp/octacity-611-cargo`, a small
wrapper for `cargo` using one task-owned temporary `CARGO_TARGET_DIR`, two build
jobs, disabled incremental compilation and zero dev/test debug information.

| Command after the wrapper | Observed Red | Observed Green |
| --- | --- | --- |
| `test --locked -p octacity-work-source-protocol --test review` | The new review read frame was `Invalid`; later checks/authorize/cancel frames were also rejected. | Independent `vcs_read`, closed read-only operations, original subject/head correlation and bounded check pages pass. |
| `test --locked -p octacity-server-work-source --test host review_role` | A manifest explicitly installing `capabilities.review` was rejected as unknown. | The verified common host dispatches the separate role and preserves a fake MR vocabulary. Without that role, no protected handle reaches a process. |
| `test --locked -p octacity-github --test review_reading installed_repository_reader` | The private profile rejected `review_repositories`. | Native repository/PR identity verification and safe optional metadata pass over a controlled HTTPS peer. |
| `test --locked -p octacity-github --test review_reading grant_checks` | The installed grant check returned `Unsupported`. | The existing private repository-read credential is checked without provider I/O; ticket credentials cannot substitute it. |
| `test --locked -p octacity-github --test review_reading check_pages` | Original-head checks returned `Unsupported`. | One bounded page, safe normalized check facts and scope-bound continuation pass without following Link authority. |
| `test --locked -p octacity-github --test review_reading native_process` | The actual JSONL process did not acknowledge review cancellation. | Native metadata, independent installed credentials, closed failures and cancellation pass through the existing process server. |
| `test --locked -p octacity-github --test review_reading caller_response_bound` | Provider metadata escaped the caller's lower response ceiling. | Oversized normalized responses produce a closed protocol fault. |
| `test --locked -p octacity-server --lib installed_review_reader_requires_current_independent_repository_grants` | The required hosted reader/profile API did not exist. | The existing application observation port reads through managed `RepositoryRead` grants; revocation precedes dispatch and preserves installed metadata visibility. |
| `test --locked -p octacity-server --lib installed_check_reads` | The bounded hosted check-read API did not exist. | Original-head pages preserve the caller's stable read identity, narrow item bounds and retain rate-limit advice without hidden retry. |
| `test --locked -p octacity-server --lib lineage_requires_an_explicit` | The required local Repository/publication-policy mapping API did not exist. | Optional candidate lineage and exact original publication-policy correspondence are checked locally, without provider calls or forge write authority. |

The final native-key regression used
`test --locked -p octacity-github --test review_reading github_checks_require`:
**Red** accepted a URL-shaped task key as a review subject for checks. **Green**,
via `test --locked -p octacity-github --test review_reading`, passes all **9**
review/check tests after reusing canonical native-key validation for both reads.
Invalid GitHub review keys now fail before provider I/O; the normalized wire and
second fake vocabulary retain their independent opaque identities.

## Composition and qualification

- `review` is an independently versioned optional family in the existing Work
  protocol and verified registry/process host. Legacy Source/Reporter manifests
  remain valid and do not acquire repository-read authority. No provider package
  or SDK was added to the Issue domain or application.
- The concrete GitHub adapter reuses the existing bounded HTTPS transport, private
  credential files/manager, network confinement and Source cursor/Link validation.
  Native repository identity verification is shared with Source and Reporter;
  their existing behavioral tests passed before that refactoring.
- Runtime reads share the existing closed process/provider failure translation
  with Source. Existing Source/runtime behavior passed before the extraction.
  Current managed grants are checked before and after the read under one original
  deadline. Observation time is captured before dispatch, without resetting it
  after a slow read or authority recheck.
- Controlled-peer and real JSONL process tests cover native PR metadata, check
  pages, missing/incorrect credentials despite ambient tokens, head/identity
  mismatch, hostile Link/cursor input, duplicate checks, response ceilings,
  unsupported fields/conclusions, rate limiting and cancellation during stalled
  HTTPS. A second fake MR vocabulary uses the same protocol, verified host and
  managed-grant runtime path; it is not a native GitLab/Jira implementation.
- Runtime tests cover independent purpose grants, exact local lineage/policy
  mappings, revocation before and during reads, cancellation, original time and
  timeout. These reads publish no inputs and create no Work/Run. Existing manual
  Issue/application behavior remains independent of provider installation.
- Each explicit read performs one attempt and at most one check page. Continuation
  and retry advice are returned to the existing owner; no SDK retry, new retry
  engine, webhook or poller is introduced. Existing retained observations are not
  cleared by an unsuccessful refresh.
- Native review approval/legacy commit-status aggregation and complete check
  summaries are unsupported and remain absent. A check outcome is external
  observation, never acceptance, merge permission or Issue lifecycle authority.
- No live provider qualification, schema migration, REST/Console implementation,
  commit or remote publication is part of this slice.

## Final verification

- `cargo test --locked -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github`: **154 passed**. After the final native-key correction, all **9** `review_reading` tests passed again (one additional regression).
- `cargo test --locked -p octacity-server-store --lib`: **81 passed**. The shared repository-identity helper preserves canonical metadata identities without manufacturing a review key.
- `cargo test --locked -p octacity-server-application --lib issue_code_review`: **6 passed**, including exact Delivery provenance, retained history, visibility and manual task independence.
- `cargo test --locked -p octacity-server --lib`: **179 passed, 29 ignored**. Ignored tests require an explicitly configured disposable PostgreSQL service; this task changes no database behavior or schema. Five new installed-reader tests cover the runtime path and in-flight authority/supervision cases.
- `cargo clippy --locked -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github -p octacity-server --all-targets -- -D warnings`: passed, including the final changes.
- `python3 tools/check_architecture.py`: **63 packages / 256 edges**, unchanged. `python3 -m unittest discover -s tools/tests -p 'test_check_architecture.py'`: **32 passed**.
- `cargo fmt --all --check`, `git diff --check`, and `openspec validate add-factory-work-card --strict`: passed.

Documentation-only changes were checked against the implemented public contracts;
they introduce no executable behavior requiring a separate Red/Green cycle.
OpenSpec progress is **43/56**, with only 6.11 newly checked.

## Resource use

Only one owned temporary Cargo target was used. Free disk space was checked
before and during builds (about 11 GiB initially, staying above 4 GiB during
verification). After verification the owned target was removed, reclaiming about
3.8 GiB; final free space is approximately **8.7 GiB**. Existing user caches and
checkouts remain intact. Local documentation links were also checked successfully.
