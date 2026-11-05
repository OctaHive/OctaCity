import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { sourceReferenceKey } from '../../shared/sourceReferences';

type S = components['schemas'];
type Node = S['FactoryFlowNodeDefinition'];
type Choice = S['FactoryCandidatePublicationChoice'];

export const publishCandidateAction = 'publish_candidate';

const choiceKey = (choice: Choice) =>
  `${sourceReferenceKey(choice.plugin)}:${sourceReferenceKey(choice.target)}`;

function fields(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
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
  const configured = choices.find(
    ({ plugin, target }) =>
      action &&
      sourceReferenceKey(plugin) === sourceReferenceKey(action.plugin) &&
      Object.entries(target).every(([key, value]) => value === fields(parameters.target)[key]),
  );
  const [selection, setSelection] = useState<string>();
  const selected = choices.find((choice) => choiceKey(choice) === selection);
  if (node.action && !action) return null;
  return (
    <fieldset>
      <legend>{t('factory.publicationAction')}</legend>
      {choices.length ? (
        <>
          <SelectMenu
            ariaLabel={t('factory.publicationTarget')}
            value={selection ?? (configured ? choiceKey(configured) : '')}
            options={[
              { value: '', label: t('factory.chooseInput') },
              ...choices.map((choice) => ({
                value: choiceKey(choice),
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
                  outcomes: [...outcomes, selected.outcome],
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
          {(['bundle', 'manifest', 'valid_until', 'review_metadata'] as const).map((field) => {
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
                  onChange={(event) => {
                    const value = event.target.value;
                    const next = { ...parameters };
                    if (name) next[field] = value;
                    else if (value) next[field] = { value: { mapping: 'pointer', path: value } };
                    else delete next[field];
                    onChange({ action: { ...action, parameters: next } });
                  }}
                />
              </label>
            );
          })}
        </details>
      ) : null}
    </fieldset>
  );
}
