import { useState } from 'react';
import type { SourceRepositories } from './SourceInputEditor';
import { AutomaticInputs } from './AutomaticInputs';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { FactoryConfigurationDetails } from './api';
import { factorySourcesApi, type FactorySourcesApi } from './sourceApi';

/** Manual Artifact intake remains independent of installed automatic-source capabilities. */
export function FactoryPipelineInputs({
  configuration,
  api = factorySourcesApi,
  onManual,
  repositories,
}: {
  configuration: FactoryConfigurationDetails;
  api?: FactorySourcesApi;
  repositories?: SourceRepositories | undefined;
  onManual?: (() => void) | undefined;
}) {
  const { t } = usePresentation();
  const [automatic, setAutomatic] = useState(false);
  return (
    <section aria-labelledby={`pipeline-inputs-${configuration.configuration.reference.id}`}>
      <h3 id={`pipeline-inputs-${configuration.configuration.reference.id}`}>
        {t('factory.inputs')}
      </h3>
      <p>{t('factory.inputsHelp')}</p>
      <button type="button" disabled={!onManual} onClick={onManual}>
        {t('factory.admitManual')}
      </button>
      <button type="button" aria-expanded={automatic} onClick={() => setAutomatic(!automatic)}>
        {t('factory.configureAutomatic')}
      </button>
      {automatic ? (
        <AutomaticInputs configuration={configuration} api={api} repositories={repositories} />
      ) : null}
    </section>
  );
}
