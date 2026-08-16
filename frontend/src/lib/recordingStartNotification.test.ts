import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const srcRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const rustLib = join(srcRoot, '../src-tauri/src/lib.rs');

function readSrc(rel: string): string {
  return readFileSync(join(srcRoot, rel), 'utf8');
}

function walkTsFiles(dir: string, acc: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      walkTsFiles(full, acc);
    } else if (/\.(ts|tsx)$/.test(name) && !/\.test\.(ts|tsx)$/.test(name)) {
      acc.push(full);
    }
  }
  return acc;
}

test('start-recording notification helper is gone from frontend src', () => {
  assert.equal(existsSync(join(srcRoot, 'lib/recordingNotification.tsx')), false);
  for (const file of walkTsFiles(srcRoot)) {
    const text = readFileSync(file, 'utf8');
    assert.equal(text.includes('showRecordingNotification'), false, file);
    assert.equal(text.includes('Thông báo khi bắt đầu ghi'), false, file);
    assert.equal(text.includes('Đã bắt đầu ghi âm'), false, file);
  }
});

test('recording start flow still wires ASR check, backend start, and UI state', () => {
  const start = readSrc('hooks/useRecordingStart.ts');
  assert.match(start, /handleRecordingStart/);
  assert.match(start, /checkAsrReady/);
  assert.match(start, /startBackendRecording\(/);
  assert.match(start, /recordingService/);
  assert.match(start, /setIsRecording\(true\)/);
  assert.match(start, /clearTranscripts\(\)/);
  assert.match(start, /setIsMeetingActive\(true\)/);
  assert.match(start, /start_recording', 'home_page'/);
  assert.match(start, /start_recording', 'sidebar_auto'/);
  assert.match(start, /start_recording', 'sidebar_direct'/);
  assert.match(start, /autoStartRecording/);

  const controls = readSrc('components/RecordingControls.tsx');
  assert.match(controls, /onRecordingStart/);
  assert.match(controls, /Bắt đầu ghi âm/);

  const page = readSrc('app/page.tsx');
  assert.match(page, /<RecordingControls/);
  assert.match(page, /handleRecordingStart/);
});

test('recording settings keep save/device options without start-notification toggle', () => {
  const settings = readSrc('components/RecordingSettings.tsx');
  assert.equal(settings.includes('show_recording_notification'), false);
  assert.equal(settings.includes('handleNotificationToggle'), false);
  assert.match(settings, /auto_save/);
  assert.match(settings, /<DeviceSelection/);
  assert.match(settings, /open_recordings_folder/);
  assert.match(settings, /select_recording_folder/);
});

test('rust start_recording no longer shows a started OS notification', () => {
  const lib = readFileSync(rustLib, 'utf8');
  assert.equal(lib.includes('show_recording_started_notification'), false);
  assert.match(lib, /start_recording_with_devices_and_meeting/);
  assert.match(lib, /RECORDING_FLAG\.store\(true/);
  assert.match(lib, /show_recording_stopped_notification/);
});
