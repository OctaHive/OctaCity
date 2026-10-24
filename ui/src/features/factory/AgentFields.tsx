import { Bot, Code, Shield, Lightbulb } from 'lucide-react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SelectMenu } from '../../shared/SelectMenu';
import { InstructionFields } from './InstructionFields';

const icons = { bot: Bot, code: Code, shield: Shield, lightbulb: Lightbulb };
type Definition = components['schemas']['FactoryRoleDefinition'];
export function AgentIcon({ name }: { name: string | null | undefined }) {
  if (!name) return null;
  const Icon = icons[name as keyof typeof icons] ?? Bot;
  return <Icon role="img" aria-label={name} size={20} />;
}
/** Shared identity/instruction fields for custom creation, replacement and independent template copies. */
export function AgentFields({
  value,
  onChange,
}: {
  value: Definition;
  onChange: (value: Definition) => void;
}) {
  const { t } = usePresentation();
  return (
    <>
      <label>
        {t('factory.agentName')}
        <input
          value={value.name}
          onChange={(event) => onChange({ ...value, name: event.target.value })}
        />
      </label>
      <label>
        {t('factory.agentDescription')}
        <textarea
          value={value.description ?? ''}
          onChange={(event) => onChange({ ...value, description: event.target.value })}
        />
      </label>
      <SelectMenu
        ariaLabel={t('factory.agentIcon')}
        value={value.icon ?? ''}
        options={[
          { value: '', label: t('factory.none') },
          ...Object.keys(icons).map((name) => ({
            value: name,
            label: name,
            icon: <AgentIcon name={name} />,
          })),
        ]}
        onValueChange={(icon) => onChange({ ...value, icon: icon || null })}
      />
      <label>
        {t('factory.agentConcurrency')}
        <input
          type="number"
          min={1}
          value={value.concurrency_limit ?? ''}
          onChange={(event) =>
            onChange({
              ...value,
              concurrency_limit: event.target.value === '' ? null : Number(event.target.value),
            })
          }
        />
      </label>
      <InstructionFields
        label={t('factory.sharedInstruction')}
        value={value.instructions}
        onChange={(instructions) => onChange({ ...value, instructions })}
      />
    </>
  );
}
