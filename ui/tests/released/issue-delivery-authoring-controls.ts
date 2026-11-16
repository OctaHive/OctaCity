import { expect, type Page, type Locator } from '@playwright/test';
import { choose, selectedNode, setChecked, t } from './manualAuthoringControls';
import { route } from './manualGraphControls';
import { processInput } from './manualPolicyControls';

/** Only read-only observation is permitted outside actual Console controls. */
export async function read(page: Page, project: string, collection: string) {
  const response = await page.request.get(`/api/v1/projects/${project}/${collection}`);
  expect(response.ok(), collection).toBe(true);
  return response.json();
}

export async function projectFields(page: Page, project: string) {
  await page.goto(`/projects/${project}`);
  await page.getByRole('button', { name: 'Issue field settings', exact: true }).click();
  const components = page.getByRole('group', { name: 'Components', exact: true });
  await option(components, 'Console', 'console');
  const fields = page.getByRole('group', { name: 'Custom Issue fields', exact: true });
  for (const [name, values] of [
    ['Delivery risk', ['low']],
    ['Delivery severity', ['low']],
    ['Delivery route', ['development']],
    ['Delivery reproducibility', ['not_applicable']],
  ] as const) {
    await fields.getByRole('textbox', { name: 'New field name', exact: true }).fill(name);
    await choose(page, fields, 'Field type', 'Choice');
    await fields.getByRole('button', { name: 'Add field', exact: true }).click();
    const field = fields.getByRole('group', { name, exact: true });
    for (const value of values) await option(field, value, value);
  }
  await page.getByRole('button', { name: 'Review field definitions', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Publish field definitions', exact: true }).click();
  await expect(dialog).toContainText('Field definitions published.');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
}

async function option(scope: Locator, name: string, id: string) {
  await scope.getByRole('textbox', { name: 'New option name', exact: true }).fill(name);
  await scope.getByRole('textbox', { name: 'New option ID (optional)', exact: true }).fill(id);
  await scope.getByRole('button', { name: 'Add option', exact: true }).click();
}

export async function executor(
  page: Page,
  scope: Locator,
  role: string,
  profile: string,
  mode?: string,
) {
  await choose(page, scope, t('factory.executorRole'), `${role} · v1`);
  await choose(page, scope, t('factory.executorProfile'), `${profile} · v1`);
  if (mode) await choose(page, scope, t('factory.stepExecutionMode'), mode);
  // A single exact installed runtime is selected by the product.
  const resolved = page.waitForResponse((response) =>
    new URL(response.url()).pathname.endsWith('/factory-execution-bindings/resolve'),
  );
  const apply = scope.getByRole('button', { name: t('factory.useExecutorProfile'), exact: true });
  await apply.click();
  expect((await resolved).ok()).toBe(true);
  await expect(apply).toBeEnabled();
}

/** Explicit source requests remain separate from the example and executor profiles. */
export async function deliveryCode(page: Page, revision: string, directory: string) {
  await choose(page, page, t('factory.projectRepository'), 'triage-code · v1');
  await choose(page, page, t('factory.codeSelection'), 'Exact revision');
  const inventory = page.getByRole('group', { name: t('factory.availableCode'), exact: true });
  await inventory.getByRole('textbox', { name: 'Exact revision', exact: true }).fill(revision);
  await inventory.getByRole('button', { name: 'Add code source', exact: true }).click();
  await choose(page, page, 'Source access ceiling · triage-code', 'Read and write');
  await choose(page, inventory, t('factory.codeScopeMode'), t('factory.codeSelectedPaths'));
  await inventory
    .getByRole('textbox', { name: t('factory.codePath'), exact: true })
    .fill(directory);
  for (const key of [
    'implementation',
    'specification_author',
    'protected_tests',
    'tests_on_base',
    'review',
    'protected_validation',
    'validation_classification',
    'publish_pull_request',
  ]) {
    await choose(page, page, t('factory.graphNode'), key);
    const node = selectedNode(page);
    await choose(page, node, t('factory.stepCodeSelection'), 'triage-code · v1');
    if (key === 'implementation')
      await choose(page, node, t('factory.stepCodeAccess'), 'Read and write');
    if (['implementation', 'protected_validation'].includes(key)) {
      const summary = node.getByText(t('factory.candidateCapture'), { exact: true });
      if ((await summary.locator('..').getAttribute('open')) === null) await summary.click();
      const policy = node.getByRole('group', {
        name: t('factory.captureDeliverables'),
        exact: true,
      });
      await policy
        .getByRole('textbox', { name: t('factory.capturePermitted'), exact: true })
        .fill(directory);
      const kinds = policy.getByRole('textbox', { name: t('factory.captureKind'), exact: true });
      for (let index = (await kinds.count()) - 1; index >= 0; index--)
        if ((await kinds.nth(index).inputValue()) !== 'scripts')
          await kinds
            .nth(index)
            .locator('xpath=ancestor::fieldset[1]')
            .getByRole('button', { name: t('factory.removeItem'), exact: true })
            .click();
      await policy
        .getByRole('textbox', { name: t('factory.captureRequired'), exact: true })
        .fill(directory);
    }
    if (
      [
        'review',
        'protected_validation',
        'validation_classification',
        'publish_pull_request',
      ].includes(key)
    ) {
      await choose(page, node, t('factory.stepCodeRevision'), t('factory.codeAcceptedCandidate'));
      await choose(page, node, t('factory.codeCandidateStep'), 'implementation');
      await choose(page, node, t('factory.codeCandidateOutcome'), 'observed');
    }
  }
}

/** Keep the qualification's one-file task within the ordinary installed budgets. */
export async function deliveryAssignments(page: Page, directory: string) {
  for (const [step, instruction] of [
    [
      'protected_tests',
      "Prepare a compact POSIX shell check for the requested file existence and exact UTF-8 bytes including its terminal newline. The task requires only this one file-content assertion. Use the Issue task and acceptance criteria; no repository inventory is needed. Keep the whole check below 8 KiB and write it in one shell invocation. Use the installed mode's assertion protocol. The isolated verifier supplies the candidate source directory as the first argument.",
    ],
    [
      'implementation',
      `Implement the one-file task from admitted Work in /workspace/source. The installed guest provides POSIX shell and Node; rg and Python are unavailable. Add a persistent standalone POSIX regression test at ${directory}/tests/test_ui_delivery_qualification.sh that checks the requested file's exact bytes. Include this public test in the candidate alongside the requested file so independent review can rerun it. Batch the small change and test in one shell invocation: observe the test fail before creating the requested file, write the exact requested bytes, then observe the test pass. No repository inventory or full input/schema dump is needed for this one-file task. Keep the execution compact within the selected Profile budget.`,
    ],
  ] as const) {
    await choose(page, page, t('factory.graphNode'), step);
    const node = selectedNode(page);
    await node
      .getByRole('button', {
        name: t('factory.addInstruction', { label: t('factory.taskInstructions').toLowerCase() }),
        exact: true,
      })
      .click();
    await node
      .getByRole('textbox', { name: `${t('factory.taskInstructions')} 1`, exact: true })
      .fill(instruction);
  }
  await choose(page, page, t('factory.graphNode'), 'publish_pull_request');
  const publisher = selectedNode(page);
  await publisher
    .getByRole('group', { name: 'published', exact: true })
    .getByRole('button', { name: t('factory.editResult'), exact: true })
    .click();
  await choose(page, publisher, t('factory.showOnIssue'), t('issueOutcomes.pullRequest'));
  await publisher.getByRole('button', { name: t('factory.saveResult'), exact: true }).click();
}

export async function publish(page: Page) {
  const access = page.getByRole('button', { name: t('factory.reviewRequiredAccess'), exact: true });
  if ((await access.count()) && (await access.isEnabled())) {
    await access.click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: t('factory.applyToDraft'), exact: true })
      .click();
  }
  await page.getByRole('button', { name: t('factory.validateDraft'), exact: true }).click();
  await expect(
    page.getByRole('button', { name: t('factory.reviewPublication'), exact: true }),
  ).toBeEnabled();
  await page.getByRole('button', { name: t('factory.reviewPublication'), exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: t('factory.confirmPublish'), exact: true }).click();
  await expect(dialog).toContainText(t('factory.published'), { timeout: 20_000 });
  await dialog.getByRole('button', { name: t('common.close'), exact: true }).click();
}

