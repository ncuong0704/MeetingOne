import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  shouldOpenModelSelectorOnTranscriptionError,
  shouldStopRecordingOnTranscriptionError,
} from './sttError.ts';

test('keeps recording when transcription-error arrives mid-session', () => {
  assert.equal(shouldStopRecordingOnTranscriptionError(true), false);
  assert.equal(shouldStopRecordingOnTranscriptionError(false), true);
});

test('actionable transcription errors always open the model selector', () => {
  assert.equal(shouldOpenModelSelectorOnTranscriptionError(), true);
});
