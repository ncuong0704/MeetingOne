import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  parseAudioCaptureSource,
  wantsMicrophone,
  wantsSystem,
} from './audioCaptureSource.ts';

test('parseAudioCaptureSource keeps valid values and defaults to both', () => {
  assert.equal(parseAudioCaptureSource('microphone'), 'microphone');
  assert.equal(parseAudioCaptureSource('system'), 'system');
  assert.equal(parseAudioCaptureSource('both'), 'both');
  assert.equal(parseAudioCaptureSource(undefined), 'both');
  assert.equal(parseAudioCaptureSource('nope'), 'both');
});

test('wantsMicrophone is true except system-only capture', () => {
  assert.equal(wantsMicrophone('microphone'), true);
  assert.equal(wantsMicrophone('both'), true);
  assert.equal(wantsMicrophone('system'), false);
});

test('wantsSystem is true except microphone-only capture', () => {
  assert.equal(wantsSystem('system'), true);
  assert.equal(wantsSystem('both'), true);
  assert.equal(wantsSystem('microphone'), false);
});
