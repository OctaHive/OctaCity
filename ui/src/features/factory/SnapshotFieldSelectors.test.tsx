// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { SnapshotFieldSelectors } from './SnapshotFieldSelectors';
afterEach(cleanup);
it('requires a fresh threshold choice when a different source happens to have the same choice IDs', async () => {
  const user = userEvent.setup();
  const choices = factoryIssueFieldsFixture().issue_fields;
  choices.fields.find(
    (field) => field.source.kind === 'property' && field.source.id === 'impact',
  )!.shape = { type: 'enum', values: ['small', 'medium', 'large'] };
  const changed = vi.fn();
  const changedThreshold = vi.fn();
  function Harness() {
    const [fields, setFields] = useState<
      Record<string, components['schemas']['FactoryFlowIssueFieldSelection']>
    >({ size: { source: { kind: 'size' }, required: true } });
    const [thresholds, setThresholds] = useState({ branch: 'medium' } as Record<string, string>);
    return (
      <SnapshotFieldSelectors
        fields={fields}
        choices={choices}
        onChange={(value) => {
          changed(value);
          setFields(value);
        }}
        conditions={{ branch: 'size' }}
        thresholds={thresholds}
        onThresholdChange={(value) => {
          changedThreshold(value);
          setThresholds(value);
        }}
      />
    );
  }
  render(<Harness />);
  await user.click(screen.getByRole('combobox', { name: 'Issue input field: size' }));
  await user.click(screen.getByRole('option', { name: 'Project field: Impact' }));
  expect(changed).toHaveBeenCalledWith({
    size: { source: { kind: 'property', id: 'impact' }, required: true },
  });
  expect(changedThreshold).toHaveBeenCalledWith({});
});
