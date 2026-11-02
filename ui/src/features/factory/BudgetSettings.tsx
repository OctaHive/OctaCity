import { useEffect, useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { BudgetEditor, BUDGET_LABELS } from './ResourceEditors';

type Budget = components['schemas']['FactoryBudget'];
/** Calculation changes draft values only. Manual limits never follow later profile changes. */
export function BudgetSettings({
  value,
  suggested,
  source,
  label,
  onChange,
  automaticByDefault = false,
  automatic: controlledAutomatic,
  onAutomaticChange,
  ...editor
}: {
  value: Budget;
  suggested: Budget | null;
  source: string;
  label: string;
  onChange: (value: Budget) => void;
  automaticByDefault?: boolean;
  automatic?: boolean;
  onAutomaticChange?: (automatic: boolean) => void;
  ceiling?: Budget;
  fieldPath?: string;
}) {
  const { t, locale } = usePresentation();
  const [localAutomatic, setLocalAutomatic] = useState(automaticByDefault);
  const automatic = controlledAutomatic ?? localAutomatic;
  useEffect(() => {
    if (
      automatic &&
      suggested &&
      Object.keys(value).some(
        (key) => value[key as keyof Budget] !== suggested[key as keyof Budget],
      )
    )
      onChange(suggested);
  }, [automatic, suggested, value, onChange]);
  return (
    <section data-factory-budget>
      <SelectMenu
        ariaLabel={`${label} · ${t('factory.calculation')}`}
        value={automatic && suggested ? 'automatic' : 'manual'}
        options={[
          { value: 'manual', label: t('factory.setManually') },
          ...(suggested
            ? [{ value: 'automatic', label: t('factory.calculateAutomatically') }]
            : []),
        ]}
        onValueChange={(mode) => (onAutomaticChange ?? setLocalAutomatic)(mode === 'automatic')}
      />
      <p>{suggested ? source : t('factory.calculationUnavailable')}</p>
      {automatic && suggested ? (
        <dl>
          {(Object.keys(BUDGET_LABELS) as (keyof Budget)[]).map((key) => (
            <div key={key}>
              <dt>{t(BUDGET_LABELS[key])}</dt>
              <dd>{value[key].toLocaleString(locale)}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      <details open={!automatic || suggested === null}>
        <summary>{t('factory.budgetValues')}</summary>
        <fieldset disabled={automatic && suggested !== null}>
          <BudgetEditor
            {...editor}
            label={label}
            value={value}
            onChange={(budget) => {
              if (automatic && !suggested) (onAutomaticChange ?? setLocalAutomatic)(false);
              onChange(budget);
            }}
          />
        </fieldset>
      </details>
    </section>
  );
}
