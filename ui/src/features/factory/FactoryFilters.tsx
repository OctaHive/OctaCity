import { useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import {
  FACTORY_STATES,
  FACTORY_DECISIONS,
  FACTORY_DELIVERY_STATES,
  readFactoryFilters,
} from './filters';
import styles from './Factory.module.css';

/** Safe shareable filters; authoring drafts never enter the URL. */
export function FactoryFiltersPanel() {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const filters = readFactoryFilters(params);
  const update = (key: string, value: string) => {
    const next = new URLSearchParams(params);
    next.delete('after');
    if (value === '') next.delete(key);
    else next.set(key, value);
    setParams(next);
  };
  return (
    <section
      className={styles.filters}
      data-factory-filters
      tabIndex={-1}
      aria-label={t('factory.filters')}
    >
      <h3>{t('factory.filters')}</h3>
      <form
        key={`${filters.source}:${filters.admitted_from_unix_ms}:${filters.admitted_before_unix_ms}`}
        onSubmit={(event) => {
          event.preventDefault();
          const form = new FormData(event.currentTarget);
          const next = new URLSearchParams(params);
          next.delete('after');
          const keys = ['source', 'admitted_from_unix_ms', 'admitted_before_unix_ms'] as const;
          for (const key of keys) {
            const value = String(form.get(key) ?? '');
            if (value) next.set(key, value);
            else next.delete(key);
          }
          const bounded = readFactoryFilters(next);
          for (const key of keys) {
            next.delete(key);
            const value = bounded[key];
            if (value !== undefined) next.set(key, String(value));
          }
          setParams(next);
        }}
      >
        <label>
          {t('factory.source')}
          <input
            name="source"
            maxLength={128}
            pattern="[a-zA-Z0-9_.-]{1,128}"
            defaultValue={filters.source ?? ''}
          />
        </label>
        <label>
          {t('factory.admittedFrom')}
          <input
            name="admitted_from_unix_ms"
            type="number"
            min={0}
            max={Number.MAX_SAFE_INTEGER}
            defaultValue={filters.admitted_from_unix_ms ?? ''}
          />
        </label>
        <label>
          {t('factory.admittedBefore')}
          <input
            name="admitted_before_unix_ms"
            type="number"
            min={0}
            max={Number.MAX_SAFE_INTEGER}
            defaultValue={filters.admitted_before_unix_ms ?? ''}
          />
        </label>
        <button type="submit">{t('factory.applyFilters')}</button>
      </form>
      <span>{t('factory.state')}</span>
      <SelectMenu
        ariaLabel={t('factory.state')}
        value={filters.state ?? ''}
        options={[
          { value: '', label: t('factory.all') },
          ...Object.entries(FACTORY_STATES).map(([value, label]) => ({ value, label })),
        ]}
        onValueChange={(value) => update('state', value)}
      />
      <span>{t('factory.decision')}</span>
      <SelectMenu
        ariaLabel={t('factory.decision')}
        value={filters.decision ?? ''}
        options={[
          { value: '', label: t('factory.all') },
          ...Object.entries(FACTORY_DECISIONS).map(([value, label]) => ({ value, label })),
        ]}
        onValueChange={(value) => update('decision', value)}
      />
      <span>{t('factory.delivery')}</span>
      <SelectMenu
        ariaLabel={t('factory.delivery')}
        value={filters.delivery_state ?? ''}
        options={[
          { value: '', label: t('factory.all') },
          ...Object.entries(FACTORY_DELIVERY_STATES).map(([value, label]) => ({ value, label })),
        ]}
        onValueChange={(value) => update('delivery_state', value)}
      />
    </section>
  );
}
