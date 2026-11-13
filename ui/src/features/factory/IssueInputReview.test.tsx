// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { IssueInputReview } from './IssueInputReview';

afterEach(cleanup);

it.each(['whole source', 'nested source', 'forwarded value and guard'] as const)(
  'distinguishes snapshot presence checks from forwarded values (%s)',
  (kind) => {
    const flow = configurationFixture().configuration.flow;
    const boolean = flow.data_schemas[0]!.reference;
    const reference = { identity: 'issue-input', version: '1', digest: 'a'.repeat(64) };
    flow.data_schemas.push({
      reference,
      shape: {
        type: 'object',
        fields: {
          component: {
            required: kind !== 'whole source',
            value: { type: 'enum', values: ['payments'] },
          },
          type: { required: false, value: { type: 'enum', values: ['defect'] } },
        },
      },
    });
    flow.issue_snapshot = {
      version: 1,
      definition_version: 7,
      schema: reference,
      fields: {
        component: { source: { kind: 'component' }, required: kind !== 'whole source' },
        type: { source: { kind: 'type' }, required: false },
      },
    };
    const node = flow.closure.definitions[0]!.nodes[0]!;
    const guard: components['schemas']['FactoryFlowValueMapping'] = {
      mapping: 'if_present',
      path:
        kind === 'nested source'
          ? '/issue'
          : kind === 'whole source'
            ? '/issue/component'
            : '/issue/type',
      then: { mapping: 'constant', value: true },
      otherwise: { mapping: 'constant', value: false },
    };
    node.input_schema = boolean;
    if (kind === 'forwarded value and guard') {
      const target = { identity: 'step-input', version: '1', digest: 'b'.repeat(64) };
      flow.data_schemas.push({
        reference: target,
        shape: {
          type: 'object',
          fields: {
            component: { required: true, value: { type: 'enum', values: ['payments'] } },
            present: { required: true, value: { type: 'boolean' } },
          },
        },
      });
      node.input_schema = target;
    }
    node.input_binding = {
      sources: {
        issue: {
          source: 'incoming',
          schema: reference,
          path: kind === 'nested source' ? '/component' : '',
        },
      },
      mapping: {
        value:
          kind === 'forwarded value and guard'
            ? {
                mapping: 'object',
                fields: {
                  component: { mapping: 'pointer', path: '/issue/component' },
                  present: guard,
                },
              }
            : guard,
      },
    };
    render(<IssueInputReview flow={flow} />);
    expect(
      screen.getByText(
        `${node.key} · ${kind === 'forwarded value and guard' ? 'component · Presence checks: /type' : 'Presence checks: /component'}`,
      ),
    ).toBeTruthy();
  },
);

it.each(['', '/selected'])(
  'reviews a nested Incoming source selected through pointer %j without adding sibling fields',
  (path) => {
    const flow = configurationFixture().configuration.flow;
    const reference = flow.data_schemas[0]!.reference;
    flow.issue_snapshot = {
      version: 1,
      definition_version: 7,
      schema: reference,
      fields: {
        component: { source: { kind: 'component' }, required: true },
        type: { source: { kind: 'type' }, required: false },
      },
      materials: {
        context: { source: { kind: 'selected', max_entries: 2 }, max_bytes: 2048, required: false },
      },
    };
    const node = flow.closure.definitions[0]!.nodes[0]!;
    node.input_binding = {
      sources: {
        selected: { source: 'incoming', schema: reference, path: '/component' },
        accepted: {
          source: 'record',
          node: 'producer',
          outcome: 'observed',
          view: 'payload',
          path: '',
        },
      },
      mapping: { value: { mapping: 'pointer', path } },
    };
    render(<IssueInputReview flow={flow} />);
    expect(screen.getByText(`${node.key} · component`)).toBeTruthy();
    expect(screen.queryByText(`${node.key} · component, type, context`)).toBeNull();
  },
);
