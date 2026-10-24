import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import type { FactoryRolesApi } from './roleApi';
import styles from './Factory.module.css';

type Reference = components['schemas']['FactoryImmutableReferenceResource'];
/** Exact retained content is read only after explicit selection; file text is never rendered as HTML. */
export function SkillContents({
  api,
  project,
  reference,
}: {
  api: FactoryRolesApi;
  project: string;
  reference: Reference;
}) {
  const { t } = usePresentation();
  const [path, setPath] = useState<string | null>(null);
  const tree = useQuery({
    queryKey: queryKeys.factorySkill(project, reference, null),
    queryFn: ({ signal }) => api.getSkill(project, reference, null, signal),
    gcTime: 0,
  });
  const file = useQuery({
    queryKey: queryKeys.factorySkill(project, reference, path),
    enabled: path !== null,
    queryFn: ({ signal }) => api.getSkill(project, reference, path, signal),
    gcTime: 0,
  });
  const label = t('factory.viewSkill', { name: `${reference.identity} · ${reference.version}` });
  return (
    <section aria-label={label}>
      <h4>{label}</h4>
      {tree.error ? (
        <QueryFailureNotice title={label} error={tree.error} onRetry={tree.refetch} />
      ) : tree.data ? (
        <ul>
          {Object.keys(tree.data.bundle.files).map((name) => (
            <li key={name}>
              <button type="button" aria-pressed={path === name} onClick={() => setPath(name)}>
                {name}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <QueryLoadingNotice label={label} />
      )}
      {path !== null ? (
        file.error ? (
          <QueryFailureNotice title={path} error={file.error} onRetry={file.refetch} />
        ) : file.data?.file ? (
          <>
            <h5>{file.data.file.path}</h5>
            {file.data.file.text !== null ? (
              <pre className={styles.skillPreview}>{file.data.file.text}</pre>
            ) : (
              <p>{t('factory.noSkillTextPreview')}</p>
            )}
          </>
        ) : (
          <QueryLoadingNotice label={path} />
        )
      ) : null}
    </section>
  );
}

export function SkillReferences({
  api,
  project,
  skills,
}: {
  api: FactoryRolesApi;
  project: string;
  skills: Record<string, Reference>;
}) {
  const { t } = usePresentation();
  const [selected, setSelected] = useState<Reference | null>(null);
  return (
    <section aria-label={t('factory.skills')}>
      <h4>{t('factory.skills')}</h4>
      {Object.entries(skills).map(([alias, reference]) => (
        <button key={alias} type="button" onClick={() => setSelected(reference)}>
          {t('factory.viewSkill', { name: `${alias} · ${reference.version}` })}
        </button>
      ))}
      {!Object.keys(skills).length ? <p>{t('factory.noSkills')}</p> : null}
      {selected ? (
        <SkillContents
          key={`${selected.identity}:${selected.version}:${selected.digest}`}
          api={api}
          project={project}
          reference={selected}
        />
      ) : null}
    </section>
  );
}
