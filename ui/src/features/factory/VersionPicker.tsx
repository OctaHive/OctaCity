import { useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import styles from './Factory.module.css';

export function VersionPicker({
  current,
  selected,
  onChange,
}: {
  current: number;
  selected: number;
  onChange: (version: number) => void;
}) {
  const { t } = usePresentation();
  const [search, setSearch] = useState('');
  const requested = Number(search);
  const head = !search.trim()
    ? current
    : Number.isSafeInteger(requested) && requested > 0 && requested <= current
      ? requested
      : 0;
  const values = Array.from({ length: Math.min(head, 100) }, (_, index) => head - index);
  if (!values.includes(selected)) values.push(selected);
  return (
    <div className={styles.versionPicker}>
      <span>{t('factory.version')}</span>
      <SelectMenu
        search={
          current > 100
            ? { label: t('factory.findVersion'), value: search, onChange: setSearch }
            : undefined
        }
        ariaLabel={t('factory.version')}
        value={String(selected)}
        onValueChange={(value) => onChange(Number(value))}
        options={values.map((value) => ({
          value: String(value),
          label: t('factory.versionLabel', { version: value }),
        }))}
      />
    </div>
  );
}
