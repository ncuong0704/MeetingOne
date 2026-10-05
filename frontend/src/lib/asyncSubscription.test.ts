import { test } from 'node:test';
import assert from 'node:assert/strict';
import { subscribeSafely } from './asyncSubscription.ts';

test('a registration finishing after unmount is immediately disposed', async () => {
  let resolve!: (cleanup: () => void) => void;
  let removed = 0;
  const registration = new Promise<() => void>((done) => { resolve = done; });
  const dispose = subscribeSafely(() => registration, (error) => { throw error; });
  dispose();
  resolve(() => { removed++; });
  await new Promise((done) => setImmediate(done));
  dispose();
  assert.equal(removed, 1);
});

test('an installed listener is removed once', async () => {
  let removed = 0;
  const dispose = subscribeSafely(async () => () => { removed++; }, (error) => { throw error; });
  await new Promise((done) => setImmediate(done));
  dispose();
  dispose();
  assert.equal(removed, 1);
});
