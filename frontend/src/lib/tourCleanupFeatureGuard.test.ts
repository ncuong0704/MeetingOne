import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const srcRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

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

test('mic quality evaluate button remains in DeviceSelection and RecordingControls', () => {
  const devices = readSrc('components/DeviceSelection.tsx');
  const controls = readSrc('components/RecordingControls.tsx');
  const dialog = readSrc('components/MicQualityDialog.tsx');
  const settings = readSrc('components/RecordingSettings.tsx');

  assert.match(devices, /MicQualityDialog/);
  assert.match(devices, /Đánh giá chất lượng microphone/);
  assert.match(devices, />\s*Đánh giá\s*</);
  assert.match(devices, /setQualityOpen\(true\)/);
  assert.match(devices, /min-w-0 flex-1/);
  assert.match(devices, /id="mic-selection"/);
  assert.match(controls, /MicQualityDialog/);
  assert.match(controls, />\s*Đánh giá\s*</);
  assert.match(controls, /hasMicrophoneAccess && wantsMicrophone/);
  assert.match(dialog, /Đánh giá Microphone/);
  assert.match(settings, /<DeviceSelection/);
});

test('tour leftovers are gone from frontend src, other sidebar and settings features remain', () => {
  const files = walkTsFiles(srcRoot);
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    assert.equal(text.includes('TOUR_TARGETS'), false, file);
    assert.equal(text.includes('data-tour'), false, file);
    assert.equal(text.includes('react-joyride'), false, file);
    assert.equal(text.includes('UserGuideJoyride'), false, file);
    assert.equal(text.includes('UserGuideProvider'), false, file);
    assert.equal(text.includes('TEMPLATE_TOUR_EVENT'), false, file);
    assert.equal(text.includes('SETTINGS_TOUR_TAB_EVENT'), false, file);
  }

  const sidebar = readSrc('components/Sidebar/index.tsx');
  assert.match(sidebar, /UserGuideButton/);
  assert.match(sidebar, /Bắt đầu ghi âm/);
  assert.match(sidebar, /Nhập file âm thanh/);
  assert.match(sidebar, /Cài đặt/);
  assert.match(sidebar, /openImportDialog/);

  const layout = readSrc('app/layout.tsx');
  assert.match(layout, /OnboardingFlow/);
  assert.match(layout, /ImportDialogProvider/);

  const templates = readSrc('components/TemplateSettings/TemplateList.tsx');
  assert.match(templates, /Tạo mới/);
  assert.match(templates, /Đặt mặc định/);

  const editor = readSrc('components/TemplateSettings/TemplateEditor.tsx');
  assert.match(editor, /onClone/);
  assert.match(editor, /onAddSection/);
  assert.match(editor, /Sao chép/);

  const prompt = readSrc('components/PromptSettings.tsx');
  assert.match(prompt, /PromptBlockNoteEditor/);

  const summary = readSrc('components/SummaryModelSettings.tsx');
  assert.match(summary, /Bật tóm tắt tự động/);

  const page = readSrc('app/page.tsx');
  assert.match(page, /<RecordingControls/);

  const guide = readSrc('components/UserGuide/UserGuideButton.tsx');
  assert.match(guide, /GUIDE_VIDEOS/);
  assert.match(guide, /Hướng dẫn/);
});
