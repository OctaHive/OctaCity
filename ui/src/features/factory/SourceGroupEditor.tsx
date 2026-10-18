import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SourcePublication } from './SourcePublication';
import type { FactorySourcesApi, SourceSnapshot, SourceIntent } from './sourceApi';

type Input = components['schemas']['WorkSourceInput'];
type Group = components['schemas']['WorkSourceGroup'];
/** Members are exact published references. Group publication never silently replaces input versions. */
export function SourceGroupEditor({
  project,
  api,
  inputs,
  group,
  onRefresh,
}: {
  project: string;
  api: FactorySourcesApi;
  inputs: Input[];
  group?: Group | undefined;
  onRefresh: () => Promise<SourceSnapshot>;
}) {
  const { t } = usePresentation();
  const [id] = useState(() => group?.id ?? crypto.randomUUID());
  const [version, setVersion] = useState(group?.version ?? null);
  // Retain every selected immutable snapshot, including choices added after opening.
  const [members, setMembers] = useState(() => group?.members ?? []);
  const options = [
    ...new Map(
      [...inputs, ...members].map((input) => [`${input.id}:${input.version}`, input]),
    ).values(),
  ];
  const intent: SourceIntent | null =
    members.length >= 2
      ? {
          project_id: project,
          inputs: [],
          group: {
            id,
            expected_version: version,
            members: members.map((input) => ({ id: input.id, version: input.version })),
          },
        }
      : null;
  async function refresh() {
    const { records } = await onRefresh();
    const current = records.find(
      (record) => record.kind === 'group' && record.definition.id === id,
    );
    if (version !== null && (!current || current.kind !== 'group')) {
      throw new Error('Current group is unavailable');
    }
    if (current?.kind === 'group') setVersion(current.definition.version);
  }
  return (
    <fieldset>
      <legend>{t('factory.exclusiveGroup')}</legend>
      <p>{t('factory.exclusiveHelp')}</p>
      {options.map((input) => (
        <label key={`${input.id}:${input.version}`}>
          <input
            type="checkbox"
            checked={members.some((m) => m.id === input.id && m.version === input.version)}
            onChange={(event) =>
              setMembers((current) => [
                ...current.filter((m) => m.id !== input.id),
                ...(event.target.checked ? [input] : []),
              ])
            }
          />
          {input.definition.source.resource.identity} ·{' '}
          {input.definition.source.configuration.identity} · {input.id} · v{input.version} →{' '}
          {input.definition.target.id} · v{input.definition.target.version}
        </label>
      ))}
      <SourcePublication
        summary={`${t('factory.exclusiveHelp')} ${members
          .map(
            (input) =>
              `${input.definition.source.configuration.identity} · ${input.id} · v${input.version} → ${input.definition.target.id} · v${input.definition.target.version}`,
          )
          .join('; ')}`}
        intent={intent}
        api={api}
        onRefresh={refresh}
        onPublished={(result) => {
          if (result.publication.group) setVersion(result.publication.group.group.version);
        }}
      />
    </fieldset>
  );
}
