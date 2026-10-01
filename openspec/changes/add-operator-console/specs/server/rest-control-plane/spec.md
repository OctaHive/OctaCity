## ADDED Requirements

### Requirement: Project-owned definition discovery
The management REST API SHALL expose authorization-safe, project-scoped, cursor-paginated collections of the current Pipeline, Repository, Build Configuration, and Trigger definition versions. Collection items SHALL contain the stable identity, owning Project identity, current version, operator-facing name or kind, relevant enabled state, and publication time needed to select the exact version through an existing detail endpoint. Trigger discovery SHALL distinguish manual, scheduled, and internal definitions without exposing provider credentials or webhook secrets.

#### Scenario: Client discovers current Project definitions
- **WHEN** a client lists one supported definition collection for a visible Project
- **THEN** the server returns a bounded deterministic page of current visible summaries and an exclusive continuation cursor when another page exists

#### Scenario: Definition has several immutable versions
- **WHEN** a Project-owned definition has multiple published versions
- **THEN** its discovery collection contains only the current version while the existing version-addressed endpoint remains available for exact historical reads

#### Scenario: Project has no definitions of one kind
- **WHEN** the requested Project is visible but has no visible current definitions in that collection
- **THEN** the server returns a valid empty page without requiring a known definition identity

### Requirement: Project Build discovery
The management REST API SHALL expose an authorization-safe, project-scoped, cursor-paginated Build collection ordered deterministically from newest to oldest. A client SHALL be able to filter the collection by exact Build Configuration identity and Build state. Each summary SHALL identify the Build, owning Project, selected Build Configuration and version, creation cause and time, current Attempt identity, number and state, and terminal time or outcome when available without embedding the complete Attempt DAG or event history.

#### Scenario: Client lists recent Project Builds
- **WHEN** a client requests the first bounded Build page for a visible Project
- **THEN** the server returns the newest visible summaries in deterministic order and a cursor that continues strictly after the last returned Build

#### Scenario: Client filters by configuration and state
- **WHEN** a client supplies a valid Build Configuration identity and Build state belonging to the selected Project
- **THEN** every returned summary matches both filters and pagination is computed over only the matching visible Builds

#### Scenario: Hidden Builds surround a page boundary
- **WHEN** non-visible Builds sort before, within, or after the requested page
- **THEN** returned items, ordering, page size, and continuation cursor are derived only from visible matching Builds

#### Scenario: Existing client reads one Build
- **WHEN** a client continues to use the existing Build detail endpoint
- **THEN** its request and representation remain compatible and do not require use of the discovery collection
