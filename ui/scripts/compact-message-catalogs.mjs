import { Buffer } from 'node:buffer';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import ts from 'typescript';

// Keep complete, type-checked source catalogs. Production stores English key prefixes
// once per section and translated values in the reconstructed English key order.
const prefixOf = (key) => key.slice(0, key.lastIndexOf('.') + 1);

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
  return new Map([...Map.groupBy(entries, ([key]) => prefixOf(key)).values()].flat());
}

// A packed list avoids repeated string delimiters. Keep the array whenever
// the separator is part of a message so decoding is lossless.
const separator = '|';
const pack = (values) =>
  values.some((value) => value.includes(separator)) ? values : values.join(separator);

// Within a section, a suffix can reuse the beginning of its preceding suffix.
function packKeys(keys) {
  return pack(
    keys.map((key, index) => {
      const previous = keys[index - 1] ?? '';
      let length = 0;
      while (length < key.length && key[length] === previous[length]) length++;
      const reference = `~${length.toString(36)}:${key.slice(length)}`;
      const literal = key.startsWith('~') ? `~${key}` : key;
      return reference.length < literal.length ? reference : literal;
    }),
  );
}

// Message lists follow English key order. Reference an earlier literal only
// when it saves space, and double the marker on a literal that starts with it.
function packTranslations(values) {
  const first = new Map();
  const unique = [...new Set(values)];
  const words = new Map();
  const wordPattern = /[\p{L}]{4,}/gu;
  for (const value of unique) {
    for (const [word] of value.matchAll(wordPattern)) words.set(word, (words.get(word) ?? 0) + 1);
  }
  const dictionary = new Map();
  for (const [word, count] of [...words].sort((a, b) => b[1] - a[1])) {
    const reference = `^${dictionary.size.toString(36)};`;
    const bytes = Buffer.byteLength(word);
    if (count * (bytes - reference.length) > bytes + 1) dictionary.set(word, reference);
  }
  const compressed = pack(
    values.map((value, index) => {
      const earlier = first.get(value);
      if (earlier === undefined) first.set(value, index);
      const escaped = value
        .replaceAll('^', '^^')
        .replace(wordPattern, (word) => dictionary.get(word) ?? word);
      const literal = escaped.startsWith('~') ? `~${escaped}` : escaped;
      if (earlier === undefined) return literal;
      const reference = `~${earlier.toString(36)}`;
      return reference.length < literal.length ? reference : literal;
    }),
  );
  return [compressed, [...dictionary.keys()].join(separator)];
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
          const prefix = prefixOf(key);
          if (groups.at(-1)?.[0] !== prefix) groups.push([prefix, []]);
          groups.at(-1)[1].push(key.slice(prefix.length));
        }
        const packed = groups.map(([prefix, keys]) => [prefix, packKeys(keys)]);
        return {
          code: `const groups=${JSON.stringify(packed)};
const unpack=values=>typeof values==='string'?values.split(${JSON.stringify(separator)}):values;
export function unpackMessages([packed,words]){
  const values=unpack(packed);
  const dictionary=unpack(words);
  for(let i=0;i<values.length;i++){
    const value=values[i];
    if(value[0]==='~'&&value[1]!=='~')values[i]=values[parseInt(value.slice(1),36)];
    else values[i]=(value.startsWith('~~')?value.slice(1):value).replace(/\\^([a-z0-9]+;|\\^)/g,(_,key)=>key==='^'?'^':dictionary[parseInt(key,36)]);
  }
  return values;
}
const english=unpackMessages(${JSON.stringify(packTranslations([...english.values()]))});
const keys=packed=>{
  let previous='';
  return unpack(packed).map(key=>{
    if(key[0]==='~'){
      const boundary=key.indexOf(':',1);
      key=key[1]==='~'?key.slice(1):previous.slice(0,parseInt(key.slice(1,boundary),36))+key.slice(boundary+1);
    }
    return previous=key;
  });
};
export const enMessages=Object.fromEntries(groups.flatMap(([prefix,suffixes])=>keys(suffixes).map(key=>prefix+key)).map((key,index)=>[key,english[index]]));`,
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
