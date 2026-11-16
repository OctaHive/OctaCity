import { expect } from '@playwright/test';
import { test } from './sourceTest';
import { retainJsonEvidence } from './retainedEvidence';
import { choose, selectedNode, setChecked, t } from './manualAuthoringControls';
import {
  configuration,
  deliveryAssignments,
  deliveryCode,
  executor,
  projectFields,
  publish,
  read,
  triage,
} from './issue-delivery-authoring-controls';

const spki = process.env.OCTACITY_RELEASE_TLS_SPKI;
test.use({
  launchOptions: { args: spki ? [`--ignore-certificate-errors-spki-list=${spki}`] : [] },
});

test('authors triage and Delivery through the ordinary stand Console without API pipeline setup', async ({
  page,
}) => {
  test.setTimeout(900_000);
  page.setDefaultTimeout(20_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const revision = process.env.OCTACITY_RELEASE_CODE_REVISION!;
  const path = process.env.OCTACITY_RELEASE_DELIVERY_FILE!;
  const repository = process.env.OCTACITY_RELEASE_SOURCE_REPOSITORY!;
  const directory = process.env.OCTACITY_RELEASE_DELIVERY_DIRECTORY!;
  expect(project).toBeTruthy();
  expect(revision).toBeTruthy();
  expect(path).toBeTruthy();
  expect(repository).toBeTruthy();
  expect(directory).toBeTruthy();
  await page.addInitScript(() =>
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        version: 2,
        language: 'en',
        theme: 'light',
        favorites: [],
        recents: [],
        expanded: [],
        explorerOpen: false,
        explorerWidth: 288,
        notificationLastOpenedAt: null,
      }),
    ),
  );
  // The driver may observe the real server, but never seeds business definitions.
  for (const collection of ['flow-definitions', 'factory-configurations', 'factory-runs'])
    expect((await read(page, project, `${collection}?limit=100`)).items).toEqual([]);
  await page.goto(`/projects/${project}`);
  const capabilities = await read(page, project, 'factory-authoring-capabilities');
  expect(capabilities.issue_classification?.plugin.identity).toBe('stand.issue-fields');
  await projectFields(page, project);
  const triageTarget = await triage(page, project);

  await page.goto(`/projects/${project}/issues`);
  await page.getByRole('button', { name: 'Create task', exact: true }).click();
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill('UI pipeline qualification');
  await page
    .getByRole('textbox', { name: 'Description', exact: true })
    .fill(
      `Add ${path} containing the single line octacity-ui-delivery. This is a small feature request for the Console component, priority low. Keep the change within the approved directory.`,
    );
  await page
    .getByRole('textbox', { name: 'Acceptance criteria', exact: true })
    .fill(
      `${path} exists with the exact UTF-8 content octacity-ui-delivery followed by a newline. Independently verify the file content.`,
    );
  await page.getByRole('button', { name: 'Review task', exact: true }).click();
  const created = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === `/api/v1/projects/${project}/issues`,
  );
  await page.getByRole('dialog').getByRole('button', { name: 'Create task', exact: true }).click();
  const issue = (await (await created).json()).resource;
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  await launch(triageTarget, false);
  const before = (await read(page, project, `issues/${issue.id}/executions`)).items;
  expect(before).toHaveLength(1);
  await settle(before[0].association.run_id);
  const triageCompletions = await (
    await page.request.get(`/api/v1/factory-runs/${before[0].association.run_id}/node-completions`)
  ).json();
  expect(triageCompletions.items).toHaveLength(1);
  const saved = triageCompletions.items[0].record;
  expect(saved).toMatchObject({
    node_key: 'assess',
    outcome: 'classified',
    issue_fields: { status: 'applied' },
  });
  expect(saved.issue_fields.delivery_record).toMatch(/^[0-9a-f]{64}$/);
  const classified = await read(page, project, `issues/${issue.id}`);
  expect(classified.content.priority).toBe('low');
  expect(classified.classification.fields).toHaveLength(8);
  for (const field of classified.classification.fields) {
    expect(field.origin).toMatchObject({
      kind: 'pipeline',
      run_id: before[0].association.run_id,
      record: saved.output_digest,
      operation_id: saved.issue_fields.operation_id,
      invocation: { node: 'assess', attempt_id: saved.node_attempt_id },
    });
    expect(field.origin.form).toMatch(/^[0-9a-f]{64}$/);
  }
  expect(classified.planning).toMatchObject({
    component: 'console',
    type: 'feature_request',
    size: 'small',
  });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: t('factory.addPipeline'), exact: true }).click();
  await page.getByText('Start from an example', { exact: true }).click();
  const opened = page.waitForResponse((response) =>
    new URL(response.url()).pathname.endsWith('/factory-presets/draft'),
  );
  await page.getByRole('button', { name: 'Ticket delivery · v2', exact: true }).click();
  const openedResponse = await opened;
  expect(openedResponse.ok(), await openedResponse.text()).toBe(true);
  const draft = await openedResponse.json();
  await expect(
    page.getByRole('textbox', { name: t('factory.pipelineName'), exact: true }),
  ).toHaveValue('Ticket delivery');
  await page.getByRole('button', { name: 'Configure Issue snapshot', exact: true }).click();
  for (const [alias, field] of [
    ['risk', 'Delivery risk'],
    ['severity', 'Delivery severity'],
    ['recommended_route', 'Delivery route'],
    ['reproducibility', 'Delivery reproducibility'],
  ])
    await choose(page, page, `Issue input field: ${alias}`, `Project field: ${field}`);
  await setChecked(
    page.getByRole('checkbox', { name: 'Map Issue fields to admission', exact: true }),
  );
  await choose(page, page, 'Admission priority field', 'Priority · priority');
  for (const [value, score] of [
    ['blocker', '100'],
    ['high', '80'],
    ['medium', '50'],
    ['low', '20'],
  ])
    await page
      .getByRole('spinbutton', { name: `Scheduling priority: ${value}`, exact: true })
      .fill(score!);
  await choose(page, page, 'Admission risk field', 'Project field: Delivery risk · risk');
  await choose(page, page, 'Admission risk: low', 'Low');
  await setChecked(
    page.getByRole('checkbox', { name: 'Allow work_kind: feature_request', exact: true }),
  );
  await page.getByRole('button', { name: 'Use Issue snapshot', exact: true }).click();
  for (const step of draft.root.nodes) {
    if (step.kind !== 'reasoning') continue;
    await choose(page, page, t('factory.graphNode'), step.key);
    const [role, profile] = ['implementation', 'specification_author'].includes(step.key)
      ? ['Coder', 'Coder · Delivery']
      : step.key === 'protected_tests'
        ? ['Tester', 'Tester · Delivery']
        : step.key === 'prepare_pull_request'
          ? ['DevOps', 'DevOps · Delivery']
          : ['Reviewer', 'Reviewer · Delivery'];
    await executor(
      page,
      selectedNode(page),
      role!,
      profile!,
      step.key === 'protected_tests' ? 'Prepare protected tests' : undefined,
    );
  }
  for (const step of draft.root.nodes.filter(
    (step: { kind: string }) => step.kind === 'build_command',
  )) {
    await choose(page, page, t('factory.graphNode'), step.key);
    const scope = selectedNode(page);
    await scope.getByRole('combobox', { name: 'Installed command', exact: true }).click();
    const report = step.key === 'tests_on_base' ? 'baseline.report' : 'validation_report';
    const options = page.getByRole('option').filter({ hasText: ` · ${report} · ` });
    await expect(options).toHaveCount(1);
    await options.click();
    await scope.getByRole('button', { name: 'Use command', exact: true }).click();
  }
  await choose(page, page, t('factory.graphNode'), 'publish_pull_request');
  const publisher = selectedNode(page);
  const target = capabilities.candidate_publications[0].target;
  await choose(
    page,
    publisher,
    t('factory.publicationTarget'),
    `${target.identity} · ${target.version}`,
  );
  await publisher
    .getByRole('button', { name: t('factory.usePublicationTarget'), exact: true })
    .click();
  await choose(page, page, t('factory.graphNode'), 'requirements_needed');
  await choose(page, selectedNode(page), 'Choice field', 'size');
  await choose(page, selectedNode(page), 'Starting choice: size · Requirements needed', 'large');
  await selectedNode(page)
    .getByRole('button', { name: 'Validate and use condition', exact: true })
    .click();
  await page
    .getByRole('textbox', { name: t('factory.pipelineName'), exact: true })
    .fill('UI Delivery');
  await deliveryCode(page, revision, directory);
  await deliveryAssignments(page, directory);
  await publish(page);
  const deliveryTarget = await configuration(page, project, 'UI Delivery', 'Delivery');
  const flow = await (
    await page.request.get(`/api/v1/factory-configurations/${deliveryTarget.id}/versions/1`)
  ).json();
  expect(flow.configuration.flow.accepted_predecessor).toBeUndefined();
  expect(flow.configuration.flow.issue_snapshot.admission.allowed.work_kind).toEqual([
    'feature_request',
  ]);
  expect(
    flow.configuration.flow.closure.definitions.flatMap((root: { nodes: unknown[] }) => root.nodes)
      .length,
  ).toBeGreaterThan(5);
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  await launch(deliveryTarget, true);
  const executions = (await read(page, project, `issues/${issue.id}/executions`)).items;
  expect(executions).toHaveLength(2);
  const delivery = executions.find(
    (execution: { association?: { run_id: string } }) =>
      execution.association?.run_id !== before[0].association.run_id,
  );
  expect(delivery).toBeTruthy();
  const snapshot = await read(
    page,
    project,
    `issues/${issue.id}/executions/${delivery.id}/snapshot`,
  );
  expect(snapshot.snapshot.values).toMatchObject({
    priority: 'low',
    component: 'console',
    work_kind: 'feature_request',
    size: 'small',
  });
  expect(snapshot.snapshot.attribution).toBeTruthy();
  for (const field of Object.values(snapshot.snapshot.attribution)) {
    expect(field).toMatchObject({
      origin: { kind: 'pipeline', run_id: before[0].association.run_id },
    });
  }
  const run = delivery.association.run_id;
  const admitted = await (await page.request.get(`/api/v1/factory-runs/${run}`)).json();
  expect(admitted).toMatchObject({
    configuration_id: deliveryTarget.id,
    configuration_version: deliveryTarget.version,
  });
  for (const binding of ['admission_policy', 'permission_ceiling']) {
    expect(flow.configuration[binding].project_policy).toMatchObject({
      project_id: project,
      version: expect.any(Number),
      digest: expect.stringMatching(/^[0-9a-f]{64}$/),
    });
  }
  await settle(run);
  const after = await read(page, project, `issues/${issue.id}/executions/${delivery.id}/snapshot`);
  expect(after.snapshot).toEqual(snapshot.snapshot);
  const retained = await (
    await page.request.get(`/api/v1/factory-configurations/${deliveryTarget.id}/versions/1`)
  ).json();
  expect(retained).toEqual(flow);
  const completions = await (
    await page.request.get(`/api/v1/factory-runs/${run}/node-completions`)
  ).json();
  // Retain publication/guard evidence before assertions or disposable-stand cleanup.
  await retainJsonEvidence(test.info(), 'delivery-records', {
    run,
    completions,
    field_save: saved,
    classification: classified.classification,
    snapshot,
    configuration: flow,
  });
  const nodes = completions.items.map(
    (entry: { record: { node_key: string } }) => entry.record.node_key,
  );
  for (const required of [
    'protected_tests',
    'implementation',
    'review',
    'protected_validation',
    'publish_pull_request',
  ])
    expect(nodes).toContain(required);
  expect(completions.items).toEqual(
    expect.arrayContaining([
      expect.objectContaining({
        record: expect.objectContaining({ node_key: 'publish_pull_request', outcome: 'published' }),
      }),
    ]),
  );
  const builds = await (
    await page.request.get(`/api/v1/factory-runs/${run}/builds?limit=100`)
  ).json();
  const independentBuilds = [
    'protected_tests',
    'implementation',
    'review',
    'protected_validation',
  ].map((key) => {
    const completion = completions.items.find(
      (entry: { record: { node_key: string } }) => entry.record.node_key === key,
    );
    const build = builds.items.find(
      (entry: { record: { node_attempt_id: string } }) =>
        entry.record.node_attempt_id === completion.record.node_attempt_id,
    );
    expect(build, key).toBeTruthy();
    return build.record.build_id;
  });
  expect(new Set(independentBuilds).size).toBe(4);
  const validation = completions.items.find(
    (entry: { record: { node_key: string } }) => entry.record.node_key === 'protected_validation',
  );
  expect(validation.record.verified_report).toBeTruthy();
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  const pullRequest = page.getByRole('link', {
    name: t('issueOutcomes.openPullRequest'),
    exact: true,
  });
  await expect(pullRequest).toHaveAttribute('href', /^https:\/\/.+\/pull\/\d+$/);
  const reviewUrl = new URL((await pullRequest.getAttribute('href'))!);
  const sourceUrl = new URL(repository.replace(/\.git$/, ''));
  expect(reviewUrl.origin).toBe(sourceUrl.origin);
  expect(reviewUrl.pathname.replace(/\/pull\/\d+$/, '')).toBe(sourceUrl.pathname);
  await retainJsonEvidence(test.info(), 'confirmed-delivery', {
    run,
    url: await pullRequest.getAttribute('href'),
    completions,
    field_save: saved,
    classification: classified.classification,
    snapshot,
    configuration: flow,
    builds,
  });

  async function settle(run: string) {
    let terminal: { state: string; diagnostics?: unknown } | undefined;
    await expect
      .poll(
        async () => {
          const response = await page.request.get(`/api/v1/factory-runs/${run}`);
          expect(response.ok()).toBe(true);
          terminal = await response.json();
          return terminal?.state;
        },
        { timeout: 480_000 },
      )
      .toMatch(/^(completed|stopped|cancelled)$/);
    await test.info().attach(`run-${run}`, {
      body: Buffer.from(JSON.stringify(terminal)),
      contentType: 'application/json',
    });
    if (terminal?.state !== 'completed') {
      for (const collection of ['node-attempts', 'node-stops', 'node-completions', 'builds']) {
        const response = await page.request.get(
          `/api/v1/factory-runs/${run}/${collection}?limit=100`,
        );
        const body = await response.text();
        await test.info().attach(`${run}-${collection}`, {
          body: Buffer.from(body),
          contentType: 'application/json',
        });
        if (collection !== 'builds' || !response.ok()) continue;
        for (const item of JSON.parse(body).items) {
          const build = await (
            await page.request.get(`/api/v1/builds/${item.record.build_id}`)
          ).json();
          if (!build.current_attempt?.id) continue;
          const attempt = await (
            await page.request.get(`/api/v1/attempts/${build.current_attempt.id}`)
          ).json();
          for (const job of attempt.jobs ?? []) {
            const events = await page.request.get(
              `/api/v1/jobs/${job.id}/events?after=0&limit=100&wait_ms=0`,
            );
            await test.info().attach(`job-${job.id}-events`, {
              body: Buffer.from(await events.text()),
              contentType: 'application/json',
            });
          }
        }
      }
    }
    expect(terminal?.state, JSON.stringify(terminal)).toBe('completed');
  }

  async function launch(target: { id: string; version: number }, snapshot: boolean) {
    await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
    await choose(
      page,
      page,
      'Factory Configurations',
      `Factory Configuration · v${target.version} · ${target.id}`,
    );
    await choose(page, page, 'Source', 'Exact revision');
    await page.getByRole('textbox', { name: 'Exact revision', exact: true }).fill(revision);
    await expect(
      page.getByRole('combobox', { name: t('issues.predecessorRun'), exact: true }),
    ).toHaveCount(0);
    if (!snapshot)
      await page.getByRole('spinbutton', { name: 'Work priority', exact: true }).fill('20');
    await page.getByRole('button', { name: 'Review pipeline launch', exact: true }).click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: 'Confirm pipeline launch', exact: true })
      .click();
    await expect(page.getByRole('dialog')).toContainText('Work and Run admitted.');
    await page.getByRole('dialog').getByRole('button', { name: 'Close', exact: true }).click();
  }
});
