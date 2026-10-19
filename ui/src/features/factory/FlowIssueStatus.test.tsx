// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { readFileSync } from 'node:fs';
import type { WorkflowResource } from '../../api/issueWorkflows';
import { createConsoleQueryClient } from '../../app/query';
import { issueWorkflowsApi } from '../../api/issueWorkflows';
import { FlowGraphEditor } from './FlowGraphEditor';
import { emptyFlowDraft } from './emptyFlowDraft';

afterEach(cleanup);
function localWorkflow(projectId: string): WorkflowResource {
  const workflow: WorkflowResource = {
    id: '20000000-0000-4000-8000-000000000001',
    project_id: projectId,
    version: 3,
    published_at_unix_ms: 1700000000000,
    definition: JSON.parse(
      readFileSync(
        '../server/api/octacity-server-api-rest/fixtures/v1/issue-workflow-definition.json',
        'utf8',
      ),
    ),
  };
  workflow.definition.transitions = [
    {
      id: '30000000-0000-4000-8000-000000000001',
      name: 'Accept delivery',
      from: workflow.definition.initial_status_id,
      to: workflow.definition.terminal_status_ids[0]!,
      origins: ['pipeline'],
      evidence: ['accepted_execution', 'passed_required_checks'],
    },
    {
      id: '30000000-0000-4000-8000-000000000002',
      name: 'Operator only',
      from: workflow.definition.initial_status_id,
      to: workflow.definition.terminal_status_ids[1]!,
      origins: ['manual'],
      evidence: [],
    },
  ];
  return workflow;
}

it('authors an exact local workflow action without a source connection', async () => {
  const projectId = '11111111-1111-4111-8111-111111111111';
  const workflow = localWorkflow(projectId);
  const flow = configurationFixture().configuration.flow;
  let observed = emptyFlowDraft();
  observed.root = structuredClone(flow.closure.definitions[0]!);
  observed.data_schemas = structuredClone(flow.data_schemas);
  const node = observed.root.nodes.find((item) => item.action)!;
  node.action = {
    ...node.action!,
    action: 'transition_issue',
    parameters: {
      workflow_id: workflow.id,
      workflow_version: 3,
      transition_id: workflow.definition.transitions[0]!.id,
      outcome: 'succeeded',
    },
  };
  observed.root.entry = node.key;
  const routes = structuredClone(observed.root.transitions);
  const api = {
    ...issueWorkflowsApi,
    list: vi.fn(async () => ({
      items: [{ ...workflow, name: workflow.definition.name, version: workflow.version }],
      next_cursor: null,
    })),
    get: vi.fn(async () => workflow),
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        issueStatus={{ api, projectId }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('heading', { name: 'Local Issue status action' })).toBeTruthy();
  await userEvent.click(await screen.findByRole('combobox', { name: 'Issue workflow' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Local workflow · v3' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Issue transition' }));
  expect(screen.queryByRole('option', { name: /Operator only/ })).toBeNull();
  await userEvent.click(screen.getByRole('option', { name: /Accept delivery/ }));
  expect(screen.getByText(/Accepted execution/)).toBeTruthy();
  expect(screen.getByText(/Required checks passed/)).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed Issue action' }));
  expect(observed.root.nodes.find((item) => item.key === node.key)!.action!.parameters).toEqual({
    workflow_id: workflow.id,
    workflow_version: 3,
    transition_id: workflow.definition.transitions[0]!.id,
    outcome: 'succeeded',
  });
  expect(observed.root.transitions).toEqual(routes);
  expect(screen.queryByRole('combobox', { name: 'Source connection' })).toBeNull();
});

it('allows explicit retained workflow version review without silently following the latest version', async () => {
  const workflow = localWorkflow('project');
  const flow = configurationFixture().configuration.flow;
  const node = flow.closure.definitions[0]!.nodes.find((item) => item.action)!;
  node.action = { ...node.action!, action: 'transition_issue', parameters: {} };
  const get = vi.fn(async (_project: string, _id: string, version?: number) => ({
    ...workflow,
    version: version ?? 3,
  }));
  const api = {
    ...issueWorkflowsApi,
    list: vi.fn(async () => ({
      items: [{ ...workflow, name: 'Local workflow', version: 3 }],
      next_cursor: null,
    })),
    get,
  };
  const onChange = vi.fn();
  const { FlowIssueStatus } = await import('./FlowIssueStatus');
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueStatus node={node} services={{ api, projectId: 'project' }} onChange={onChange} />
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Issue workflow' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Local workflow · v3' }));
  const version = await screen.findByRole('spinbutton', { name: 'Issue workflow version' });
  await userEvent.clear(version);
  await userEvent.type(version, '2');
  await userEvent.click(screen.getByRole('button', { name: 'Review workflow version' }));
  expect(get).toHaveBeenCalledWith('project', workflow.id, 2, expect.anything());
  await waitFor(() =>
    expect(screen.getByRole('combobox', { name: 'Issue workflow' }).textContent).toContain(
      'Local workflow · v2',
    ),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Issue transition' }));
  await userEvent.click(screen.getByRole('option', { name: /Accept delivery/ }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Declared action outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'succeeded' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed Issue action' }));
  expect(onChange).toHaveBeenCalledWith(
    expect.objectContaining({
      action: expect.objectContaining({
        parameters: {
          workflow_id: workflow.id,
          workflow_version: 2,
          transition_id: workflow.definition.transitions[0]!.id,
          outcome: 'succeeded',
        },
      }),
    }),
  );
});

it('explains an empty visible workflow catalog without enabling or changing an action', async () => {
  const node = configurationFixture().configuration.flow.closure.definitions[0]!.nodes.find(
    (item) => item.action,
  )!;
  const change = vi.fn();
  const get = vi.fn();
  const { FlowIssueStatus } = await import('./FlowIssueStatus');
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueStatus
        node={node}
        services={{
          projectId: 'project',
          api: {
            ...issueWorkflowsApi,
            list: vi.fn(async () => ({ items: [], next_cursor: null })),
            get,
          },
        }}
        onChange={change}
      />
    </QueryClientProvider>,
  );
  expect(await screen.findByText('No visible workflows are available.')).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Use reviewed Issue action' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(change).not.toHaveBeenCalled();
  expect(get).not.toHaveBeenCalled();
});

