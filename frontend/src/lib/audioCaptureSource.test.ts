import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  parseAudioCaptureSource,
  wantsMicrophone,
  wantsSystem,
  canRecordWithDevices,
  effectiveAudioSource,
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

test('canRecordWithDevices needs speaker output or a usable mic', () => {
  assert.equal(canRecordWithDevices(false, false), false);
  assert.equal(canRecordWithDevices(true, false), true);
  assert.equal(canRecordWithDevices(false, true), true);
  assert.equal(canRecordWithDevices(true, true), true);
});

test('effectiveAudioSource falls back to system when mic is unavailable', () => {
  assert.equal(effectiveAudioSource('both', false), 'system');
  assert.equal(effectiveAudioSource('system', false), 'system');
  assert.equal(effectiveAudioSource('both', true), 'both');
  const micOnly = effectiveAudioSource('microphone', false);
  assert.equal(typeof micOnly, 'object');
  assert.equal('error' in (micOnly as object) && (micOnly as { error: string }).error.includes('microphone'), true);
});
