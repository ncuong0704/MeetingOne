import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  listDetectedSpeakers,
  mergeTargets,
  previewStopTime,
  shouldShowSpeakerButton,
  SPEAKER_PREVIEW_SECONDS,
} from './speakerPreview.ts';

test('preview window is 15 seconds', () => {
  assert.equal(SPEAKER_PREVIEW_SECONDS, 15);
});

test('previewStopTime adds 15 seconds and clamps to duration', () => {
  assert.equal(previewStopTime(12, 100), 27);
  assert.equal(previewStopTime(98, 100), 100);
  assert.equal(previewStopTime(0, 3), 3);
  assert.equal(previewStopTime(10, undefined), 25);
  assert.equal(previewStopTime(0, undefined), 15);
});

test('listDetectedSpeakers skips unlabeled segments and uses first appearance', () => {
  const speakers = listDetectedSpeakers([
    { speakerId: null, speakerName: null, speakerColor: null, timestamp: 1 },
    { speakerId: 'b', speakerName: 'Người nói 2', speakerColor: '#222', timestamp: 8 },
    { speakerId: 'a', speakerName: 'Người nói 1', speakerColor: '#111', timestamp: 4 },
    { speakerId: 'a', speakerName: 'Người nói 1', speakerColor: '#111', timestamp: 20 },
    { speakerId: 'b', speakerName: 'Người nói 2', speakerColor: '#222', timestamp: 2 },
  ]);

  assert.deepEqual(speakers, [
    {
      id: 'b',
      displayName: 'Người nói 2',
      color: '#222',
      previewStart: 2,
    },
    {
      id: 'a',
      displayName: 'Người nói 1',
      color: '#111',
      previewStart: 4,
    },
  ]);
});

test('mergeTargets excludes the source speaker', () => {
  const speakers = [
    { id: 'a', displayName: 'A', color: '#1', previewStart: 0 },
    { id: 'b', displayName: 'B', color: '#2', previewStart: 1 },
  ];
  assert.deepEqual(mergeTargets(speakers, 'a').map((s) => s.id), ['b']);
  assert.deepEqual(mergeTargets(speakers, 'missing'), speakers);
  assert.deepEqual(mergeTargets([speakers[0]], 'a'), []);
});

test('shouldShowSpeakerButton only when at least one speaker exists', () => {
  assert.equal(shouldShowSpeakerButton([]), false);
  assert.equal(
    shouldShowSpeakerButton([{ id: 'a', displayName: 'A', color: '#1', previewStart: 0 }]),
    true,
  );
});
