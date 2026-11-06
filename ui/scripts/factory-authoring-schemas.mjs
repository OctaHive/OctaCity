/** Retains authoritative bounds for Factory documents, Work and operator commands. */
export function factoryAuthoringSchemas(document) {
  return reachableSchemas(document, [
    'FactoryRuntimeSettingsRequest',
    'FactoryConfigurationDefinition',
    'FactoryFlowDefinitionDraft',
    'ValidateFactoryGateProgramRequest',
    'AdmitManualFactoryWorkRequest',
    'FactoryNodeOperationRequest',
    'FactoryNodeResponseRequest',
    'ImportProjectSkillRequest',
  ]);
}

/** Setup selections use the same schema traversal without enlarging ordinary editor exports. */
export function factorySetupSchemas(document) {
  return reachableSchemas(document, [
    'FactoryPresetResolutionRequest',
    'FactoryReadinessSelection',
    'SourceAcceptanceMapping',
  ]);
}

/** Shares repeated, immutable constraints in generated TypeScript without widening their types. */
export function factorySchemaSource(schemas) {
  const occurrences = new Map();
  const shareable = (value) => !Array.isArray(value) && !Object.hasOwn(value, '__proto__');
  function count(value) {
    if (value === null || typeof value !== 'object') return;
    if (shareable(value)) {
      const literal = JSON.stringify(value);
      if (literal.length >= 24) occurrences.set(literal, (occurrences.get(literal) ?? 0) + 1);
    }
    Object.values(value).forEach(count);
  }
  count(schemas);
  const shared = new Map();
  const declarations = [];
  function emit(value) {
    if (value === null || typeof value !== 'object') return JSON.stringify(value);
    if (shareable(value)) {
      const literal = JSON.stringify(value);
      if ((occurrences.get(literal) ?? 0) > 1) {
        let name = shared.get(literal);
        if (!name) {
          name = `schema${shared.size}`;
          shared.set(literal, name);
          declarations.push(`const ${name} = ${emitEntries(value)} as const;`);
        }
        return name;
      }
    }
    if (Array.isArray(value)) return `[${value.map(emit).join(',')}]`;
    return emitEntries(value);
  }
  function emitEntries(value) {
    return `{${Object.entries(value)
      .map(([key, item]) => {
        const quoted = JSON.stringify(key);
        return `${key === '__proto__' ? `[${quoted}]` : quoted}:${emit(item)}`;
      })
      .join(',')}}`;
  }
  const expression = emit(schemas);
  return { declarations: declarations.join('\n'), expression };
}

function reachableSchemas(document, pending) {
  const source = document.components?.schemas ?? {};
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
    result[name] = validationSchema(schema);
    references(schema);
  }
  return result;
}

/** Documentation is retained in OpenAPI; the browser needs only executable validation facts. */
function validationSchema(schema) {
  if (schema === null || typeof schema !== 'object') return schema;
  return Object.fromEntries(
    Object.entries(schema)
      .filter(([key]) => key !== 'description')
      .map(([key, value]) => {
        if (key === 'properties')
          return [
            key,
            Object.fromEntries(
              Object.entries(value).map(([name, child]) => [name, validationSchema(child)]),
            ),
          ];
        if (['oneOf', 'anyOf', 'allOf'].includes(key)) return [key, value.map(validationSchema)];
        if (['items', 'additionalProperties', 'propertyNames'].includes(key))
          return [key, validationSchema(value)];
        return [key, value];
      }),
  );
}
