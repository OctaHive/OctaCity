// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { PresentationProvider } from '../../app/presentation/PresentationProvider';
import { DEFAULT_PREFERENCES } from '../../app/presentation/preferences';
import { translate } from '../../app/presentation/messages';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { IssueAdmission, type IssueAdmissionPolicy } from './IssueAdmission';

afterEach(cleanup);

it.each(['en', 'ru'] as const)(
  'edits reviewed admission with the keyboard and explains invalid scores (%s)',
  async (language) => {
    const user = userEvent.setup();
    const t = (key: Parameters<typeof translate>[1], values = {}) =>
      translate(language, key, values);
    const storage = {
      getItem: () => JSON.stringify({ ...DEFAULT_PREFERENCES, language }),
      setItem: () => {},
    };
    const offered = factoryIssueFieldsFixture().issue_fields.fields;
    const fields = offered
      .filter(
        ({ source }) =>
          source.kind === 'priority' || (source.kind === 'property' && source.id === 'impact'),
      )
      .map((field) => ({
        field,
        alias: field.source.kind === 'priority' ? 'priority' : 'impact',
        required: true,
      }));
    let selected: IssueAdmissionPolicy | undefined;
    function Harness() {
      const [value, setValue] = useState<IssueAdmissionPolicy | undefined>({
        priority: { field: 'priority', values: { urgent: 90 } },
        risk: { field: 'impact', values: { major: 'high' } },
        allowed: {},
      });
      return (
        <IssueAdmission
          value={value}
          fields={fields}
          onChange={(value) => {
            selected = value;
            setValue(value);
          }}
        />
      );
    }
    render(
      <PresentationProvider storage={storage}>
        <Harness />
      </PresentationProvider>,
    );
    const score = screen.getByRole('spinbutton', {
      name: t('factory.schedulingPriority', { choice: 'urgent' }),
    });
    await user.clear(score);
    await user.type(score, '101');
    expect(screen.getByRole('alert').textContent).toBe(t('factory.admissionInvalid'));
    await user.clear(score);
    await user.type(score, '80');
    expect(screen.queryByRole('alert')).toBeNull();
    const risk = screen.getByRole('combobox', {
      name: t('factory.admissionRisk', { choice: 'major' }),
    });
    risk.focus();
    await user.keyboard('{Enter}{Home}{ArrowDown}{Enter}');
    expect(selected).toEqual({
      priority: { field: 'priority', values: { urgent: 80 } },
      risk: { field: 'impact', values: { major: 'low' } },
      allowed: {},
    });
  },
);
