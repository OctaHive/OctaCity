import { readFile } from 'node:fs/promises';
import path from 'node:path';
import ts from 'typescript';

// Keep complete, type-checked source catalogs. Production stores translated
// values in the English key order instead of shipping every long key twice.
function catalog(code, name) {
  const source = ts.createSourceFile(`${name}.ts`, code, ts.ScriptTarget.Latest, true);
  const declaration = source.statements
    .filter(ts.isVariableStatement)
    .flatMap((statement) => [...statement.declarationList.declarations])
    .find((item) => ts.isIdentifier(item.name) && item.name.text === name);
  let object = declaration?.initializer;
  while (object && (ts.isSatisfiesExpression(object) || ts.isAsExpression(object)))
    object = object.expression;
  if (!object || !ts.isObjectLiteralExpression(object))
    throw new Error(`Expected a literal ${name} catalog`);
  const entries = new Map();
  for (const item of object.properties) {
    if (
      !ts.isPropertyAssignment(item) ||
      !ts.isStringLiteral(item.name) ||
      !ts.isStringLiteral(item.initializer) ||
      entries.has(item.name.text)
    )
      throw new Error(`Unsupported or duplicate entry in ${name}`);
    entries.set(item.name.text, item.initializer.text);
  }
  return entries;
}
/** @returns {import('vite').Plugin} */
export function compactMessageCatalogs() {
  return {
    name: 'compact-message-catalogs',
    apply: 'build',
    enforce: 'pre',
    async transform(code, id) {
      if (!id.replaceAll('\\', '/').endsWith('/app/presentation/messages.ru.ts')) return;
      const englishPath = path.join(path.dirname(id), 'messages.en.ts');
      this.addWatchFile(englishPath);
      const english = catalog(await readFile(englishPath, 'utf8'), 'enMessages');
      const russian = catalog(code, 'ruMessages');
      if (english.size !== russian.size || [...english.keys()].some((key) => !russian.has(key)))
        throw new Error('Translation catalog keys differ');
      return {
        code: `import {enMessages} from './messages.en';const translations=${JSON.stringify([...english.keys()].map((key) => russian.get(key)))};export const ruMessages=Object.fromEntries(Object.keys(enMessages).map((key,index)=>[key,translations[index]]));`,
        map: null,
      };
    },
  };
}
