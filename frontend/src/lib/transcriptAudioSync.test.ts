import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  maxLoadedEndTime,
  resolveActiveSegment,
  segmentEndTime,
} from './transcriptAudioSync.ts';
import type { TranscriptSegmentData } from '../types/index.ts';

function seg(
  partial: Partial<TranscriptSegmentData> & { id: string },
): TranscriptSegmentData {
  return {
    timestamp: 0,
    text: '',
    ...partial,
  };
}

test('segmentEndTime uses endTime or falls back to 5s after start', () => {
  assert.equal(segmentEndTime(seg({ id: 'a', timestamp: 10, endTime: 12 })), 12);
  assert.equal(segmentEndTime(seg({ id: 'b', timestamp: 10 })), 15);
  assert.equal(segmentEndTime(seg({ id: 'c' })), 5);
});

test('resolveActiveSegment returns null for empty list or negative time', () => {
  assert.equal(resolveActiveSegment([], 0), null);
  assert.equal(resolveActiveSegment([seg({ id: 'a', timestamp: 1 })], -0.1), null);
});

test('resolveActiveSegment highlights the in-window segment with the latest start', () => {
  const segments = [
    seg({ id: 'early', timestamp: 0, endTime: 10 }),
    seg({ id: 'overlap', timestamp: 4, endTime: 8 }),
    seg({ id: 'later', timestamp: 20, endTime: 25 }),
  ];
  assert.equal(resolveActiveSegment(segments, 5), 'overlap');
  assert.equal(resolveActiveSegment(segments, 9), 'early');
});

test('resolveActiveSegment picks the nearest upcoming segment before any window', () => {
  const segments = [
    seg({ id: 'first', timestamp: 3, endTime: 5 }),
    seg({ id: 'second', timestamp: 10, endTime: 12 }),
  ];
  assert.equal(resolveActiveSegment(segments, 0), 'first');
});

test('resolveActiveSegment keeps the last past segment after playback passes them', () => {
  const segments = [
    seg({ id: 'a', timestamp: 0, endTime: 2 }),
    seg({ id: 'b', timestamp: 3, endTime: 5 }),
  ];
  assert.equal(resolveActiveSegment(segments, 8), 'b');
});

test('maxLoadedEndTime is the largest segment end, including the 5s fallback', () => {
  assert.equal(maxLoadedEndTime([]), 0);
  assert.equal(
    maxLoadedEndTime([
      seg({ id: 'a', timestamp: 1, endTime: 3 }),
      seg({ id: 'b', timestamp: 10 }),
    ]),
    15,
  );
});
