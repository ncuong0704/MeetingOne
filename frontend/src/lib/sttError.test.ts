import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  isGeminiKeyOrQuotaError,
  shouldOpenModelSelectorOnTranscriptionError,
  shouldStopRecordingOnTranscriptionError,
} from './sttError.ts';

test('keeps recording when transcription-error arrives mid-session', () => {
  assert.equal(shouldStopRecordingOnTranscriptionError(true), false);
  assert.equal(shouldStopRecordingOnTranscriptionError(false), true);
});

test('gemini key and quota errors skip the model selector while recording', () => {
  assert.equal(
    shouldOpenModelSelectorOnTranscriptionError(
      true,
      'Gemini STT quota exhausted',
      'Hết hạn mức Gemini Transcribe Live. Kiểm tra quota API rồi thử lại.',
    ),
    false,
  );
  assert.equal(
    shouldOpenModelSelectorOnTranscriptionError(
      true,
      'Gemini STT authentication failed',
      'API key Gemini không hợp lệ hoặc bị từ chối. Kiểm tra key trong Cài đặt → Nhận dạng.',
    ),
    false,
  );
  assert.equal(
    shouldOpenModelSelectorOnTranscriptionError(
      false,
      'Gemini STT authentication failed',
      'API key Gemini không hợp lệ hoặc bị từ chối.',
    ),
    true,
  );
});

test('local model errors still open the selector even while recording', () => {
  assert.equal(
    shouldOpenModelSelectorOnTranscriptionError(
      true,
      'Streaming ASR model not loaded',
      'Model streaming chưa sẵn sàng. Vui lòng tải trong Cài đặt → Transcription.',
    ),
    true,
  );
  assert.equal(isGeminiKeyOrQuotaError('Streaming ASR model not loaded', 'Model streaming'), false);
});
