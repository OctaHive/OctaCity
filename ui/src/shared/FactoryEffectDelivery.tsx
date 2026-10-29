import type { components } from '../../.generated/api/schema';
import { usePresentation } from '../app/presentation/PresentationProvider';

/** A retained receipt is separate from gate acceptance, local Issue state and current authority. */
export function FactoryEffectDelivery({
  record,
}: {
  record: components['schemas']['FactoryEffectDeliveryDiagnostic'];
}) {
  const { t } = usePresentation();
  return (
    <section
      aria-label={t(
        record.kind === 'work.status.projection' ? 'triageResult.reporter' : 'triageResult.effect',
      )}
    >
      <h3>
        {t(
          record.kind === 'work.status.projection'
            ? 'triageResult.reporter'
            : 'triageResult.effect',
        )}
      </h3>
      <p>{t(`triageResult.delivery.${record.state}`)}</p>
      {record.failure ? <p>{t(`triageResult.failure.${record.failure}`)}</p> : null}
      <p>{t('triageResult.deliveryHelp')}</p>
    </section>
  );
}
