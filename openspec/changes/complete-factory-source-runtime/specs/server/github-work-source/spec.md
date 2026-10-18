## ADDED Requirements

### Requirement: Managed automatic inputs run through released Source and Reporter composition
The released runtime SHALL resolve explicitly enabled managed input/group versions through trusted installed Source, preparation and optional Reporter profiles and current connection/resource/Project grants. Saving a connection, granting a Project, checking access or previewing SHALL NOT activate an input. Enabled valid inputs SHALL be discovered without a server restart and polled by the existing bounded supervised Source lifecycle. Disabling inputs or revoking access SHALL prevent subsequent dispatch; shutdown SHALL drain or fence active owners within existing bounds. Shared contracts SHALL use open provider identities and exact native pins, with GitHub as the concrete supported adapter and a synthetic second provider qualifying the common boundary. Neither card association nor a write credential SHALL be required for read-only intake.

#### Scenario: Operator enables a valid read-only input
- **WHEN** the operator publishes a valid enabled input using an installed Source/preparation profile and current task-read grant without Reporter authority
- **THEN** the running server polls it, retains a bounded immutable mapped snapshot and admits eligible data through the ordinary Factory boundary without native writes or requiring an internal Issue

#### Scenario: Existing source is granted to another Project
- **WHEN** an authorized administrator grants an already installed compatible resource to another Project and explicitly publishes its valid input
- **THEN** runtime resolves that Project's authorized profile/target and processes the input without hardcoded Project IDs, borrowing another Project's grant or requiring a per-input image rebuild

#### Scenario: Preview and publication use the same profile
- **WHEN** an operator previews a selector and publishes it with current versions
- **THEN** execution applies the same retained predicate and preparation selections under current authority, while preview has created no progress, reservation, Work or Run

### Requirement: Released source recovery preserves admitted identity and optional reporting
The composed runtime SHALL reuse existing durable scan progress, admission deduplication, snapshot/preparation journals and independently fenced reporting outbox rather than introducing another scheduler, reservation or effect journal. Restart and overlapping observations SHALL retain the original input-scoped Work/Run and immutable inputs. Missing required mapped content SHALL produce a classified disposition without fabricated acceptance data. Optional reporting SHALL consume only accepted configured actions with exact native item, mapping, provenance, original deadline and current task-update authority. Unknown provider permission SHALL remain unverified until lawful operation evidence exists; qualification checks SHALL NOT perform test writes.

#### Scenario: Restart follows admission before scan completion
- **WHEN** the released server stops after admission but before recording the page's completed progress and later resumes
- **THEN** the next fenced scan reuses the same Work/Run and original snapshot, processes remaining candidates and creates no duplicate execution

#### Scenario: Reporter loses an update response
- **WHEN** the remote provider applied an accepted configured effect but its response was lost
- **THEN** recovery observes and converges the original effect through the existing bounded outbox without creating a new Run or resetting its deadline

#### Scenario: Read or write access is revoked
- **WHEN** a connection is disabled or the relevant Project purpose is revoked before subsequent Source or Reporter dispatch
- **THEN** that dispatch is blocked without credential fallback or retargeting and accepted history is preserved

#### Scenario: Internal cards are absent
- **WHEN** automatic intake and reporting operate in a deployment without internal Issues
- **THEN** native source correlation, Work/Run history and reporting remain usable and no synthetic card is created
