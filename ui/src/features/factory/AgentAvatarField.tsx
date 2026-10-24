import { useState } from 'react';
import { usePresentation } from '../../app/presentation/PresentationProvider';

const SOURCE_BYTES = 4 * 1024 * 1024;
const AVATAR_EDGE = 96;
const SOURCE_EDGE = 4096;

/** Normalize one explicitly chosen raster in memory; no external URL or browser persistence. */
export function AgentAvatarField({
  value,
  onChange,
  onPendingChange,
}: {
  value: string | null | undefined;
  onChange: (value: string | null) => void;
  onPendingChange: (pending: boolean) => void;
}) {
  const { t } = usePresentation();
  const [error, setError] = useState(false);
  async function select(file: File | undefined) {
    if (!file) return;
    setError(false);
    onPendingChange(true);
    try {
      if (
        !['image/png', 'image/jpeg', 'image/webp'].includes(file.type) ||
        file.size > SOURCE_BYTES
      )
        throw new Error();
      const source = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () =>
          typeof reader.result === 'string' ? resolve(reader.result) : reject(new Error());
        reader.onerror = () => reject(new Error());
        reader.readAsDataURL(file);
      });
      const image = new Image();
      await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error());
        image.src = source;
      });
      if (
        !image.naturalWidth ||
        !image.naturalHeight ||
        image.naturalWidth > SOURCE_EDGE ||
        image.naturalHeight > SOURCE_EDGE
      )
        throw new Error();
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = AVATAR_EDGE;
      const context = canvas.getContext('2d');
      if (!context) throw new Error();
      const edge = Math.min(image.naturalWidth, image.naturalHeight);
      context.drawImage(
        image,
        (image.naturalWidth - edge) / 2,
        (image.naturalHeight - edge) / 2,
        edge,
        edge,
        0,
        0,
        AVATAR_EDGE,
        AVATAR_EDGE,
      );
      onChange(canvas.toDataURL('image/png'));
    } catch {
      setError(true);
    } finally {
      onPendingChange(false);
    }
  }
  return (
    <div>
      <label>
        {t('factory.agentAvatar')}
        <input
          type="file"
          accept="image/png,image/jpeg,image/webp"
          onChange={(event) => {
            const file = event.target.files?.[0];
            event.target.value = '';
            void select(file);
          }}
        />
      </label>
      {value ? (
        <>
          <img src={value} alt={t('factory.agentAvatarImage')} width={64} height={64} />
          <button type="button" onClick={() => onChange(null)}>
            {t('factory.removeAvatar')}
          </button>
        </>
      ) : null}
      {error ? <p role="alert">{t('factory.avatarError')}</p> : null}
    </div>
  );
}
