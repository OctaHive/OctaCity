import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';
import type { usePresentation } from '../../app/presentation/PresentationProvider';
import type { IssuePlanning, IssuePropertyCatalog } from './api';

type Choice = components['schemas']['IssuePropertyChoice'];
type Translate = ReturnType<typeof usePresentation>['t'];

/** Omitted wire lists use the server's generated defaults; an explicit empty list stays empty. */
export function issueBuiltinChoices(definition: IssuePropertyCatalog['definition']) {
  return {
    component: definition.components ?? [],
    type: definition.types ?? bounds.defaultTypes,
    priority: definition.priorities ?? bounds.defaultPriorities,
  };
}

export function choiceLabel(
  value: string | null | undefined,
  choices: readonly Choice[],
  t: Translate,
) {
  return value
    ? (choices.find((c) => c.id === value)?.name ?? t('planning.unavailableValue'))
    : t('issues.notSet');
}

export function choiceOptions(
  value: string | null | undefined,
  choices: readonly Choice[],
  t: Translate,
) {
  return [
    { value: '', label: t('issues.notSet') },
    ...choices
      .filter((c) => !c.retired || c.id === value)
      .map((c) => ({
        value: c.id,
        label: c.retired ? t('classification.retiredChoice', { name: c.name }) : c.name,
      })),
    ...(value && !choices.some((c) => c.id === value)
      ? [{ value, label: t('planning.unavailableValue') }]
      : []),
  ];
}

/** The built-ins and legacy custom fields retain separate identities and storage. */
export function builtinPlanningFields(
  plan: IssuePlanning | undefined,
  definition: IssuePropertyCatalog['definition'],
  t: Translate,
) {
  const choices = issueBuiltinChoices(definition);
  return [
    {
      field: 'component' as const,
      label: t('classification.component'),
      value: choiceLabel(plan?.component, choices.component, t),
      options: choiceOptions(plan?.component, choices.component, t),
    },
    {
      field: 'type' as const,
      label: t('classification.type'),
      value: choiceLabel(plan?.type, choices.type, t),
      options: choiceOptions(plan?.type, choices.type, t),
    },
    {
      field: 'size' as const,
      label: t('classification.size'),
      value: plan?.size ? t(`triageResult.value.${plan.size}`) : t('issues.notSet'),
      options: [
        { value: '', label: t('issues.notSet') },
        ...bounds.sizes.map((value) => ({ value, label: t(`triageResult.value.${value}`) })),
      ],
    },
  ];
}