it('discards the previous action review when selecting another execution profile in the same node', async () => {
  const projectId = '11111111-1111-4111-8111-111111111111';
  const workflowA = localWorkflow(projectId);
  const workflowB = { ...localWorkflow(projectId), id: '20000000-0000-4000-8000-000000000002' };
  const flowA = configurationFixture().configuration.flow;
  const definitionA = flowA.closure.definitions[0]!;
  const nodeA = definitionA.nodes.find((item) => item.action)!;
  nodeA.action = {
    ...nodeA.action!,
    action: 'transition_issue',
    parameters: {
      workflow_id: workflowA.id,
      workflow_version: 3,
      transition_id: workflowA.definition.transitions[0]!.id,
      outcome: 'failed',
    },
  };
  definitionA.entry = nodeA.key;
  definitionA.nodes = [nodeA];
  const flowB = structuredClone(flowA);
  const definitionB = flowB.closure.definitions[0]!;
  definitionB.reference.id = '40000000-0000-4000-8000-000000000002';
  flowB.closure.root = definitionB.reference;
  const nodeB = definitionB.nodes[0]!;
  definitionB.entry = nodeB.key = 'other-profile';
  nodeB.action = {
    ...nodeB.action!,
    parameters: {
      workflow_id: workflowB.id,
      workflow_version: 3,
      transition_id: workflowB.definition.transitions[0]!.id,
      outcome: 'failed',
    },
  };
  let observed: ReturnType<typeof emptyFlowDraft> = {
    ...emptyFlowDraft(),
    root: structuredClone(definitionA),
    data_schemas: flowA.data_schemas,
  };
  const routes = structuredClone(observed.root.transitions);
  const api = {
    ...issueWorkflowsApi,
    list: vi.fn(async () => ({
      items: [workflowA, workflowB].map((workflow, index) => ({
        ...workflow,
        name: `Workflow ${index ? 'B' : 'A'}`,
      })),
      next_cursor: null,
    })),
    get: vi.fn(async (_project: string, id: string) =>
      id === workflowB.id ? workflowB : workflowA,
    ),
  };
  const capability = nodeA.action.plugin;
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[
          {
            alias: 'local-action',
            identity: 'local-action',
            kind: 'node_capability',
            reference: { kind: 'immutable', reference: capability },
          },
        ]}
        subflows={[flowA, flowB].map((flow) => ({
          flow,
          project_id: projectId,
          published_at_unix_ms: 1,
        }))}
        issueStatus={{ api, projectId }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Issue workflow' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Workflow A · v3' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Issue transition' }));
  await userEvent.click(screen.getByRole('option', { name: /Accept delivery/ }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Declared action outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'succeeded' }));

  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed Issue action' }));
  expect(screen.getByRole('button', { name: 'Use reviewed Issue action' })).toHaveProperty(
    'disabled',
    false,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  await userEvent.click(screen.getByRole('option', { name: 'other-profile · other-profile · v1' }));
  expect(screen.getByRole('combobox', { name: 'Declared action outcome' }).textContent).toContain(
    'failed',
  );
  expect(screen.getByRole('button', { name: 'Use reviewed Issue action' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Issue workflow' }));
  await userEvent.click(screen.getByRole('option', { name: 'Workflow B · v3' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Issue transition' }));
  await userEvent.click(screen.getByRole('option', { name: /Accept delivery/ }));
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed Issue action' }));
  expect(observed.root.nodes[0]!.action!.parameters).toEqual({
    workflow_id: workflowB.id,
    workflow_version: 3,
    transition_id: workflowB.definition.transitions[0]!.id,
    outcome: 'failed',
  });
  expect(observed.root.transitions).toEqual(routes);
});
