import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { shouldSplashMeetingDetails } from './meetingDetailsLoadGate.ts';

const srcRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

test('keep the meeting view mounted after first load so dialogs survive refetch', () => {
  assert.equal(shouldSplashMeetingDetails(true), false);
  assert.equal(shouldSplashMeetingDetails(false), true);
});

test('meeting details page does not unmount on transcript refetch while details exist', () => {
  const page = readFileSync(join(srcRoot, 'app/meeting-details/page.tsx'), 'utf8');
  assert.match(page, /shouldSplashMeetingDetails/);
  assert.doesNotMatch(
    page,
    /if \(\(isLoading \|\| isLoadingTranscripts\) \|\| !meetingDetails\)/,
  );
});

test('paginated refetch reloads in place instead of resetting to a full-page loader', () => {
  const src = readFileSync(join(srcRoot, 'hooks/usePaginatedTranscripts.ts'), 'utf8');
  const start = src.indexOf('const refetch = useCallback');
  const end = src.indexOf('// Initial load');
  assert.ok(start >= 0 && end > start, 'refetch callback should exist');
  const body = src.slice(start, end);
  assert.doesNotMatch(body, /\breset\(\)/);
  assert.doesNotMatch(body, /setIsLoading\(true\)/);
});
