import { ISSUE_CONSTRAINTS } from '../../../.generated/api/constraints';
import { FACTORY_AUTHORING_SCHEMAS } from '../../../.generated/api/factory-authoring';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { compatibleFactoryShape } from './validateDraft';

type Schema = components['schemas'];
export type IssuePresentation = Schema['FactoryFlowIssuePresentation'];
type DisplayKind = IssuePresentation['kind'] | 'off';
type Shape = Schema['FactoryFlowValueSchema'];

const builtIns = FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssuePresentation['x-built-in-contracts'];
const keySchema = FACTORY_AUTHORING_SCHEMAS.FactoryFlowOutcome.properties.key;
const factoryKey = new RegExp(keySchema.pattern);

const displayLabels = {
  off: 'factory.issueCardOff',
  triage_v1: 'factory.issueCardTriage',
  specifications_v1: 'factory.issueCardSpecifications',
  review_v1: 'factory.issueCardReview',
  custom_v1: 'factory.issueCardCustom',
} satisfies Record<DisplayKind, MessageKey>;

const fieldLabels = {
  conclusion: 'factory.issueCardConclusion',
  decision: 'factory.decision',
  duplicate_coverage: 'factory.issueCardDuplicateCoverage',
  priority: 'issueOutcomes.recommendedPriority',
  size: 'issueOutcomes.recommendedSize',
  component: 'triageResult.component',
  documents: 'factory.issueCardDocuments',
  verdict: 'factory.issueCardVerdict',
  review: 'factory.issueCardReview',
} satisfies Record<(typeof builtIns)[keyof typeof builtIns]['fields'][number]['name'], MessageKey>;

const builtInKinds = Object.keys(builtIns) as (keyof typeof builtIns)[];
const maxConclusionBytes = ISSUE_CONSTRAINTS.outcomeConclusionMaximumBytes;
const conclusionShape = { type: 'string', min_bytes: 1, max_bytes: maxConclusionBytes } as const;
const customProperties = FACTORY_AUTHORING_SCHEMAS.FactoryFlowIssuePresentation.oneOf
  .map(({ properties }) => properties)
  .find((properties) => 'title' in properties)!;
const maxTitleBytes = customProperties.title['x-max-utf8-bytes'];

function conclusionFields(shape: Shape): string[] {
  if (shape.type !== 'object') return [];
  return Object.entries(shape.fields).flatMap(([name, { required, value }]) => {
    return required && compatibleFactoryShape(value, conclusionShape) ? [name] : [];
  });
}

function supportsBuiltIn(shape: Shape, kind: keyof typeof builtIns): boolean {
  if (shape.type !== 'object') return false;
  return builtIns[kind].fields.every((rule) => {
    const field = Object.hasOwn(shape.fields, rule.name) ? shape.fields[rule.name] : undefined;
    if (!field) return !rule.required;
    if (rule.required && !field.required) return false;
    const expected = rule.shape as Shape | null;
    if (expected === null)
      return (
        field.value.type === 'enum' &&
        field.value.values.every(
          (value) =>
            typeof value === 'string' &&
            value.length <= keySchema.maxLength &&
            factoryKey.test(value),
        )
      );
    if (expected.type === 'array' && field.value.type !== 'array') return false;
    return compatibleFactoryShape(field.value, expected);
  });
}

/** The editor guides field selection. Flow publication validates the complete binding. */
export function canSaveIssuePresentation(
  shape: Shape,
  presentation: IssuePresentation | undefined,
) {
  if (!presentation) return true;
  if (presentation.kind !== 'custom_v1') return supportsBuiltIn(shape, presentation.kind);
  const title = presentation.title.trim();
  return (
    title.length > 0 &&
    new TextEncoder().encode(title).length <= maxTitleBytes &&
    conclusionFields(shape).includes(presentation.conclusion_field)
  );
}

