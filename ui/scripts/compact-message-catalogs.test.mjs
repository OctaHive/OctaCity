import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { build } from 'vite';
import { compactMessageCatalogs } from './compact-message-catalogs.mjs';

test('compacts translated keys in production while preserving reordered keys, escapes and Unicode', async () => {
  const root = await mkdtemp(path.join(tmpdir(), 'octacity-catalog-'));
  try {
    const directory = path.join(root, 'src/app/presentation');
    await mkdir(directory, { recursive: true });
    await writeFile(
      path.join(directory, 'messages.en.ts'),
      `export const enMessages = {'factory.first.long.key': 'First', 'factory.second.long.key': 'Second'};`,
    );
    await writeFile(
      path.join(directory, 'messages.ru.ts'),
      `export const ruMessages = {'factory.second.long.key': 'Строка\\nдва', 'factory.first.long.key': 'Первый «текст»'};`,
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
    const { ruMessages } = await import(
      `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
    );
    assert.deepEqual(ruMessages, {
      'factory.first.long.key': 'Первый «текст»',
      'factory.second.long.key': 'Строка\nдва',
    });
    assert.equal(code.split('factory.first.long.key').length - 1, 1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
