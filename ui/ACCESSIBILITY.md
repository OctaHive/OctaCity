# Operator Console Accessibility Verification

The console targets keyboard-operable, data-dense desktop workflows and preserves access at the
supported narrow width. Automated browser checks cover every primary route at 1440 × 900 and
640 × 900. Stable visual snapshots cover every primary route across representative desktop and
narrow layouts, both palettes, and reduced motion.

## Automated checks

From `ui/` with the pinned Node and pnpm versions:

```sh
corepack pnpm test:accessibility
corepack pnpm test:visual
```

The accessibility suite rejects duplicate identities, missing accessible control, graphic, dialog,
and table names, broken `aria-labelledby` references, missing primary landmarks or headings,
invisible keyboard focus, and viewport-level horizontal overflow. It traverses the complete
browser-defined tab order rather than sampling its beginning, and structural names must come from
an explicit ARIA label, labelled reference, caption, or image alternative—not arbitrary descendant
text. Component tests separately prove that the Attempt SVG and dependency table expose the same
Jobs, states, and dependency relations.

## Manual keyboard journeys

Run these journeys in both light and dark themes. Repeat the narrow-layout steps at 640 pixels and
enable the operating system's reduced-motion preference for one pass. Do not use a pointing device.

### Shared shell and section navigation

1. Load `/projects`, press `Tab`, and verify the skip link is visibly focused and moves focus to the
   primary content.
2. Continue through global search, readiness, theme, notifications, operator menu, and the four
   large section controls. Verify every focus indicator remains visible and follows visual order.
3. Press `Control+K` or `Command+K`; verify focus enters Search query, cycles within the command
   center, and returns to the invoking control after `Escape`.
4. Collapse the contextual explorer, reopen it from the rail, and verify focus moves to its heading
   and returns to the opener after dismissal.
5. Focus the explorer separator. Verify arrow keys resize in bounded steps, `Home` selects 240 px,
   and `End` selects 480 px without obscuring the detail workbench.

### Projects: `/projects` and `/projects/:projectId`

1. Expand a Project with children and activate a leaf Project. Verify leaf Projects have no empty
   disclosure control and the selected link is announced as current.
2. Follow the breadcrumb, independently page each definition section, open and close definition
   details, operate Build filters, and activate a recent Build link.
3. Open the manual Build confirmation, cancel it with `Escape`, and verify focus returns to the
   trigger control without submitting a command.

### Builds: `/builds` and `/builds/:buildId`

1. Expand Project, Build Configuration, and Build branches in the explorer and activate a Build.
2. Select Jobs from both the Job controls and the dependency table. Verify state is always announced
   as text and the table exposes the same dependency relationships as the SVG.
3. Tab through event diagnostics, log filters, Artifact downloads, cache diagnostics, retention, and
   available Build commands. Open a confirmation and verify focus containment and restoration.

### Agents: `/agents`, `/agents/:agentId`, `/agent-pools`, and `/agent-pools/:poolId`

1. Expand an Agent Pool, activate an Agent, and verify Pool and Agent selection are announced.
2. Traverse inventory, readiness, assignment, drain, version, and current-execution information.
   Confirm that online/offline, drain, and idle states remain understandable without color.
3. Open drain and pool-reassignment confirmations, cancel them, and verify focus returns to the
   invoking controls. Navigate directly to the Agent Pool detail route and back to the hierarchy.

### Audit: `/audit`

1. Traverse every filter in the contextual explorer, apply a bounded filter, and clear it.
2. Move through the audit table and open fact details. Verify outcome and actor states are readable
   as text and that the evidence disclosure is keyboard-operable.
3. At 640 pixels, open the Audit explorer overlay, verify focus remains inside until dismissal, then
   horizontally scroll the bounded table without creating page-level horizontal overflow.

## Expected reduced-motion behavior

With reduced motion enabled, focus, selection, loading, and stale-data states still change
immediately, but transitions, animations, and smooth scrolling do not convey required information.
