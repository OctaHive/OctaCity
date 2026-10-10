# Repository instructions

## Git commits

- Use the Conventional Commits format for every commit:
  `type(optional-scope): concise description`.
- Use an appropriate type such as `feat`, `fix`, `docs`, `refactor`, `test`,
  `build`, `ci`, or `chore`.
- Keep the subject concise, imperative, lowercase, and without a trailing
  period.
- Keep each commit focused on one logical change.
- Before committing, inspect the staged diff and ensure unrelated changes are
  not included.
- Do not create a commit unless the user explicitly asks for one.

Examples:

- `feat: add production deployment workflow`
- `fix(storage): remove board objects after deletion`
- `docs: document manual release process`

## Mandatory TDD

TDD is required across this repository for every new feature, bug fix, and
observable behavior change, including changes to scripts, configuration, and
database migrations. Apply it automatically; the user does not need to request
TDD or invoke a skill for each task. This rule takes precedence over workflows
that implement behavior before writing tests.

Work in small vertical slices:

1. Identify the required behavior from the task or specification and choose its
   public test boundary using existing project contracts.
2. **Red:** write or extend one meaningful behavioral test before changing the
   implementation. Run it and observe failure caused by the missing behavior
   or newly required API. Environment failures do not establish Red.
3. **Green:** implement only enough behavior to pass that test, then run it and
   observe success.
4. Repeat for the next behavior. Run the relevant regression tests and required
   repository checks before declaring the task complete.

Tests must assert independently specified outcomes through public boundaries.
Do not backfill tests after implementation, mirror implementation logic in
assertions, or replace behavioral verification with compilation alone. Include
the test commands and observed Red/Green outcomes in the completion report.

For refactoring without behavior changes, run existing behavioral tests first
and keep them passing. Add characterization tests before modifying any relevant
behavior that lacks coverage. A discovered behavior correction requires its
own Red/Green cycle.

Documentation-only and formatting-only changes have no executable behavior to
drive with TDD; validate them with the appropriate checks and state why TDD is
not applicable. This exemption does not cover executable behavior changes.

If the required test cannot run, restore the test environment or report the
blocker before implementing that behavior. Deviating from this workflow
requires an explicit instruction from the user.
