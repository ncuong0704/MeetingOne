import { test } from 'node:test';
import assert from 'node:assert/strict';
import { emptySpeakerHotkeys } from './speakerHotkeys.ts';

test('emptySpeakerHotkeys has slots 1-9 and no extras', () => {
  const slots = emptySpeakerHotkeys();
  assert.equal(Object.keys(slots).length, 9);
  for (let i = 1; i <= 9; i += 1) {
    assert.equal(slots[String(i)], '');
  }
  assert.equal(slots['0'], undefined);
  assert.equal(slots['10'], undefined);
});
