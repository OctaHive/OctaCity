import { expect, type Locator, type Page } from '@playwright/test';
import { choose, selectedNode, setChecked, t } from './manualAuthoringControls';
import { route } from './manualGraphControls';
import { budget, runBudget } from './manualPolicyControls';
import { executor, publish, read } from './issue-delivery-authoring-controls';
import { reviewIssueSnapshotLaunch } from './issueMaterialChecks';

const names = ['CI note', 'CI disposition', 'CI passed', 'CI failures', 'CI date'];
const ciName = 'Retained build report assessment';
const consumerName = 'Independent report follow-up';

/** Extends the existing packaged authoring journey; every mutation goes through Console controls. */
export async function ciAnalysisJourney(page: Page, phase: string) {
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  page.setDefaultTimeout(15_000);
  const properties = async () => read(page, project, 'issue-property-definitions');
  if (phase === 'fields') {
    await page.goto(`/projects/${project}`);
    await page.getByRole('button', { name: 'Issue field settings', exact: true }).click();
    const custom = page.getByRole('group', { name: 'Custom Issue fields', exact: true });
    for (const [name, type] of [
      ...names.map(
        (name, index) => [name, ['Text', 'Choice', 'Boolean', 'Integer', 'Date'][index]!] as const,
      ),
    ]) {
      await custom.getByRole('textbox', { name: 'New field name', exact: true }).fill(name);
      await choose(page, custom, 'Field type', type);
      await custom.getByRole('button', { name: 'Add field', exact: true }).click();
    }
    const disposition = custom.getByRole('group', { name: 'CI disposition', exact: true });
    for (const id of ['passed', 'failed']) {
      await disposition.getByRole('textbox', { name: 'New option name', exact: true }).fill(id);
      await disposition
        .getByRole('textbox', { name: 'New option ID (optional)', exact: true })
        .fill(id);
      await disposition.getByRole('button', { name: 'Add option', exact: true }).click();
    }
    await page.getByRole('button', { name: 'Review field definitions', exact: true }).click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: 'Publish field definitions', exact: true })
      .click();
    await expect(page.getByRole('dialog')).toContainText('Field definitions published.');
    expect((await properties()).definition.classification ?? {}).toEqual({});
    return;
  }
  const catalog = await properties();
  const fields = catalog.definition.properties as { id: string; name: string }[];
  const ids = Object.fromEntries(fields.map(({ id, name }) => [name, id]));
  if (phase === 'author') {
    for (const collection of ['flow-definitions', 'factory-configurations', 'factory-runs'])
      expect((await read(page, project, `${collection}?limit=100`)).items).toEqual([]);
    await page.goto(`/ai-settings?project_id=${project}&view=agents`);
    await page.getByRole('button', { name: 'Create AI agent', exact: true }).click();
    const agent = page.getByRole('region', { name: t('factory.agentEditor'), exact: true });
    await agent
      .getByRole('textbox', { name: t('factory.agentName'), exact: true })
      .fill('Report interpreter');
    await agent
      .getByRole('textbox', { name: t('factory.sharedInstruction') + ' 1', exact: true })
      .fill('Analyze only the selected retained report; missing reports are not passing tests.');
    await page.getByRole('button', { name: t('factory.reviewAgent'), exact: true }).click();
    await commit(page, t('factory.confirmPublish'), t('factory.published'));
    await page.getByRole('button', { name: 'Report interpreter', exact: true }).click();
    await page.getByRole('button', { name: t('factory.createProfile'), exact: true }).click();
    const profile = page.getByRole('region', { name: t('factory.profileEditor'), exact: true });
    await choose(
      page,
      profile,
      t('factory.profileRuntime'),
      'manual-runtime · codex · finite-model',
    );
    await profile
      .getByRole('textbox', { name: t('factory.profileName'), exact: true })
      .fill('Report reader');
    await page.getByRole('button', { name: t('factory.reviewProfile'), exact: true }).click();
    await commit(page, t('factory.confirmPublish'), t('factory.published'));

    await page.goto(`/factory?project_id=${project}`);
    await page.getByRole('button', { name: 'Add pipeline', exact: true }).click();
    await page.getByRole('textbox', { name: 'Pipeline name', exact: true }).fill(ciName);
    await addStep(page, 'collect', t('factory.kind.build_command'));
    await result(page, 'reported');
    await selectedNode(page)
      .getByRole('combobox', { name: t('factory.installedCommand'), exact: true })
      .click();
    await page.getByRole('option').filter({ hasText: 'ci-report-collector' }).click();
    await selectedNode(page)
      .getByRole('button', { name: t('factory.useCommand'), exact: true })
      .click();
    await stepBudget(page);
    await page.getByRole('button', { name: 'Configure Issue snapshot', exact: true }).click();
    await page.getByRole('checkbox', { name: 'Description', exact: true }).check();
    await page.getByRole('checkbox', { name: 'Required: Description', exact: true }).check();
    await page.getByRole('checkbox', { name: 'collect: Description', exact: true }).check();
    await page.getByRole('button', { name: 'Use Issue snapshot', exact: true }).click();
    await addStep(page, 'disposition', 'Decision Signal');
    await choose(page, selectedNode(page), 'Decision Signal policy', 'ci-disposition · v1');
    await selectedNode(page)
      .getByRole('button', { name: 'Use Decision Signal policy', exact: true })
      .click();
    await connectField(page, 'collect', 'passed', false);
    await stepBudget(page);
    await addStep(page, 'analyze_report', 'AI agent');
    await result(page, 'analyzed', names);
    await executor(page, selectedNode(page), 'Report interpreter', 'Report reader');
    await selectedNode(page)
      .getByRole('checkbox', { name: t('factory.workTask'), exact: true })
      .check();
    for (const field of ['passed', 'status', 'summary', 'failures', 'observed_date'])
      await connectField(page, 'collect', field, true);
    await stepBudget(page);
    // Reopen and rename through the same public controls; generated form identities stay pinned.
    await selectedNode(page).getByRole('button', { name: 'Edit result', exact: true }).click();
    for (const name of names)
      await expect(
        selectedNode(page).getByRole('checkbox', { name: `Project field: ${name}`, exact: true }),
      ).toBeChecked();
    await selectedNode(page).getByText(t('factory.resultDetails'), { exact: true }).click();
    await selectedNode(page)
      .getByRole('textbox', { name: 'Result key', exact: true })
      .fill('reviewed');
    await selectedNode(page)
      .getByRole('textbox', { name: 'Step name', exact: true })
      .fill('interpret_report');
    await selectedNode(page).getByRole('button', { name: 'Save result', exact: true }).click();
    await terminals(page, [
      ['finished', 'interpret_report.reviewed'],
      ['review', 'disposition.review'],
    ]);
    await route(page, 'collect', 'reported', 'disposition');
    await route(page, 'disposition', 'analyze', 'interpret_report');
    await route(page, 'disposition', 'review', 'review', true);
    await route(page, 'interpret_report', 'reviewed', 'finished', true);
    await publish(page);
    await discard(page);
    const ci = await configuration(page, project, ciName);
    for (const status of ['passed', 'failed', 'missing']) {
      await page.goto(`/projects/${project}/issues`);
      await page.getByRole('button', { name: 'Create task', exact: true }).click();
      await page.getByRole('textbox', { name: 'Title', exact: true }).fill(`CI ${status}`);
      await page
        .getByRole('textbox', { name: 'Description', exact: true })
        .fill(JSON.stringify({ mode: status }));
      await page
        .getByRole('textbox', { name: 'Acceptance criteria', exact: true })
        .fill('Record the selected CI assessment; do not turn missing evidence into success.');
      await page.getByRole('button', { name: 'Review task', exact: true }).click();
      const created = page.getByRole('dialog');
      await created.getByRole('button', { name: 'Create task', exact: true }).click();
      await expect(created.getByRole('link', { name: /^Open ISS-/ })).toBeVisible();
      await created.getByRole('button', { name: 'Close', exact: true }).click();
      const issue = (await read(page, project, 'issues?limit=100')).items.find(
        (item: { title: string }) => item.title === `CI ${status}`,
      );
      await launch(page, project, issue.id, ci);
    }
    const flows = (await read(page, project, 'flow-definitions?limit=100')).items;
    const flow = flows[0].flow;
    expect(flow.closure.definitions[0].nodes.map((node: { kind: string }) => node.kind)).toEqual([
      'build_command',
      'decision_signal',
      'reasoning',
    ]);
    const writer = flow.closure.definitions[0].nodes.find(
      (node: { key: string }) => node.key === 'interpret_report',
    );
    expect(
      writer.outcomes[0].issue_fields.form.fields
        .map((row: { field: { id: string } }) => row.field.id)
        .sort(),
    ).toEqual(names.map((name) => ids[name]).sort());
    expect(writer.outcomes[0].issue_fields.form.definition_version).toBe(catalog.version);
    return;
  }
  expect(phase).toBe('verify');
  const issues = (await read(page, project, 'issues?limit=100')).items;
  const origins = new Map<string, string>();
  for (const status of ['passed', 'failed', 'missing']) {
    const item = issues.find((issue: { title: string }) => issue.title === `CI ${status}`);
    await expect(async () => {
      const issue = await read(page, project, `issues/${item.id}`);
      const execution = (await read(page, project, `issues/${item.id}/executions`)).items[0];
      const run = await (
        await page.request.get(`/api/v1/factory-runs/${execution.association.run_id}`)
      ).json();
      expect(run.state).toBe(status === 'missing' ? 'stopped' : 'completed');
      if (status === 'missing') expect(issue.classification?.fields ?? []).toEqual([]);
      else expect(issue.classification.fields).toHaveLength(5);
    }).toPass({ timeout: 240_000 });
    const execution = (await read(page, project, `issues/${item.id}/executions`)).items[0];
    origins.set(status, execution.association.run_id);
    const records = (
      await (
        await page.request.get(
          `/api/v1/factory-runs/${execution.association.run_id}/node-completions?limit=100`,
        )
      ).json()
    ).items;
    if (status === 'missing') {
      const stops = await (
        await page.request.get(
          `/api/v1/factory-runs/${execution.association.run_id}/node-stops?limit=100`,
        )
      ).json();
      expect(stops.items).toHaveLength(1);
      expect(stops.items[0].record.reason).toEqual({ kind: 'execution_failed' });
      expect(
        records.some(
          (row: { record: { node_key: string } }) => row.record.node_key === 'interpret_report',
        ),
      ).toBe(false);
      continue;
    }
    expect(
      records.find(
        (row: { record: { node_key: string } }) => row.record.node_key === 'disposition',
      ),
    ).toMatchObject({ record: { outcome: 'analyze' } });
    const accepted = records.find(
      (row: { record: { node_key: string } }) => row.record.node_key === 'interpret_report',
    );
    const issue = await read(page, project, `issues/${item.id}`);
    for (const field of issue.classification.fields) {
      expect(field.origin).toMatchObject({
        kind: 'pipeline',
        run_id: execution.association.run_id,
        record: accepted.record.output_digest,
      });
      expect(field.origin.operation_id).toBe(accepted.record.issue_fields.operation_id);
      expect(field.origin.form).toMatch(/^[0-9a-f]{64}$/);
      expect(field.origin.invocation).toMatchObject({
        node: 'interpret_report',
        attempt_id: accepted.record.node_attempt_id,
      });
    }
  }
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Add pipeline', exact: true }).click();
  await page.getByRole('textbox', { name: 'Pipeline name', exact: true }).fill(consumerName);
  await addStep(page, 'observe_fields', t('factory.kind.human_gate'));
  await result(page, 'observed');
  await choose(page, selectedNode(page), t('factory.humanPurpose'), t('factory.humanReview'));
  await choose(
    page,
    selectedNode(page),
    t('factory.humanResponseOutcome', { response: t('factory.humanAcknowledge') }),
    'observed',
  );
  await selectedNode(page)
    .getByRole('button', { name: t('factory.useHumanDecision'), exact: true })
    .click();
  await stepBudget(page);
  await page.getByRole('button', { name: 'Configure Issue snapshot', exact: true }).click();
  for (const name of names) {
    await page.getByRole('checkbox', { name: `Project field: ${name}`, exact: true }).check();
    await page
      .getByRole('checkbox', { name: `Required: Project field: ${name}`, exact: true })
      .check();
    await page
      .getByRole('checkbox', { name: `observe_fields: Project field: ${name}`, exact: true })
      .check();
  }
  await page.getByRole('button', { name: 'Use Issue snapshot', exact: true }).click();
  await terminals(page, [['finished', 'observe_fields.observed']]);
  await route(page, 'observe_fields', 'observed', 'finished', true);
  await publish(page);
  await discard(page);
  const consumer = await configuration(page, project, consumerName);
  for (const status of ['passed', 'failed']) {
    const item = issues.find((issue: { title: string }) => issue.title === `CI ${status}`);
    await launch(page, project, item.id, consumer);
    const executions = (await read(page, project, `issues/${item.id}/executions`)).items;
    expect(executions).toHaveLength(2);
    const downstream = executions.find(
      (entry: { intent: { target: { configuration_id: string } } }) =>
        entry.intent.target.configuration_id === consumer.id,
    );
    expect(downstream).toBeTruthy();
    const snapshot = await read(
      page,
      project,
      `issues/${item.id}/executions/${downstream.id}/snapshot`,
    );
    expect(snapshot.snapshot.values).toMatchObject({
      [ids['CI note']!]: `${status} report`,
      [ids['CI disposition']!]: status,
      [ids['CI passed']!]: status === 'passed',
      [ids['CI failures']!]: status === 'passed' ? 0 : 2,
      [ids['CI date']!]: '2026-11-16',
    });
    for (const field of Object.values(snapshot.snapshot.attribution))
      expect(field).toMatchObject({ origin: { kind: 'pipeline', run_id: origins.get(status) } });
  }
}

