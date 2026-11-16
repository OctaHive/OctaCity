// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { SnapshotFieldSelectors } from './SnapshotFieldSelectors';
afterEach(cleanup);
it('rebinds an existing alias without changing its requiredness when choice IDs happen to match', async () => {
  const user = userEvent.setup();
  const choices = factoryIssueFieldsFixture().issue_fields;
  choices.fields.find(
    (field) => field.source.kind === 'property' && field.source.id === 'impact',
  )!.shape = { type: 'enum', values: ['small', 'medium', 'large'] };
  const changed = vi.fn();
  function Harness() {
    const [fields, setFields] = useState<
      Record<string, components['schemas']['FactoryFlowIssueFieldSelection']>
    >({ size: { source: { kind: 'size' }, required: true } });
    return (
      <SnapshotFieldSelectors
        fields={fields}
        choices={choices}
        onChange={(value) => {
          changed(value);
          setFields(value);
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
});
