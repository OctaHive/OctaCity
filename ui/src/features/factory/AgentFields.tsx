import { Bot, Code, Shield, Lightbulb } from 'lucide-react';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { AgentAvatarField } from './AgentAvatarField';
import { InstructionFields } from './InstructionFields';
import { SkillFolderFields } from './SkillFolderFields';
import type { FactoryRolesApi } from './roleApi';

const icons = { bot: Bot, code: Code, shield: Shield, lightbulb: Lightbulb };
type Definition = components['schemas']['FactoryRoleDefinition'];
export function AgentIcon({
  name,
  avatar,
}: {
  name: string | null | undefined;
  avatar?: string | null | undefined;
}) {
  const { t } = usePresentation();
  if (avatar)
    return <img src={avatar} alt={t('factory.agentAvatarImage')} width={40} height={40} />;
  if (!name) return null;
  const Icon = icons[name as keyof typeof icons] ?? Bot;
  return <Icon role="img" aria-label={name} size={20} />;
}
/** Shared identity/instruction fields for custom creation, replacement and independent template copies. */
export function AgentFields({
  value,
  onChange,
  api,
  projectId,
  onPendingChange,
}: {
  value: Definition;
  onChange: (value: Definition) => void;
  api: FactoryRolesApi;
  projectId: string;
  onPendingChange: (pending: boolean) => void;
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
      <AgentAvatarField
        value={value.avatar}
        onPendingChange={onPendingChange}
        onChange={(avatar) => onChange({ ...value, avatar, icon: null })}
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
      <SkillFolderFields
        api={api}
        project={projectId}
        value={value.skills ?? {}}
        onChange={(skills) => onChange({ ...value, skills })}
      />
    </>
  );
}