async function commit(page: Page, button: string, message: string) {
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: button, exact: true }).click();
  await expect(dialog).toContainText(message);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
}
async function addStep(page: Page, name: string, kind: string) {
  await page.getByRole('button', { name: 'Add step', exact: true }).click();
  await selectedNode(page).getByRole('textbox', { name: 'Step name', exact: true }).fill(name);
  await choose(page, selectedNode(page), 'Step type', kind);
}
async function result(page: Page, key: string, fill: string[] = []) {
  await selectedNode(page).getByRole('button', { name: 'Add result', exact: true }).click();
  await selectedNode(page).getByText(t('factory.resultDetails'), { exact: true }).last().click();
  await selectedNode(page).getByRole('textbox', { name: 'Result key', exact: true }).fill(key);
  if (fill.length) {
    await selectedNode(page)
      .getByRole('checkbox', { name: 'Fill Issue fields', exact: true })
      .check();
    for (const name of fill)
      await selectedNode(page)
        .getByRole('checkbox', { name: `Project field: ${name}`, exact: true })
        .check();
  }
  await selectedNode(page).getByRole('button', { name: 'Save result', exact: true }).click();
}
async function connectField(page: Page, producer: string, field: string, add: boolean) {
  const inputs = selectedNode(page).getByRole('group', { name: 'Accepted results', exact: true });
  await choose(page, inputs, 'Result from step', producer);
  await choose(page, inputs, 'Result field', field);
  await choose(
    page,
    inputs,
    'Step input field',
    add ? t('factory.newResultInput') : t('factory.entireTypedValue'),
  );
  if (add)
    await inputs
      .getByRole('textbox', { name: t('factory.resultInputName'), exact: true })
      .fill(field);
  await inputs.getByRole('button', { name: 'Use selected field', exact: true }).click();
  await expect(
    inputs.getByRole('button', {
      name: t('factory.disconnectAcceptedField', {
        name: add ? field : t('factory.entireTypedValue'),
      }),
      exact: true,
    }),
  ).toBeVisible();
}
async function setBudget(page: Page, scope: Locator, label: string, value = runBudget) {
  const section = scope.locator('[data-factory-budget]').filter({
    has: page.getByRole('combobox', {
      name: `${label} · ${t('factory.calculation')}`,
      exact: true,
    }),
  });
  await choose(page, section, `${label} · ${t('factory.calculation')}`, t('factory.setManually'));
  await budget(section, label, value);
}
async function stepBudget(page: Page) {
  await setBudget(page, selectedNode(page), t('factory.nodeBudget'), {
    max_attempts: 1,
    max_elapsed_millis: 120000,
    max_tokens: 1000,
    max_cost_micro_units: 1000,
    max_output_bytes: 16 * 1024 * 1024,
  });
}
async function terminals(page: Page, entries: string[][]) {
  await page.getByRole('button', { name: 'Flow settings', exact: true }).click();
  const settings = page.getByRole('group', { name: 'Flow settings', exact: true });
  await setBudget(page, settings, t('factory.executionBudget'));
  await settings.getByText(t('factory.admissionLimitsDetails'), { exact: true }).click();
  await setBudget(page, settings, t('factory.admissionBudget'));
  for (const [name, result] of entries) {
    await settings.getByRole('button', { name: t('factory.addTerminal'), exact: true }).click();
    await settings
      .getByRole('textbox', { name: t('factory.terminalName'), exact: true })
      .last()
      .fill(name!);
    await settings
      .getByRole('combobox', { name: t('factory.terminalSchema'), exact: true })
      .last()
      .click();
    await page.getByRole('option', { name: new RegExp(result!.replaceAll('.', '\\.')) }).click();
  }
  await page.getByRole('button', { name: 'Flow settings', exact: true }).click();
}
async function discard(page: Page) {
  await page.getByRole('button', { name: 'Discard draft', exact: true }).click();
}
async function configuration(page: Page, project: string, name: string) {
  await page.getByRole('button', { name: 'Create configuration draft', exact: true }).click();
  const form = page.getByRole('group', { name: t('factory.configurationDraft'), exact: true });
  await choose(page, form, t('factory.rootFlow'), `${name} · v1`);
  await choose(page, form, 'Launch policy', 'Standard · v1');
  await choose(page, form, 'Access profile', 'Standard · v1');
  await setChecked(form.getByRole('checkbox', { name: 'Enabled', exact: true }));
  await form.getByRole('spinbutton', { name: 'Maximum active Runs', exact: true }).fill('3');
  await publish(page);
  await discard(page);
  const flows = (await read(page, project, 'flow-definitions?limit=100')).items;
  const flow = flows.find((entry: { flow: { closure: { definitions: { name: string }[] } } }) =>
    entry.flow.closure.definitions.some((root) => root.name === name),
  );
  const configurations = (await read(page, project, 'factory-configurations?limit=100')).items;
  for (const entry of configurations) {
    const detail = await (
      await page.request.get(`/api/v1/factory-configurations/${entry.id}/versions/${entry.version}`)
    ).json();
    if (detail.configuration.flow.closure.root.id === flow.flow.closure.root.id)
      return detail.configuration.reference;
  }
  throw new Error('Console did not publish the selected configuration');
}
async function launch(
  page: Page,
  project: string,
  issue: string,
  target: { id: string; version: number },
) {
  await page.goto(`/projects/${project}/issues/${issue}`);
  await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
  await choose(
    page,
    page,
    'Factory Configurations',
    `Factory Configuration · v${target.version} · ${target.id}`,
  );
  await choose(page, page, 'Source', 'Exact revision');
  await page
    .getByRole('textbox', { name: 'Exact revision', exact: true })
    .fill(process.env.OCTACITY_RELEASE_CODE_REVISION!);
  const before = (await read(page, project, `issues/${issue}/executions`)).items.length;
  const dialog = await reviewIssueSnapshotLaunch(page);
  const submitted = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === `/api/v1/projects/${project}/issues/${issue}/executions`,
  );
  await dialog.getByRole('button', { name: 'Confirm pipeline launch', exact: true }).click();
  expect((await submitted).ok()).toBe(true);
  await expect
    .poll(
      async () =>
        (await read(page, project, `issues/${issue}/executions`)).items.filter(
          (entry: { association: unknown }) => entry.association,
        ).length,
    )
    .toBe(before + 1);
}
