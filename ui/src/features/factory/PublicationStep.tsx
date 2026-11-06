import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';

type S = components['schemas'];
type Node = S['FactoryFlowNodeDefinition'];
type Choice = S['FactoryCandidatePublicationChoice'];

export const publishCandidateAction = 'publish_candidate';

export const publicationKey = (choice: Choice) =>
  `${sourceReferenceKey(choice.plugin)}:${sourceReferenceKey(choice.target)}`;

function fields(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

/** Resolves the exact target selected by the trusted publication action. */
export function publicationChoice(action: Node['action'], choices: readonly Choice[]) {
  if (action?.action !== publishCandidateAction) return undefined;
  const target = fields(fields(action.parameters).target);
  return choices.find(
    (choice) =>
      sourceReferenceKey(choice.plugin) === sourceReferenceKey(action.plugin) &&
      Object.entries(choice.target).every(([key, value]) => value === target[key]),
  );
}

/** Binds the selected installed target and receipt. Input evidence remains explicitly connected. */
export function PublicationStep({
  node,
  choices,
  onChange,
}: {
  node: Node;
  choices: readonly Choice[];
  onChange: (patch: Partial<Node>, schemas?: S['FactoryFlowDataSchema'][]) => void;
}) {
  const { t } = usePresentation();
  const action = node.action?.action === publishCandidateAction ? node.action : null;
  const parameters = fields(action?.parameters);
  const configured = publicationChoice(action, choices);
  const [selection, setSelection] = useState<string>();
  const selected = choices.find((choice) => publicationKey(choice) === selection);
  const update = (field: string, value: unknown) => {
    if (!action) return;
    const next = { ...parameters };
    if (value === undefined) delete next[field];
    else next[field] = value;
    onChange({ action: { ...action, parameters: next } });
  };
  const failures = [
    { value: '', label: t('factory.none') },
    ...node.outcomes
      .filter(({ kind }) => kind === 'failure')
      .map(({ key }) => ({ value: key, label: key })),
  ];
  if (node.action && !action) return null;
  return (
    <fieldset>
      <legend>{t('factory.publicationAction')}</legend>
      {choices.length ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.publicationTarget')}
            value={selection ?? (configured ? publicationKey(configured) : '')}
            options={[
              { value: '', label: t('factory.chooseInput') },
              ...choices.map((choice) => ({
                value: publicationKey(choice),
                label: `${choice.target.identity} · ${choice.target.version}`,
              })),
            ]}
            onValueChange={setSelection}
          />
          <button
            type="button"
            disabled={!selected || selected === configured}
            onClick={() => {
              if (!selected) return;
              const receipt = { ...selected.outcome };
              const presentation = node.outcomes.find(
                ({ key, kind, schema }) =>
                  key === receipt.key &&
                  kind === receipt.kind &&
                  schema.digest === receipt.schema.digest,
              )?.issue_presentation;
              if (selected.review_url && presentation) receipt.issue_presentation = presentation;
              const outcomes = node.outcomes.filter(
                (outcome) =>
                  outcome.key !== selected.outcome.key && outcome.key !== configured?.outcome.key,
              );
              onChange(
                {
                  action: {
                    plugin: selected.plugin,
                    action: publishCandidateAction,
                    parameters: {
                      bundle: 'candidate_bundle',
                      manifest: 'candidate_manifest',
                      ...parameters,
                      target: selected.target,
                      require_review_url: selected.review_url,
                    },
                  },
                  outcomes: [...outcomes, receipt],
                },
                [selected.receipt_schema],
              );
              setSelection(undefined);
            }}
          >
            {t('factory.usePublicationTarget')}
          </button>
        </>
      ) : (
        <p>{t('factory.noPublicationTargets')}</p>
      )}
      {action ? (
        <details>
          <summary>{t('factory.publicationInputs')}</summary>
          {(
            ['bundle', 'manifest', 'valid_until', 'review_valid_until', 'review_metadata'] as const
          ).map((field) => {
            const parameter = parameters[field];
            const pointer =
              typeof parameter === 'object' && parameter !== null && 'value' in parameter
                ? parameter.value
                : null;
            const name = field === 'bundle' || field === 'manifest';
            return (
              <label key={field}>
                {t(`factory.publication.${field}`)}
                <input
                  value={
                    name
                      ? typeof parameter === 'string'
                        ? parameter
                        : ''
                      : pointer && typeof pointer === 'object' && 'path' in pointer
                        ? String(pointer.path)
                        : ''
                  }
                  onChange={({ target: { value } }) =>
                    update(
                      field,
                      name
                        ? value
                        : value
                          ? { value: { mapping: 'pointer', path: value } }
                          : undefined,
                    )
                  }
                />
              </label>
            );
          })}
          {(['conflict_outcome', 'rejected_outcome'] as const).map((field) => {
            const label = t(`factory.publication.${field}`);
            const value = parameters[field];
            return (
              <label key={field}>
                {label}
                <SelectMenu
                  ariaLabel={label}
                  value={typeof value === 'string' ? value : ''}
                  options={failures}
                  onValueChange={(selected) => update(field, selected || undefined)}
                />
              </label>
            );
          })}
        </details>
      ) : null}
    </fieldset>
  );
}
