import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  audioMimeType,
  classifyMediaPlaybackError,
  isMissingAudioInvokeError,
  prefersBlobPlayback,
} from './meetingAudioPlayback.ts';

test('imported 16 kHz WAV must load as a blob, live mp4 stays on convertFileSrc', () => {
  assert.equal(prefersBlobPlayback('C:\\Meetings\\foo\\audio.wav'), true);
  assert.equal(prefersBlobPlayback('/tmp/meeting/AUDIO.WAV'), true);
  assert.equal(prefersBlobPlayback('C:\\Meetings\\foo\\audio.mp4'), false);
  assert.equal(prefersBlobPlayback('/tmp/meeting/audio.m4a'), false);
});

test('audioMimeType matches the file extension', () => {
  assert.equal(audioMimeType('audio.wav'), 'audio/wav');
  assert.equal(audioMimeType('audio.mp4'), 'audio/mp4');
  assert.equal(audioMimeType('audio.mp3'), 'audio/mpeg');
});

test('invoke "No audio file found" is FILE_NOT_FOUND', () => {
  assert.equal(isMissingAudioInvokeError('No audio file found in: C:\\x'), true);
  assert.equal(isMissingAudioInvokeError('os error 2'), true);
  assert.equal(isMissingAudioInvokeError('permission denied'), false);
});

test('media error 4 after the file resolved is playback failure, not missing file', () => {
  // WebView2 often rejects PCM WAV served via convertFileSrc with MEDIA_ERR_SRC_NOT_SUPPORTED (4).
  assert.equal(classifyMediaPlaybackError(4, true), 'PLAYBACK_FAILED');
  assert.equal(classifyMediaPlaybackError(2, true), 'PLAYBACK_FAILED');
  assert.equal(classifyMediaPlaybackError(4, false), 'FILE_NOT_FOUND');
});
