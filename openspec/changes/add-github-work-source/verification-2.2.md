# Task 2.2 verification: input-scoped identity and retained admission recovery

Status: complete for task 2.2; change progress is 6/29. Task 2.3 and later remain pending.

## Public boundary and identity

`FactorySourceItem` carries bounded opaque native container and item identities.
The trusted `FactorySourceAuthorizer` resolves the native container identity
from the pinned installed resource definition. This is distinct from its
operator reference and supports native ids with mixed case without introducing
GitHub, GitLab or Jira DTOs into admission or the Flow interpreter.

The application constructs a domain-separated, length-framed identity from
stable installation, native container, native item and stable input identity.
Project/installation and accepted security scope partition the existing source
namespace. Source transport keys also include the derived item identity, so
the same poll key does not collide between different input bindings.

Binding/target/configuration versions, labels, observation time, task content
and credential values are absent from that identity. Installation descriptor
rotation keeps its stable installation identity, and the authorizer contract
requires a stable security scope across credential rotation. The common
intent's external identity cannot override this application-derived identity.

`FactorySourceAdmissionHandlers::recover` takes only input authorization facts
and native item identity. It checks current source authority and Project
execution access, reads the original receipt through the existing
`FactoryAdmissionStore`, and checks visibility of the retained Configuration,
Repository and task/acceptance/specification Artifacts before returning it.
No fresh artifacts or mutable VCS resolution are needed. Recovery is read-only
and reserves nothing. Submitting a different full intent still crosses strict
replay and conflicts instead of replacing the accepted Work.

## Baselines

Before changing behavior, the 16 existing manual/source admission tests passed:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
```

Before changing the PostgreSQL admission implementation, its existing public
memory/parity test passed on an independently created disposable PostgreSQL
18.6 container:

```sh
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test factory_discovery factory_configuration_admission_and_discovery_match_the_in_memory_contract -- --ignored --exact --test-threads=1
```

`OCTACITY_POSTGRES_URL` selected that owned loopback-only test service on port
32768. Test support created disposable UUID databases. The user's local stand
and its databases were not selected or changed.

## Observed Red/Green cycles

Tests were added or extended before their corresponding implementation changes.
Application Red/Green commands were:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib retained_intent -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
```

PostgreSQL Red/Green runs used the exact parity test command above with the
disposable service selected. Incidental test setup import/constructor/lock
binding errors were corrected; they are not counted as behavioral Red evidence.

| Cycle | Public behavior | Observed Red | Observed Green |
| --- | --- | --- | --- |
| 1 | One native item enters two independently configured stable inputs once each, for GitHub/GitLab/Jira/custom installations | E0432/E0560: native item API and command field absent | 17 admission tests passed |
| 2 | An observation cannot substitute an unauthorized native container | Invalid container was accepted and published Work | 2 accumulated recovery/identity tests passed |
| 3 | Read original admission by scoped identity, without fresh intent; isolate source/security/item partitions and preserve strict conflict checking | E0432/E0599: identity lookup type/method absent | Memory/PostgreSQL parity test passed, including restart and original receipt after Run advancement |
| 4 | Recover original artifacts/Work/base after restart and changed content/label/update metadata without VCS; changed full intent remains a conflict | E0432/E0599: source recovery command/method absent | 19 admission tests passed |
| 5 | Concurrent PostgreSQL observers with equal intent and distinct transport/candidate identities converge on one receipt | Losing publication returned `Duplicate { entity: FactoryRun }` | PostgreSQL parity test passed after strict replay following the uniqueness race |
| 6 | Fresh transport replay after Run advancement returns the same initial receipt as memory | PostgreSQL receipt differed from memory after the Run became `running` | PostgreSQL parity test passed; a separate current Run query still reported `Running` |
| 7 | Native immutable container ids remain opaque and are separate from operator resource references | E0560/E0308: trusted native container fact absent and resource type constrained to `FactoryKey` | 20 admission tests passed with mixed-case native ids and independent container scopes |

Cycle 5 reuses the existing uniqueness/idempotency authority: after a failed
duplicate/conflicting publication, PostgreSQL performs the existing strict
replay comparison. It does not accept a different digest or substitute the
losing observer's prepared Work. The extended public contract also verifies
one visible Run for that source and unchanged receipt after conflicting reuse.
Cycle 6 reconstructs only the immutable admission receipt; no runtime state
or progress row is rewritten.

## Additional behavioral characterization

Already-supported behavior was characterized through the same public interface
without manufacturing a failure or changing production behavior:

- Input replacement changes target Configuration/root Flow, task Artifacts and
  installation/configuration/resource descriptor versions while retaining input,
  native container/item and installation identities. Recovery returns the
  original version-one Work/Run; explicit changed-intent submission conflicts.
  A deliberately new input identity has no retained admission.
- Changed title/body mapping is represented by different immutable task and
  acceptance references; labels/update metadata and transport key also change.
  Recovery returns the original Work snapshot, admission time and base revision
  while an unavailable VCS resolver proves there is no new resolution.
- Recovery rejects denied/empty Project, execution, Configuration, Repository
  and every original Artifact visibility; revoked source authority is rejected.
  An independently authorized security partition cannot recover another scope.
- Two application observers are synchronized at a controlled VCS boundary.
  They receive one Work/Run with one admission/audit publication. Both may have
  resolved VCS before either publishes; this is not a claim of exclusive
  pre-admission preparation ownership.

The final admission suite contains 23 tests: 16 prior tests and 7 new tests.

## Scope limits

There is no new deduplication store, migration, dependency, provider network
client, REST/UI contract, background worker or live tracker activation. The
PostgreSQL lookup uses existing Work/Run/Configuration records and their scoped
identity. GitHub Delivery and ordinary manual admission continue through their
existing contracts.

Recovery covers already accepted immutable Work/Flow/artifact intent. Durable
pending preparation before admission, retained source input/provenance versions,
scan ownership/fencing and migration contracts remain task 2.3. Concrete private
artifact preparation/orphan cleanup remains task 2.4. Live credential resolution,
provider normalization and worker composition remain tasks 3.x. Descriptor
rotation is exercised without placing credential values in Work or tests.

Earlier uncommitted task 2.1 changes were preserved. No commit was created.

## Final regression and quality checks

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_flow_action_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib management_security::tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test factory_discovery -- --ignored --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-factory -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery --all-features -- --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Observed final regression outcomes: 23 admission tests, 15 configured action/
status tests, 16 management security tests, 21 PostgreSQL discovery/admission
contracts, 292 Factory tests and 29 GitHub/Delivery tests passed. That is 396
distinct behavioral tests plus 1 compile-fail doctest. The PostgreSQL file's
21 explicitly selected tests actually ran; none were ignored in that run.
Full application/release journey and unrelated database suites were not run.

Clippy passed with warnings denied. Formatting, architecture (61 workspace
packages, 245 dependency edges), strict OpenSpec and diff checks passed.
Documentation-only edits have no executable behavior and use these checks.

After qualification, the exact task-owned disposable PostgreSQL container was
stopped and removed after checking its captured id/name and task ownership
label. No existing project service or database was stopped, deleted or reset.
