/** Retains the authoritative bounds for the two editable Factory documents and their references. */
export function factoryAuthoringSchemas(document) {
  const source = document.components?.schemas ?? {};
  const pending = ['FactoryConfigurationDefinition', 'FactoryFlowDefinitionDraft'];
  const result = {};
  function references(value) {
    if (value === null || typeof value !== 'object') return;
    if (typeof value.$ref === 'string') {
      const prefix = '#/components/schemas/';
      if (!value.$ref.startsWith(prefix))
        throw new Error('Factory authoring schema must use local references.');
      pending.push(value.$ref.slice(prefix.length));
    }
    for (const item of Object.values(value)) references(item);
  }
  while (pending.length) {
    const name = pending.pop();
    if (Object.hasOwn(result, name)) continue;
    const schema = source[name];
    if (!schema) throw new Error(`Factory authoring schema ${name} is missing.`);
    result[name] = schema;
    references(schema);
  }
  return result;
}
