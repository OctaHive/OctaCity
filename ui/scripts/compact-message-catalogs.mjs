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

// Message lists follow English key order. Reference an earlier literal only
// when it saves space, and double the marker on a literal that starts with it.
function packTranslations(values) {
  const first = new Map();
  return pack(
    values.map((value, index) => {
      const earlier = first.get(value);
      if (earlier === undefined) first.set(value, index);
      const literal = value.startsWith('~') ? `~${value}` : value;
      if (earlier === undefined) return literal;
      const reference = `~${earlier.toString(36)}`;
      return reference.length < literal.length ? reference : literal;
    }),
  );
}

/** @returns {import('vite').Plugin} */
export function compactMessageCatalogs() {
  return {
    name: 'compact-message-catalogs',
    apply: 'build',
    enforce: 'pre',
    async transform(code, id) {
      const file = id.replaceAll('\\', '/');
      if (file.endsWith('/app/presentation/messages.en.ts')) {
        const english = catalog(code, 'enMessages');
        const groups = [];
        for (const key of english.keys()) {
          const boundary = key.indexOf('.') + 1;
          const prefix = key.slice(0, boundary);
          if (groups.at(-1)?.[0] !== prefix) groups.push([prefix, []]);
          groups.at(-1)[1].push(key.slice(boundary));
        }
        const packed = groups.map(([prefix, keys]) => [prefix, pack(keys)]);
        return {
          code: `const groups=${JSON.stringify(packed)};
const unpack=values=>typeof values==='string'?values.split(${JSON.stringify(separator)}):values;
export function unpackMessages(packed){
  const values=unpack(packed);
  for(let i=0;i<values.length;i++){
    const value=values[i];
    if(value[0]==='~')values[i]=value[1]==='~'?value.slice(1):values[parseInt(value.slice(1),36)];
  }
  return values;
}
const english=unpackMessages(${JSON.stringify(packTranslations([...english.values()]))});
export const enMessages=Object.fromEntries(groups.flatMap(([prefix,keys])=>unpack(keys).map(key=>prefix+key)).map((key,index)=>[key,english[index]]));`,
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
        code: `import {enMessages,unpackMessages} from './messages.en';const translations=unpackMessages(${JSON.stringify(packTranslations([...english.keys()].map((key) => russian.get(key))))});export const ruMessages=Object.fromEntries(Object.keys(enMessages).map((key,index)=>[key,translations[index]]));`,
        map: null,
      };
    },
  };
}
