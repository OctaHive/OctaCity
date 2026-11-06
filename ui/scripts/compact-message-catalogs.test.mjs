import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { build } from 'vite';
import { compactMessageCatalogs } from './compact-message-catalogs.mjs';

test('compacts both catalogs while preserving message keys, escapes and Unicode', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'octacity-catalog-'));
  try {
    const directory = path.join(root, 'src/app/presentation');
    await mkdir(directory, { recursive: true });
    const repeatedEnglish = 'Review the retained result and confirm the selection.';
    const english = Object.fromEntries(
      Array.from({ length: 200 }, (_, index) => [
        `example.resultForRetainedArtifact${index}`,
        index === 0 ? '~example' : index % 2 ? repeatedEnglish : `Message ${index}`,
      ]),
    );
    const repeated = 'Прочитайте сохранённый результат и подтвердите выбор.';
    const russian = Object.fromEntries(
      Array.from({ length: 200 }, (_, index) => [
        `example.resultForRetainedArtifact${index}`,
        index === 0 ? '~42' : index % 2 ? repeated : `Текст ${index}`,
      ]),
    );
    const entries = (messages) => JSON.stringify(messages).slice(1, -1);
    await writeFile(
      path.join(directory, 'messages.en.ts'),
      `export const enMessages = {'factory.first.long.key': 'First', 'factory.second.long.key': 'Second', 'factory.2': 'Two', 'factory.1': 'One', 'factory.__proto__': 'Prototype | legacy',${entries(english)},'factory.first.long.last': 'Last'};`,
    );
    await writeFile(
      path.join(directory, 'messages.ru.ts'),
      `export const ruMessages = {'factory.second.long.key': 'Строка\\nдва', 'factory.first.long.key': 'Первый «текст»', 'factory.1': 'Один', 'factory.__proto__': 'Прототип', 'factory.2': 'Два',${entries(russian)},'factory.first.long.last': 'Последний'};`,
    );
    await writeFile(
      path.join(directory, 'entry.ts'),
      `export {enMessages} from './messages.en'; export {ruMessages} from './messages.ru';`,
    );
    const result = await build({
      configFile: false,
      root,
      logLevel: 'silent',
      plugins: [compactMessageCatalogs()],
      build: {
        write: false,
        minify: false,
        rolldownOptions: {
          input: path.join(directory, 'entry.ts'),
          preserveEntrySignatures: 'strict',
        },
      },
    });
    assert(!Array.isArray(result) && 'output' in result);
    const code = result.output.find((item) => item.type === 'chunk' && item.isEntry)?.code;
    assert(code);
    const bytes = Buffer.byteLength(code);
    assert(bytes < 8500, `packed catalog exceeds 8500 bytes: ${bytes}`);
    assert.equal(code.split(repeated).length - 1, 1);
    assert.equal(code.split(repeatedEnglish).length - 1, 1);
    assert.equal(code.split('resultForRetainedArtifact').length - 1, 1);
    const { enMessages, ruMessages } = await import(
      `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
    );
    assert.deepEqual(enMessages, {
      'factory.first.long.key': 'First',
      'factory.second.long.key': 'Second',
      'factory.2': 'Two',
      'factory.1': 'One',
      'factory.__proto__': 'Prototype | legacy',
      ...english,
      'factory.first.long.last': 'Last',
    });
    assert.deepEqual(ruMessages, {
      'factory.first.long.key': 'Первый «текст»',
      'factory.second.long.key': 'Строка\nдва',
      'factory.2': 'Два',
      'factory.1': 'Один',
      'factory.__proto__': 'Прототип',
      ...russian,
      'factory.first.long.last': 'Последний',
    });
    assert.deepEqual(
      Object.keys(enMessages).filter((key) => /^factory\.[^.]+$/.test(key)),
      ['factory.2', 'factory.1', 'factory.__proto__'],
    );
    assert.deepEqual(Object.keys(ruMessages), Object.keys(enMessages));
    assert.equal(code.includes('factory.first.long.key'), false);
    assert.equal(code.split('"factory."').length - 1, 1);
    assert.equal(code.split('"factory.first.long."').length - 1, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
