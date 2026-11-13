import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import type { MessageKey } from '../../app/presentation/messages';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice } from '../../shared/QueryStateNotice';
import { useFactoryAuthoringCapabilities } from './useFactoryAuthoringCapabilities';
import type { FlowAuthoringServices } from './FlowPrimitives';

type S = components['schemas'];
export type IssueFieldChoice = S['FactoryIssueFieldChoice'];
export type IssueFieldChoices = S['FactoryIssueFieldChoices'];
type Translate = ReturnType<typeof usePresentation>['t'];
const labels = {
  title: 'issues.title',
  description: 'issues.description',
  acceptance: 'issues.acceptance',
  priority: 'issues.priority',
  component: 'classification.component',
  type: 'classification.type',
  size: 'classification.size',
  dependencies: 'classification.dependencies',
} satisfies Record<
  Exclude<IssueFieldChoice['source']['kind'], 'property' | 'classification'>,
  MessageKey
>;

export function issueFieldLabel(
  field: Pick<IssueFieldChoice, 'source'> & Partial<Pick<IssueFieldChoice, 'name'>>,
  t: Translate,
) {
  return field.source.kind === 'property'
    ? t('factory.issuePropertyField', { name: field.name ?? field.source.id })
    : field.source.kind === 'classification'
      ? t('factory.issueClassificationField', { name: field.name ?? field.source.slot })
      : t(labels[field.source.kind]);
}

/** Server availability, Project bindings and shapes supply the targets; the UI assembles only projections. */
export function classificationFields(choices: IssueFieldChoices | undefined) {
  const selection = choices?.classification;
  if (!selection || !choices) return [];
  const enabled = new Set(
    Object.entries(selection)
      .filter(([, value]) => value === true)
      .map(([key]) => key),
  );
  return choices.fields.flatMap((field) => {
    if (field.source.kind === 'classification') {
      const binding = Object.entries(selection.properties).find(
        ([slot, id]) =>
          field.source.kind === 'classification' &&
          slot === field.source.slot &&
          id === field.property_id,
      );
      return binding ? [{ name: binding[0], field }] : [];
    }
    const name = field.source.kind === 'type' ? 'kind' : field.source.kind;
    return enabled.has(name) ? [{ name, field }] : [];
  });
}

export function ClassificationFieldPicker({
  choices,
  selected,
  onChange,
}: {
  choices: IssueFieldChoices;
  selected: readonly string[];
  onChange: (selected: string[]) => void;
}) {
  const { t } = usePresentation();
  return (
    <fieldset>
      <legend>{t('factory.projectClassificationFields')}</legend>
      <p>{t('planning.definition', { version: choices.definition_version })}</p>
      {classificationFields(choices).map(({ name, field }) => (
        <label key={name}>
          <input
            type="checkbox"
            checked={selected.includes(name)}
            onChange={(event) =>
              onChange(
                event.target.checked ? [...selected, name] : selected.filter((key) => key !== name),
              )
            }
          />
          {issueFieldLabel(field, t)}
        </label>
      ))}
    </fieldset>
  );
}

/** Explicitly reuse selected Project types in a result draft; saving remains the normal schema command. */
export function ProjectIssueResultFields({
  services,
  shape,
  onChange,
}: {
  services: FlowAuthoringServices;
  shape: S['FactoryFlowValueSchema'];
  onChange: (shape: S['FactoryFlowValueSchema']) => void;
}) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  const [selected, setSelected] = useState<string[]>([]);
  const query = useFactoryAuthoringCapabilities(services, open);
  const choices = query.data?.issue_fields;
  return (
    <>
      <button type="button" onClick={() => setOpen(!open)} aria-expanded={open}>
        {t('factory.reuseProjectFields')}
      </button>
      {open ? (
        <>
          {query.error ? (
            <QueryFailureNotice
              title={t('factory.projectClassificationFields')}
              error={query.error}
              onRetry={query.refetch}
            />
          ) : null}
          {query.isFetching ? <p role="status">{t('factory.loadingStepCapabilities')}</p> : null}
          {choices?.classification ? (
            <ClassificationFieldPicker
              choices={choices}
              selected={selected}
              onChange={setSelected}
            />
          ) : (
            <p>{t('factory.issueFieldsUnavailable')}</p>
          )}
          <button
            type="button"
            disabled={!selected.length || query.isFetching || !!query.error}
            onClick={() => {
              const fields = shape.type === 'object' ? shape.fields : {};
              const additions = Object.fromEntries(
                classificationFields(choices)
                  .filter(({ name }) => selected.includes(name))
                  .map(({ name, field }) => [
                    name,
                    { required: fields[name]?.required ?? false, value: field.shape },
                  ]),
              );
              onChange({ type: 'object', fields: { ...fields, ...additions } });
              setOpen(false);
            }}
          >
            {t('factory.useSelectedFieldContracts')}
          </button>
        </>
      ) : null}
    </>
  );
}
