// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { factoryApi, type FactoryApi } from './api';
import { emptyFlowDraft } from './emptyFlowDraft';
import { TypedIssueClassification } from './TypedIssueClassification';

type S = components['schemas'];
afterEach(cleanup);

it.each(['producer', 'outcome', 'writer'] as const)(
  'does not apply a captured classification after its %s changes during normalization',
  async (change) => {
    const user = userEvent.setup();
    const capabilities = factoryIssueFieldsFixture();
    const draft = emptyFlowDraft();
    const template = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
    const schema: S['FactoryFlowDataSchema'] = {
      reference: { identity: 'assessment', version: '1', digest: 'a'.repeat(64) },
      shape: {
        type: 'object',
        fields: {
          component: { required: true, value: { type: 'enum', values: ['payments'] } },
          private_notes: {
            required: true,
            value: { type: 'string', min_bytes: 0, max_bytes: 100 },
          },
        },
      },
    };
    const node: S['FactoryFlowNodeDefinition'] = {
      ...template,
      key: 'apply',
      kind: 'trusted_action',
      input_schema: null,
      input_binding: null,
      action: null,
      outcomes: [],
    };
    draft.root.nodes = [
      ...['first', 'replacement'].map((key) => ({
        ...template,
        key,
        outcomes: ['assessed', 'reassessed'].map((key) => ({
          key,
          kind: 'success' as const,
          schema: schema.reference,
        })),
      })),
      node,
    ];
    const transitions = (
      producer: string,
      outcome = 'assessed',
    ): S['FactoryFlowRootDraft']['transitions'] => [
      {
        predecessor: producer,
        outcome,
        target: { kind: 'node', key: 'apply' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
    ];
    draft.root.transitions = transitions('first');
    let release: (() => void) | undefined;
    const api = {
      ...factoryApi,
      normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(
        (body) =>
          new Promise((resolve) => {
            release = () =>
              resolve({
                reference: {
                  identity: body.identity,
                  version: body.version,
                  digest: 'c'.repeat(64),
                },
                shape: body.shape,
              });
          }),
      ),
    };
    const onChange = vi.fn();
    function Harness() {
      const [root, setRoot] = useState(draft.root);
      const [choices, setChoices] = useState(capabilities.issue_classification);
      return (
        <>
          <button
            type="button"
            onClick={() => {
              if (change === 'writer') setChoices({ ...choices, policy: 'replacement-policy' });
              else
                setRoot({
                  ...root,
                  transitions:
                    change === 'producer'
                      ? transitions('replacement')
                      : transitions('first', 'reassessed'),
                });
            }}
          >
            Change compiler inputs
          </button>
          <TypedIssueClassification
            node={node}
            root={root}
            schemas={[schema]}
            services={{ api, projectId: 'delivery' }}
            choices={choices}
            fields={capabilities.issue_fields}
            ready
            onChange={onChange}
          />
        </>
      );
    }
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <Harness />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('checkbox', { name: 'Issue component' }));
    await user.click(screen.getByRole('button', { name: 'Use Project classification' }));
    expect(api.normalizeSchema).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Change compiler inputs' }));
    await act(async () => release!());
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Use Project classification' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    expect(onChange).not.toHaveBeenCalled();
  },
);