export async function configuration(page: Page, project: string, flow: string, access: string) {
  await page.getByRole('button', { name: t('factory.discardDraft'), exact: true }).click();
  await page.getByRole('button', { name: t('factory.createConfiguration'), exact: true }).click();
  const form = page.getByRole('group', { name: t('factory.configurationDraft'), exact: true });
  await choose(page, form, t('factory.rootFlow'), `${flow} · v1`);
  // Select the published policy versions displayed by the normal installer.
  await form.getByRole('combobox', { name: 'Launch policy', exact: true }).click();
  await page
    .getByRole('option')
    .filter({ hasText: /^Standard · v\d+$/ })
    .click();
  await form.getByRole('combobox', { name: 'Access profile', exact: true }).click();
  await page
    .getByRole('option')
    .filter({ hasText: new RegExp(`^${access} · v\\d+$`) })
    .click();
  await setChecked(form.getByRole('checkbox', { name: t('factory.enabled'), exact: true }));
  await publish(page);
  const items = (await read(page, project, 'factory-configurations?limit=100')).items;
  const flows = (await read(page, project, 'flow-definitions?limit=100')).items;
  const selected = flows.find((item: { flow: { closure: { definitions: { name: string }[] } } }) =>
    item.flow.closure.definitions.some((root) => root.name === flow),
  );
  expect(selected).toBeTruthy();
  // Read each exact record rather than relying on catalog order or UUIDs from a fixture.
  for (const item of items) {
    const detail = await (
      await page.request.get(`/api/v1/factory-configurations/${item.id}/versions/${item.version}`)
    ).json();
    if (detail.configuration.flow.closure.root.id === selected.flow.closure.root.id)
      return detail.configuration.reference;
  }
  throw new Error(`UI did not publish the Configuration for ${flow}`);
}

