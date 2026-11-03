import { readFile } from 'node:fs/promises';
import path from 'node:path';
import ts from 'typescript';

// Keep complete, type-checked source catalogs. Production stores English key prefixes
// once per run and translated values in the reconstructed English key order.
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

// A packed list avoids repeated string delimiters. Keep the array whenever
// the separator is part of a message so decoding is lossless.
const separator = '|';
const pack = (values) =>
  values.some((value) => value.includes(separator)) ? values : values.join(separator);

/** @returns {import('vite').Plugin} */
export function compactMessageCatalogs() {
  return {
    name: 'compact-message-catalogs',
    apply: 'build',
    enforce: 'pre',
    async transform(code, id) {
      const file = id.replaceAll('\\', '/');
      if (file.endsWith('/app/presentation/messages.en.ts')) {
        const groups = [];
        for (const [key, value] of catalog(code, 'enMessages')) {
          const boundary = key.indexOf('.') + 1;
          const prefix = key.slice(0, boundary);
          if (groups.at(-1)?.[0] !== prefix) groups.push([prefix, []]);
          groups.at(-1)[1].push([key.slice(boundary), value]);
        }
        const packed = groups.map(([prefix, items]) => [
          prefix,
          pack(items.map(([key]) => key)),
          pack(items.map(([, value]) => value)),
        ]);
        return {
          code: `const groups=${JSON.stringify(packed)};export const unpackMessages=values=>typeof values==='string'?values.split(${JSON.stringify(separator)}):values;export const enMessages=Object.fromEntries(groups.flatMap(([prefix,keys,text])=>{const values=unpackMessages(text);return unpackMessages(keys).map((key,index)=>[prefix+key,values[index]])}));`,
          map: null,
        };
      }
      if (!file.endsWith('/app/presentation/messages.ru.ts')) return;
      const englishPath = path.join(path.dirname(id), 'messages.en.ts');
      this.addWatchFile(englishPath);
      const english = catalog(await readFile(englishPath, 'utf8'), 'enMessages');
      const russian = catalog(code, 'ruMessages');
      if (english.size !== russian.size || [...english.keys()].some((key) => !russian.has(key)))
        throw new Error('Translation catalog keys differ');
      return {
        code: `import {enMessages,unpackMessages} from './messages.en';const translations=unpackMessages(${JSON.stringify(pack([...english.keys()].map((key) => russian.get(key))))});export const ruMessages=Object.fromEntries(Object.keys(enMessages).map((key,index)=>[key,translations[index]]));`,
        map: null,
      };
    },
  };
}
