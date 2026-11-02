import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { PolicySummary } from './PolicySummary';
import { policyVersionKey, useFactoryPolicy, useFactoryPolicyCatalog } from './useFactoryPolicies';
import type { PolicyKind, PolicyRecord, PolicyRef } from './policyApi';
import styles from './Factory.module.css';

export function ConfigurationPolicyPicker({
  project,
  kind,
  selected,
  original,
  onSelect,
  onManage,
  offered,
  legacy,
  legacyUnavailable,
}: {
  project: string;
  kind: PolicyKind;
  selected: PolicyRef | null;
  original: PolicyRef | null;
  onSelect: (selection: PolicyRef) => void;
  onManage: (trigger: HTMLButtonElement) => void;
  offered?: PolicyRecord | undefined;
  legacyUnavailable: boolean;
  legacy?: components['schemas']['FactoryLegacyPolicyReference'] | null | undefined;
}) {
  const { t } = usePresentation();
  const label = t(kind === 'launch' ? 'policy.launchLabel' : 'policy.accessLabel');
  const catalog = useFactoryPolicyCatalog(project);
  const exact = useFactoryPolicy(project, selected);
  const record = exact.data?.policy;
  const options = new Map(
    catalog.data?.pages
      .flatMap((page) => page.items)
      .filter((item) => item.reference.kind === kind)
      .map((item) => [policyVersionKey(item.reference), item]),
  );
  const current = selected ? policyVersionKey(selected) : '';
  const originalKey = original ? policyVersionKey(original) : '';
  const historic = useFactoryPolicy(
    project,
    original && originalKey !== current && !options.has(originalKey) ? original : null,
  );
  if (original && originalKey !== current && !options.has(originalKey))
    options.set(originalKey, {
      name: historic.data?.policy.policy.definition.name ?? t('policy.selectedVersion'),
      reference: original,
    });
  const offer = offered && {
    name: offered.policy.definition.name,
    reference: { kind: offered.kind, reference: offered.policy.reference },
  };
  if (offer) options.set(policyVersionKey(offer.reference), offer);
  if (selected && !options.has(current))
    options.set(current, {
      name: record?.policy.definition.name ?? t('policy.selectedVersion'),
      reference: selected,
    });
  return (
    <div
      className={styles.selectField}
      data-factory-field={kind === 'launch' ? '/admission_policy' : '/permission_ceiling'}
      tabIndex={-1}
    >
      <span>{label}</span>
      <SelectMenu
        ariaLabel={label}
        value={current}
        options={[
          ...(!selected ? [{ value: '', label: t('policy.selectPolicy') }] : []),
          ...Array.from(options, ([value, item]) => ({
            value,
            label: `${item.name} · v${item.reference.reference.version}`,
          })),
        ]}
        onValueChange={(value) => {
          const item = options.get(value);
          if (item) onSelect(item.reference);
        }}
      />
      {!selected && legacyUnavailable ? <p>{t('policy.referenceUnavailable')}</p> : null}
      {!selected ? (
        <p>
          {t(
            legacy && !legacyUnavailable
              ? 'factory.unmappedLegacyPolicy'
              : 'policy.selectPolicyHelp',
          )}
        </p>
      ) : null}
      {catalog.isPending ? <QueryLoadingNotice label={label} /> : null}
      {catalog.error ? (
        <QueryFailureNotice title={label} error={catalog.error} onRetry={catalog.refetch} />
      ) : null}
      {catalog.hasNextPage ? (
        <button
          type="button"
          onClick={() => void catalog.fetchNextPage()}
          disabled={catalog.isFetchingNextPage}
        >
          {t('policy.more')}
        </button>
      ) : null}
      {selected && exact.error ? (
        <QueryFailureNotice
          title={t('policy.referenceUnavailable')}
          error={exact.error}
          onRetry={exact.refetch}
        />
      ) : null}
      {selected && exact.isPending ? <QueryLoadingNotice label={label} /> : null}
      {record ? <PolicySummary record={record} /> : null}
      {legacy || selected ? (
        <details>
          <summary>{t('policy.bindingDetails')}</summary>
          <dl className={styles.definitionFacts}>
            {legacy ? (
              <div>
                <dt>{t('policy.originalReference')}</dt>
                <dd>
                  {legacy.reference.identity} · {legacy.reference.version} ·{' '}
                  {legacy.reference.digest}
                </dd>
              </div>
            ) : null}
            {selected ? (
              <div>
                <dt>{t('policy.exactReference')}</dt>
                <dd>
                  {selected.reference.id} · v{selected.reference.version} ·{' '}
                  {selected.reference.digest}
                </dd>
              </div>
            ) : null}
          </dl>
        </details>
      ) : null}
      <button type="button" onClick={(event) => onManage(event.currentTarget)}>
        {t(kind === 'launch' ? 'policy.manageLaunch' : 'policy.manageAccess')}
      </button>
      {offer && policyVersionKey(offer.reference) !== current ? (
        <button type="button" onClick={() => onSelect(offer.reference)}>
          {t('policy.adopt', { name: offer.name, version: offer.reference.reference.version })}
        </button>
      ) : null}
    </div>
  );
}
