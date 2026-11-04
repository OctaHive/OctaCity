# Ticket delivery reproduction branch

`reproduction-v1.json` is an editable Flow manifest for the reproduction branch.
Resolve its parts relative to `config/factory` through the ordinary manifest
loader. The branch accepts only `work_kind` and `reproducibility` from the exact
accepted triage result. Select those fields explicitly in the parent call.
When placing the nodes directly in a root Flow, use the same projection from
the pinned incoming triage schema instead of the caller input.

Bind `reproduction` to the exact installed Tester command and review its
permissions and budget. Add its canonical `observation_contract.report_schema()`
under the `reproduction_report` schema choice and select that same report as
the command's primary outcome. The observation schema is reserved for
commands with an installed observation criterion. The command can read the original
Work task at `/octacity/protected/work-task`. Select any required code access
for this step. The installed command pins the symptom criterion and environment
and records bounded checks against the admitted subject.

By default an indeterminate report or a triage assessment that requires more
input leaves the Flow waiting for an operator at `review-reproduction`. Dismissing that step ends
the branch as `unresolved` without a Reporter effect. If the installed command
sets `stop_on_indeterminate`, the Flow stops at the command instead and retains
the report under the Build. An explicit retry uses the ordinary technical
retry contract.

Continue to size routing from `bypass` or `reproduced`. Only the latter carries
a new reproduction report. The existing triage predecessor binding excludes
review and missing-input outcomes. `cannot_reproduce` ends the branch before
any implementation work.

For optional external effects, use `reproduction-effects-v1.json` as the
`outcomes` selection of the ordinary result reporting composer for
`reproduction-result`. Choose an existing writer and replace the label
strings with the installation's existing managed labels when needed. Include
the exact reporting child and schemas in normal publication. The negative
finding requests adding the mapped `can't reproduce` label, removing the
managed `need reproduce` label and keeping the ticket open. The positive
finding only requests removing the managed label. Reporter preserves other
labels, and the accepted Flow result remains separate from delivery receipts.
