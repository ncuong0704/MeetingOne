import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { formatMeetingListDate } from './formatMeetingListDate.ts';

const srcRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const now = new Date(2026, 7, 16, 15, 0, 0); // 16/08/2026 local

function localIso(y: number, mo: number, d: number, h: number, mi: number): string {
  return new Date(y, mo - 1, d, h, mi, 0).toISOString();
}

test('today uses Hôm nay and 24h time with padded minutes', () => {
  assert.equal(
    formatMeetingListDate(localIso(2026, 8, 16, 9, 30), now),
    'Hôm nay, 09:30',
  );
});

test('yesterday uses Hôm qua', () => {
  assert.equal(
    formatMeetingListDate(localIso(2026, 8, 15, 14, 0), now),
    'Hôm qua, 14:00',
  );
});

test('older dates use dd/mm/yyyy, HH:mm', () => {
  assert.equal(
    formatMeetingListDate(localIso(2026, 8, 12, 9, 15), now),
    '12/08/2026, 09:15',
  );
});

test('empty or invalid input returns empty string', () => {
  assert.equal(formatMeetingListDate(undefined, now), '');
  assert.equal(formatMeetingListDate('', now), '');
  assert.equal(formatMeetingListDate('not-a-date', now), '');
});

test('does not label a future calendar day as Hôm nay', () => {
  const label = formatMeetingListDate(localIso(2026, 8, 17, 8, 0), now);
  assert.equal(label.startsWith('Hôm nay'), false);
  assert.equal(label.startsWith('Hôm qua'), false);
  assert.match(label, /^\d{2}\/\d{2}\/\d{4}, \d{2}:\d{2}$/);
});

test('sidebar still maps created_at and renders the formatted date', () => {
  const provider = readFileSync(join(srcRoot, 'components/Sidebar/SidebarProvider.tsx'), 'utf8');
  const sidebar = readFileSync(join(srcRoot, 'components/Sidebar/index.tsx'), 'utf8');
  const meetingData = readFileSync(join(srcRoot, 'hooks/meeting-details/useMeetingData.ts'), 'utf8');
  const api = readFileSync(join(srcRoot, '../src-tauri/src/api/api.rs'), 'utf8');

  assert.match(api, /pub created_at: String/);
  assert.match(provider, /createdAt: meeting\.created_at/);
  assert.match(sidebar, /formatMeetingListDate\(item\.created_at\)/);
  assert.match(sidebar, /handleEditStart/);
  assert.match(sidebar, /setDeleteModalState/);
  assert.match(sidebar, /searchTranscripts/);
  assert.match(meetingData, /\{ \.\.\.m, title: meetingTitle \}/);
  assert.match(meetingData, /\{ \.\.\.m, title: newTitle \}/);
});
