import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  audioMimeType,
  classifyMediaPlaybackError,
  isMissingAudioInvokeError,
  mediaSrcAllowsBlobPlayback,
  prefersBlobPlayback,
  shouldUseBlobPlayback,
} from './meetingAudioPlayback.ts';

test('imported 16 kHz WAV must load as a blob, live mp4 stays on convertFileSrc', () => {
  assert.equal(prefersBlobPlayback('C:\\Meetings\\foo\\audio.wav'), true);
  assert.equal(prefersBlobPlayback('/tmp/meeting/AUDIO.WAV'), true);
  assert.equal(prefersBlobPlayback('C:\\Meetings\\foo\\audio.mp4'), false);
  assert.equal(prefersBlobPlayback('/tmp/meeting/audio.m4a'), false);
});

test('long WAV skips blob so the player does not load hundreds of MB into JS', () => {
  assert.equal(shouldUseBlobPlayback('C:\\Meetings\\foo\\audio.wav', 8 * 1024 * 1024), true);
  assert.equal(shouldUseBlobPlayback('C:\\Meetings\\foo\\audio.wav', 33 * 1024 * 1024), false);
  assert.equal(shouldUseBlobPlayback('C:\\Meetings\\foo\\audio.mp4', 8 * 1024 * 1024), false);
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

test('blob: is required on media-src; asset: alone is not enough', () => {
  assert.equal(mediaSrcAllowsBlobPlayback("'self' asset: https://asset.localhost"), false);
  assert.equal(
    mediaSrcAllowsBlobPlayback("'self' asset: https://asset.localhost blob:"),
    true,
  );
});

test('production CSP allows blob media so imported WAV playback is not blocked', () => {
  const conf = JSON.parse(
    readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src-tauri', 'tauri.conf.json'),
      'utf8',
    ),
  ) as { app: { security: { csp: { 'media-src': string } } } };
  assert.equal(
    mediaSrcAllowsBlobPlayback(conf.app.security.csp['media-src']),
    true,
    'tauri:dev skips CSP; production applies media-src and will block blob: WAV playback without it',
  );
});
