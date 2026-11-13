import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  classificationFields,
  issueFieldLabel,
  type IssueFieldChoices,
} from './IssueFieldContracts';
import { classificationParameters } from './issueClassification';
import { sourceReferenceKey } from '../../shared/sourceReferences';

type S = components['schemas'];
type MappingRead = { path: string; presence: boolean };

function mappingReads(mapping: S['FactoryFlowValueMapping']): MappingRead[] {
  switch (mapping.mapping) {
    case 'pointer':
      return [{ path: mapping.path, presence: false }];
    case 'object':
      return Object.values(mapping.fields).flatMap(mappingReads);
    case 'array':
      return mapping.items.flatMap(mappingReads);
    case 'if_present':
      return [
        { path: mapping.path, presence: true },
        ...mappingReads(mapping.then),
        ...mappingReads(mapping.otherwise),
      ];
    case 'constant':
      return [];
  }
}

export function TypedClassificationReview({
  action,
  choices,
}: {
  action: S['FactoryFlowNodeDefinition']['action'];
  choices: IssueFieldChoices;
}) {
  const { t } = usePresentation();
  const selected = classificationParameters(action)?.fields;
  if (!selected || typeof selected !== 'object') return null;
  return (
    <>
      <p>{t('planning.definition', { version: choices.definition_version })}</p>
      <ul>
        {classificationFields(choices)
          .filter(({ name, field }) =>
            field.source.kind === 'classification'
              ? 'properties' in selected &&
                !!selected.properties &&
                typeof selected.properties === 'object' &&
                Object.keys(selected.properties).includes(name)
              : Object.entries(selected).some(([key, value]) => key === name && value === true),
          )
          .map(({ name, field }) => (
            <li key={name}>{issueFieldLabel(field, t)}</li>
          ))}
      </ul>
    </>
  );
}

/** Publication reviews the frozen input policy and step projections, never material bodies. */
export function IssueInputReview({ flow }: { flow: S['FactoryFlowConfiguration'] }) {
  const { t } = usePresentation();
  const snapshot = flow.issue_snapshot;
  if (!snapshot) return null;
  const requirement = (required: boolean) =>
    t(required ? 'issueResults.required' : 'issueResults.optional');
  return (
    <section aria-label={t('factory.issueSnapshot')}>
      <h3>{t('factory.issueSnapshot')}</h3>
      <p>{t('planning.definition', { version: snapshot.definition_version })}</p>
      <ul>
        {Object.entries(snapshot.fields).map(([alias, field]) => (
          <li key={alias}>
            {alias} · {requirement(field.required)}
          </li>
        ))}
        {Object.entries(snapshot.materials ?? {}).map(([alias, material]) => (
          <li key={alias}>
            {alias} · {requirement(material.required)} · {material.source.kind} ·{' '}
            {material.source.kind === 'selected'
              ? material.source.max_entries
              : material.source.references.length}{' '}
            · {material.max_bytes} {t('factory.bytes')}
          </li>
        ))}
      </ul>
      <h4>{t('factory.snapshotProjections')}</h4>
      <ul>
        {flow.closure.definitions.flatMap((definition) =>
          definition.nodes.flatMap((node) => {
            const binding = node.input_binding;
            if (!binding) return [];
            const sources = Object.entries(binding.sources).filter(
              ([, source]) =>
                source.source === 'incoming' &&
                sourceReferenceKey(source.schema) === sourceReferenceKey(snapshot.schema),
            );
            const parts = (path: string) =>
              path
                .split('/')
                .slice(1)
                .map((part) => part.replaceAll('~1', '/').replaceAll('~0', '~'));
            const resolved = mappingReads(binding.mapping.value).flatMap(({ path, presence }) => {
              const [alias, ...tail] = parts(path);
              return sources.flatMap(([name, source]) => {
                // An empty mapping pointer forwards the whole source map.
                if (path !== '' && name !== alias) return [];
                const field = [...parts(source.path), ...(path === '' ? [] : tail)].join('/');
                return [{ path: field, presence }];
              });
            });
            const forwarded = resolved.filter((read) => !read.presence).map((read) => read.path);
            const selected = [
              ...new Set(
                forwarded.flatMap((field) =>
                  field
                    ? [field]
                    : [
                        ...Object.keys(snapshot.fields),
                        ...Object.keys(snapshot.materials ?? {}),
                        ...Object.keys(snapshot.evidence ?? {}),
                      ],
                ),
              ),
            ];
            const checks = [
              ...new Set(
                resolved
                  .filter(
                    (read) =>
                      read.presence &&
                      !forwarded.some(
                        (path) =>
                          path === '' || read.path === path || read.path.startsWith(`${path}/`),
                      ),
                  )
                  .map((read) => (read.path ? `/${read.path}` : t('factory.issueSnapshot'))),
              ),
            ];
            return selected.length || checks.length
              ? [
                  <li key={`${definition.reference.digest}:${node.key}`}>
                    {node.key} · {selected.join(', ')}
                    {selected.length && checks.length ? ' · ' : null}
                    {checks.length
                      ? t('factory.snapshotPresenceChecks', { fields: checks.join(', ') })
                      : null}
                  </li>,
                ]
              : [];
          }),
        )}
      </ul>
    </section>
  );
}