export async function triage(page: Page, project: string) {
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: t('factory.addPipeline'), exact: true }).click();
  await page
    .getByRole('textbox', { name: t('factory.pipelineName'), exact: true })
    .fill('UI triage');
  await page.getByRole('button', { name: t('factory.addNode'), exact: true }).click();
  const node = selectedNode(page);
  await node.getByRole('textbox', { name: t('factory.nodeName'), exact: true }).fill('assess');
  await choose(page, node, t('factory.nodeKind'), t('factory.kind.reasoning'));
  await node.getByRole('button', { name: t('factory.addResult'), exact: true }).click();
  await node.getByText(t('factory.resultDetails'), { exact: true }).last().click();
  await node
    .getByRole('textbox', { name: t('factory.resultName'), exact: true })
    .fill('classified');
  await node.getByRole('checkbox', { name: 'Fill Issue fields', exact: true }).check();
  for (const name of [
    'Priority',
    'Issue component',
    'Issue type',
    'Issue size',
    'Project field: Delivery risk',
    'Project field: Delivery severity',
    'Project field: Delivery route',
    'Project field: Delivery reproducibility',
  ])
    await node.getByRole('checkbox', { name, exact: true }).check();
  await node.getByRole('button', { name: t('factory.saveResult'), exact: true }).click();
  await executor(page, node, 'Systems analyst', 'Systems analysis');
  await node
    .getByRole('button', {
      name: t('factory.addInstruction', { label: t('factory.taskInstructions').toLowerCase() }),
      exact: true,
    })
    .click();
  await node
    .getByRole('textbox', { name: `${t('factory.taskInstructions')} 1`, exact: true })
    .fill(
      'Classify this small feature request for the Console component. Fill the selected Issue fields with priority low, size small, type feature_request and component console. Fill Delivery risk and Delivery severity with low, Delivery route with development and Delivery reproducibility with not_applicable. These are assessments under the selected Project definitions. Do not claim verified reproduction or change provider data.',
    );
  await setChecked(node.getByRole('checkbox', { name: t('factory.workTask'), exact: true }));
  await page.getByRole('button', { name: t('factory.flowSettings'), exact: true }).click();
  const settings = page.getByRole('group', { name: t('factory.flowSettings'), exact: true });
  await choose(page, settings, t('factory.entry'), 'assess');
  await settings.getByRole('button', { name: t('factory.addTerminal'), exact: true }).click();
  await settings
    .getByRole('textbox', { name: t('factory.terminalName'), exact: true })
    .last()
    .fill('classified');
  await settings
    .getByRole('combobox', { name: t('factory.terminalSchema'), exact: true })
    .last()
    .click();
  await page
    .getByRole('option')
    .filter({ hasText: /^assess\.classified$/ })
    .click();
  await page.getByRole('button', { name: t('factory.flowSettings'), exact: true }).click();
  await route(page, 'assess', 'classified', 'classified', true);
  await processInput(
    page,
    'goals',
    'Assess the selected task and apply only reviewed classification fields to its Issue.',
  );
  await processInput(
    page,
    'criteria',
    'The schema-valid accepted assessment is applied to the associated Issue, or the guarded application fails visibly.',
  );
  await setChecked(
    page.getByRole('checkbox', {
      name: t('factory.criteriaRecipient', { name: 'assess' }),
      exact: true,
    }),
  );
  await publish(page);
  return configuration(page, project, 'UI triage', 'Triage');
}
