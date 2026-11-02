import Markdown from 'markdown-to-jsx/react';
import styles from './ProjectDocumentPublisher.module.css';

const omitImage = () => null;
function safeDocumentUrl(value: string) {
  try {
    return ['http:', 'https:', 'mailto:'].includes(new URL(value, window.location.href).protocol)
      ? value
      : null;
  } catch {
    return null;
  }
}

/** Render private Markdown as React elements, without raw HTML or remote image requests. */
export function DocumentMarkdown({ content }: { content: string }) {
  return (
    <Markdown
      className={styles.markdown}
      options={{
        disableParsingRawHTML: true,
        sanitizer: safeDocumentUrl,
        forceWrapper: true,
        overrides: { img: omitImage },
      }}
    >
      {content}
    </Markdown>
  );
}
