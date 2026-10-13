/** Validation paths connect summaries to controls, without placing draft names in fragment URLs. */
export function factoryFieldId(field: string) {
  return `factory-field-${encodeURIComponent(field)}`;
}
export function focusFactoryField(field: string) {
  const exact = document.getElementById(factoryFieldId(field));
  if (exact) {
    exact.focus();
    return;
  }
  const node = /^\/root\/nodes\/(\d+)(?:\/|$)/u.exec(field);
  if (node) {
    document.querySelector<HTMLButtonElement>(`[data-factory-node-index="${node[1]}"]`)?.click();
  } else if (
    /^\/(?:limits|root\/(?:entry|input_schema|execution|terminals))(?:\/|$)/u.test(field)
  ) {
    const settings = document.querySelector<HTMLButtonElement>('[data-factory-settings]');
    if (settings?.getAttribute('aria-expanded') === 'false') settings.click();
  }
  queueMicrotask(() => {
    const exact = document.getElementById(factoryFieldId(field));
    if (exact) {
      exact.focus();
      return;
    }
    const sections = [...document.querySelectorAll<HTMLElement>('[data-factory-field]')]
      .filter((element) => {
        const prefix = element.dataset.factoryField ?? '';
        return field === prefix || field.startsWith(`${prefix}/`);
      })
      .sort(
        (a, b) => (b.dataset.factoryField?.length ?? 0) - (a.dataset.factoryField?.length ?? 0),
      );
    sections[0]?.focus();
  });
}
