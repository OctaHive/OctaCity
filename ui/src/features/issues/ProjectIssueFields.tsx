import { useId, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { IssuesApi, IssuePropertyCatalog, IssuePropertiesPublication } from './api';
import { fieldCatalogProblems, reconcileFieldCatalog } from './issueFieldCatalog';
import { issueBuiltinChoices } from './issueBuiltinFields';
import { IssueFieldCatalogReview } from './IssueFieldCatalogReview';
import { IssueFieldSemantics } from './IssueFieldSemantics';
import { IssueFieldDefinitions } from './IssueFieldDefinitions';
import { IssueChoiceList } from './IssueChoiceList';
import styles from './Issues.module.css';

type Api = Pick<IssuesApi, 'planningChoices' | 'publishProperties'>;
/** Project publication remains independent of Issue edits and execution. Collapsing retains the draft. */
export function ProjectIssueFields({ api, projectId }: { api: Api; projectId: string }) {
  const { t } = usePresentation();
  const id = useId();
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState(false);
  return (
    <section className={styles.page} aria-label={t('issueFields.settings')}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => {
          setLoaded(true);
          setOpen(!open);
        }}
      >
        {t('issueFields.settings')}
      </button>
      <div id={id} hidden={!open}>
        {loaded ? <Catalog api={api} projectId={projectId} /> : null}
      </div>
    </section>
  );
}
function Catalog({ api, projectId }: { api: Api; projectId: string }) {
  const { t } = usePresentation();
  const query = useQuery({
    queryKey: queryKeys.issuePropertyChoices(projectId),
    gcTime: 0,
    queryFn: ({ signal }) => api.planningChoices(projectId, signal),
  });
  return (
    <QueryContent query={query} label={t('issueFields.settings')}>
      {(data) =>
        data.definitions.project_id === projectId ? (
          <Editor
            api={api}
            catalog={data.definitions}
            ready={!query.isFetching && !query.error}
            refresh={query.refetch}
          />
        ) : (
          <p role="alert">{t('issueFields.catalogMismatch')}</p>
        )
      }
    </QueryContent>
  );
}
function initialDefinition(catalog: IssuePropertyCatalog) {
  const choices = issueBuiltinChoices(catalog.definition);
  return structuredClone({
    ...catalog.definition,
    priorities: [...choices.priority],
    components: [...choices.component],
    types: [...choices.type],
  });
}
function Editor({
  api,
  catalog,
  ready,
  refresh,
}: {
  api: Api;
  catalog: IssuePropertyCatalog;
  ready: boolean;
  refresh: () => Promise<unknown>;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [discardGeneration, setDiscardGeneration] = useState(0);
  const [baseline, setBaseline] = useState(catalog);
  const [draft, setDraft] = useState(() => initialDefinition(catalog));
  const [resolutions, setResolutions] = useState<{
    draft: typeof draft;
    version: number;
    choices: Record<string, 'local' | 'current'>;
  } | null>(null);
  const [conflicts, setConflicts] = useState<string[]>([]);
  const [review, setReview] = useState<IssuePropertiesPublication | null>(null);
  const problems = fieldCatalogProblems(draft);
  const buttonRef = useRef<HTMLButtonElement>(null);
  return (
    <section className={styles.panel}>
      <h2>{t('issueFields.settings')}</h2>
      <p>{t('issueFields.scope')}</p>
      <p>{t('issueFields.version', { version: baseline.version })}</p>
      {baseline.version !== catalog.version ? (
        <section aria-label={t('issueFields.stale')}>
          <p role="alert">
            {t('issueFields.stale')} · {t('issueFields.version', { version: catalog.version })}
          </p>
          {conflicts.length ? (
            <p role="alert">{t('issueFields.conflicts', { fields: conflicts.join(', ') })}</p>
          ) : null}
          {conflicts.map((key) => (
            <SelectMenu
              key={key}
              ariaLabel={t('issueFields.resolveConflict', { key })}
              value={
                resolutions?.draft === draft && resolutions.version === catalog.version
                  ? (resolutions.choices[key] ?? '')
                  : ''
              }
              options={[
                { value: '', label: t('factory.chooseInput') },
                { value: 'local', label: t('issueFields.keepLocal') },
                { value: 'current', label: t('issueFields.useCurrent') },
              ]}
              onValueChange={(choice) => {
                if (choice === 'local' || choice === 'current')
                  setResolutions({
                    draft,
                    version: catalog.version,
                    choices: {
                      ...(resolutions?.draft === draft && resolutions.version === catalog.version
                        ? resolutions.choices
                        : {}),
                      [key]: choice,
                    },
                  });
              }}
            />
          ))}
          {conflicts.length ? (
            <details>
              <summary>{t('issueFields.currentCatalog')}</summary>
              <IssueFieldCatalogReview definition={initialDefinition(catalog)} />
            </details>
          ) : null}
          <button
            type="button"
            disabled={!ready || review !== null}
            onClick={() => {
              const result = reconcileFieldCatalog(
                initialDefinition(baseline),
                draft,
                initialDefinition(catalog),
                resolutions?.draft === draft && resolutions.version === catalog.version
                  ? resolutions.choices
                  : {},
              );
              setConflicts(result.conflicts);
              if (!result.conflicts.length) {
                setBaseline(catalog);
                setDraft(initialDefinition({ ...catalog, definition: result.definition }));
              }
            }}
          >
            {t('issueFields.reconcile')}
          </button>
        </section>
      ) : null}
      <fieldset key={discardGeneration} disabled={review !== null}>
        <IssueChoiceList
          label={t('issueFields.priorities')}
          choices={draft.priorities}
          onChange={(priorities) => setDraft({ ...draft, priorities })}
        />
        <IssueChoiceList
          label={t('issueFields.components')}
          choices={draft.components}
          onChange={(components) => setDraft({ ...draft, components })}
        />
        <IssueChoiceList
          label={t('issueFields.types')}
          choices={draft.types}
          onChange={(types) => setDraft({ ...draft, types })}
        />
        <IssueFieldDefinitions
          fields={draft.properties}
          onChange={(properties) => setDraft({ ...draft, properties })}
        />
        <IssueFieldSemantics
          definition={draft}
          onChange={(classification) => setDraft({ ...draft, classification })}
        />
        <p>{t('issueFields.fixedSize', { values: bounds.sizes.join(', ') })}</p>
      </fieldset>
      {problems.length ? (
        <p role="alert">{t('issueFields.invalid', { fields: problems.join(', ') })}</p>
      ) : null}
      <button
        ref={buttonRef}
        type="button"
        disabled={
          !ready || problems.length > 0 || review !== null || baseline.version !== catalog.version
        }
        onClick={() =>
          setReview({ expected_version: baseline.version, definition: structuredClone(draft) })
        }
      >
        {t('issueFields.review')}
      </button>
      <button
        type="button"
        disabled={!ready || review !== null}
        onClick={() => {
          setBaseline(catalog);
          setDraft(initialDefinition(catalog));
          setConflicts([]);
          setResolutions(null);
          setDiscardGeneration((generation) => generation + 1);
        }}
      >
        {t('issueFields.discard')}
      </button>
      {review ? (
        <ConfirmedCommand
          title={t('issueFields.review')}
          confirmLabel={t('issueFields.publish')}
          consequence={`${t('issueFields.version', { version: review.expected_version })}. ${t('issueFields.scope')}`}
          request={review}
          returnFocus={buttonRef.current!}
          ready={ready && review.expected_version === catalog.version}
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.issuePropertyChoices(catalog.project_id) }]}
          renderReview={<IssueFieldCatalogReview definition={review.definition} />}
          execute={async (attempt) => {
            const result = await api.publishProperties(
              catalog.project_id,
              structuredClone(attempt.request) as IssuePropertiesPublication,
              attempt.headers,
            );
            setBaseline(result.resource);
            setDraft(initialDefinition(result.resource));
            return result;
          }}
          onClose={() => setReview(null)}
          onRefresh={refresh}
          renderSuccess={() => <p>{t('issueFields.published')}</p>}
        />
      ) : null}
    </section>
  );
}
