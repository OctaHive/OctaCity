import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';

type S = components['schemas'];
type Reference = S['FactoryImmutableReferenceResource'];
type Node = S['FactoryFlowNodeDefinition'];
type Schema = S['FactoryFlowDataSchema'];
const same = (left: Reference | null | undefined, right: Reference) =>
  !!left &&
  left.identity === right.identity &&
  left.version === right.version &&
  left.digest === right.digest;

/** Display aliases follow current graph names; immutable references remain untouched. */
export function flowSchemaChoices(
  schemas: readonly Schema[],
  nodes: readonly Node[],
  t: ReturnType<typeof usePresentation>['t'],
) {
  return schemas.map((schema, index) => {
    const results = nodes.flatMap((node) =>
      node.outcomes
        .filter((outcome) => same(outcome.schema, schema.reference))
        .map((outcome) => `${node.key}.${outcome.key}`),
    );
    const inputs = nodes
      .filter((node) => same(node.input_schema, schema.reference))
      .map((node) => t('factory.stepInputFormat', { name: node.key }));
    return {
      value: String(index),
      kind: results.length ? 'result' : inputs.length ? 'input' : 'unassigned',
      label:
        (results.length ? results : inputs).join(', ') ||
        t('factory.additionalFormat', { number: index + 1 }),
    };
  });
}

/** Human-readable selection with a complete, explicitly opened reference catalog. */
export function FlowSchemaSelect({
  schemas,
  nodes,
  value,
  onValueChange,
  ariaLabel,
  allowNone = false,
}: {
  schemas: readonly Schema[];
  nodes: readonly Node[];
  value: string;
  onValueChange: (value: string) => void;
  ariaLabel: string;
  allowNone?: boolean;
}) {
  const { t } = usePresentation();
  const [identifiers, setIdentifiers] = useState(false);
  const choices = flowSchemaChoices(schemas, nodes, t);
  return (
    <>
      <SelectMenu
        ariaLabel={ariaLabel}
        value={value}
        onValueChange={onValueChange}
        options={[...(allowNone ? [{ value: '-1', label: t('factory.none') }] : []), ...choices]}
      />
      <details onToggle={(event) => setIdentifiers(event.currentTarget.open)}>
        <summary>{t('factory.formatIdentifiers')}</summary>
        {identifiers ? (
          <ul>
            {schemas.map((schema, index) => (
              <li
                key={`${schema.reference.identity}:${schema.reference.version}:${schema.reference.digest}`}
              >
                <strong>{choices[index]!.label}</strong>
                <p>
                  {schema.reference.identity} · {schema.reference.version} ·{' '}
                  {schema.reference.digest}
                </p>
              </li>
            ))}
          </ul>
        ) : null}
      </details>
    </>
  );
}
