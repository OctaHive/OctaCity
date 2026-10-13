import { expect, it } from 'vitest';
import browserConfig from '../../playwright.config';
import config from '../../vite.config';

it('bounds unit-test workers to avoid exhausting the host while Rust and browser checks run', () => {
  expect(config.test?.maxWorkers).toBe(4);
});

it('bounds concurrent browser workers while preserving every test and screenshot assertion', () => {
  expect(browserConfig.workers).toBe(2);
});
