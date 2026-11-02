import type { components } from '../../../.generated/api/schema';
import { useState } from 'react';
import type { SourceRepositories } from './SourceInputEditor';
import { AutomaticInputs } from './AutomaticInputs';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { FactoryConfigurationDetails } from './api';
import { factorySourcesApi, type FactorySourcesApi } from './sourceApi';
import styles from './Factory.module.css';

/** Manual Artifact intake remains independent of installed automatic-source capabilities. */
export function FactoryPipelineInputs({
  configuration,
  api = factorySourcesApi,
  onManual,
  repositories,
  initialInput,
}: {
  configuration: FactoryConfigurationDetails;
  api?: FactorySourcesApi;
  repositories?: SourceRepositories | undefined;
  onManual?: (() => void) | undefined;
  initialInput?: components['schemas']['WorkSourceInputIntent'] | undefined;
}) {
  const { t } = usePresentation();
  const [automatic, setAutomatic] = useState(false);
  // Consumption survives closing the automatic-input section; published inputs reopen from its catalog.
  const [publishedInitial, setPublishedInitial] = useState<string | null>(null);
  return (
    <section
      className={styles.pipelineInputs}
      aria-labelledby={`pipeline-inputs-${configuration.configuration.reference.id}`}
    >
      <h3 id={`pipeline-inputs-${configuration.configuration.reference.id}`}>
        {t('factory.inputs')}
      </h3>
      <p className={styles.supportingText}>{t('factory.inputsHelp')}</p>
      <div className={styles.definitionActions}>
        <button type="button" disabled={!onManual} onClick={onManual}>
          {t('factory.admitManual')}
        </button>
        <button type="button" aria-expanded={automatic} onClick={() => setAutomatic(!automatic)}>
          {t('factory.configureAutomatic')}
        </button>
      </div>
      {automatic ? (
        <AutomaticInputs
          configuration={configuration}
          api={api}
          repositories={repositories}
          initialInput={initialInput?.id === publishedInitial ? undefined : initialInput}
          onInitialPublished={() => {
            if (initialInput) setPublishedInitial(initialInput.id);
          }}
        />
      ) : null}
    </section>
  );
}