export function OutcomeIssuePresentationEditor({
  shape,
  value,
  onChange,
}: {
  shape: Shape;
  value: IssuePresentation | undefined;
  onChange: (value: IssuePresentation | undefined) => void;
}) {
  const { t } = usePresentation();
  const fields = conclusionFields(shape);
  const options: DisplayKind[] = ['off'];
  for (const kind of builtInKinds) {
    if (value?.kind === kind || supportsBuiltIn(shape, kind)) options.push(kind);
  }
  if (fields.length || value?.kind === 'custom_v1') options.push('custom_v1');
  return (
    <fieldset>
      <legend>{t('factory.showOnIssue')}</legend>
      {options.length === 1 ? (
        <p>{t('factory.issueCardFormatHelp', { max: maxConclusionBytes })}</p>
      ) : null}
      <SelectMenu
        ariaLabel={t('factory.showOnIssue')}
        value={value?.kind ?? 'off'}
        options={options.map((kind) => ({ value: kind, label: t(displayLabels[kind]) }))}
        onValueChange={(kind) => {
          if (kind === value?.kind) return;
          if (kind === 'off') onChange(undefined);
          else if (kind === 'custom_v1') onChange({ kind, title: '', conclusion_field: '' });
          else onChange({ kind });
        }}
      />
      {value?.kind === 'custom_v1' ? (
        <>
          <label>
            {t('factory.issueCardTitle')}
            <input
              value={value.title}
              maxLength={maxTitleBytes}
              onChange={(event) => onChange({ ...value, title: event.target.value })}
            />
          </label>
          <fieldset>
            <legend>{t('factory.issueCardConclusionField')}</legend>
            <SelectMenu
              ariaLabel={t('factory.issueCardConclusionField')}
              value={value.conclusion_field}
              options={[
                { value: '', label: t('factory.chooseInput') },
                ...fields.map((field) => ({ value: field, label: field })),
              ]}
              onValueChange={(field) => onChange({ ...value, conclusion_field: field })}
            />
          </fieldset>
        </>
      ) : null}
    </fieldset>
  );
}

export function issuePresentationTitle(
  presentation: IssuePresentation,
  t: ReturnType<typeof usePresentation>['t'],
) {
  return presentation.kind === 'custom_v1'
    ? presentation.title
    : t(displayLabels[presentation.kind]);
}

/** Shows only fields the normalized Flow can disclose through each binding. */
export function OutcomeIssuePresentationReview({
  flow,
}: {
  flow: Schema['FactoryFlowConfiguration'];
}) {
  const { t } = usePresentation();
  const cards = flow.closure.definitions.flatMap((definition) =>
    definition.nodes.flatMap((node) =>
      node.outcomes.flatMap((outcome) => {
        const binding = outcome.issue_presentation;
        if (!binding) return [];
        const producer = `${node.key}.${outcome.key}`;
        const shape = flow.data_schemas.find(
          ({ reference }) => reference.digest === outcome.schema.digest,
        )?.shape;
        const fields = shape?.type === 'object' ? shape.fields : {};
        const title = issuePresentationTitle(binding, t);
        const slot =
          binding.slot ?? (binding.kind === 'custom_v1' ? undefined : builtIns[binding.kind].slot);
        return (
          <article
            key={JSON.stringify([
              definition.reference.id,
              definition.reference.version,
              node.key,
              outcome.key,
            ])}
            aria-label={title}
          >
            <h4>{title}</h4>
            <p>{definition.name ? `${definition.name} · ${producer}` : producer}</p>
            <p>
              {slot === undefined
                ? t('factory.issueCardAutomaticSlot', {
                    flow: definition.reference.id,
                    step: node.key,
                    outcome: outcome.key,
                  })
                : t('factory.issueCardSlot', { slot })}
            </p>
            <ul>
              {binding.kind === 'custom_v1' ? (
                <>
                  <li>
                    {t('factory.issueCardConclusion')} <code>{binding.conclusion_field}</code>
                  </li>
                  {binding.references?.map(({ field }) => (
                    <li key={field}>{field}</li>
                  ))}
                </>
              ) : (
                <>
                  {builtIns[binding.kind].fields
                    .filter(({ name }) => Object.hasOwn(fields, name))
                    .map(({ name }) => (
                      <li key={name}>{t(fieldLabels[name])}</li>
                    ))}
                </>
              )}
            </ul>
          </article>
        );
      }),
    ),
  );
  return cards.length ? (
    <section aria-label={t('factory.showOnIssue')}>
      <h3>{t('factory.showOnIssue')}</h3>
      {cards}
    </section>
  ) : null;
}
